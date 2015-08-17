// Copyright 2015 MaidSafe.net limited.
//
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.0.  This, along with the
// Licenses can be found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

use std::sync::mpsc;
use std::thread::spawn;
use std::collections::BTreeMap;
use sodiumoxide::crypto::sign::{verify_detached, Signature};
use sodiumoxide::crypto::sign;
use sodiumoxide::crypto;
use time::{Duration, SteadyTime};
use std::cmp::min;

use crust;
use crust::{ConnectionManager, Endpoint, Port};
use lru_time_cache::LruCache;

use action::Action;
use event::Event;
use NameType;
use name_type::{closer_to_target_or_equal};
use routing_core::{RoutingCore, ConnectionName};
use id::Id;
use public_id::PublicId;
use hello::Hello;
use types;
use types::{MessageId, Bytes, Address};
use utils::{encode, decode};
use utils;
use data::{Data, DataRequest};
use authority::{Authority, our_authority};
use wake_up::WakeUpCaller;

use messages::{RoutingMessage,
               SignedMessage, SignedToken,
               ConnectRequest,
               ConnectResponse,
               Content,
               ExternalRequest, ExternalResponse,
               InternalRequest, InternalResponse };

use error::{RoutingError, ResponseError};
use refresh_accumulator::RefreshAccumulator;
use message_filter::MessageFilter;
use message_accumulator::MessageAccumulator;


type RoutingResult = Result<(), RoutingError>;

static MAX_BOOTSTRAP_CONNECTIONS : usize = 1;
static MAX_CRUST_EVENT_COUNTER : u8 = 10;
/// Routing Node
pub struct RoutingNode {
    // for CRUST
    crust_receiver      : mpsc::Receiver<crust::Event>,
    connection_manager  : crust::ConnectionManager,
    accepting_on        : Vec<crust::Endpoint>,
    // for RoutingNode
    client_restriction  : bool,
    action_sender       : mpsc::Sender<Action>,
    action_receiver     : mpsc::Receiver<Action>,
    event_sender        : mpsc::Sender<Event>,
    wakeup              : WakeUpCaller,
    filter              : MessageFilter<types::FilterType>,
    core                : RoutingCore,
    public_id_cache     : LruCache<NameType, PublicId>,
    connection_cache    : BTreeMap<NameType, SteadyTime>,
    accumulator         : MessageAccumulator,
    // refresh_accumulator : RefreshAccumulator,
}

impl RoutingNode {
    pub fn new(action_sender      : mpsc::Sender<Action>,
               action_receiver    : mpsc::Receiver<Action>,
               event_sender       : mpsc::Sender<Event>,
               client_restriction : bool) -> RoutingNode {

        let (crust_sender, crust_receiver) = mpsc::channel::<crust::Event>();
        let mut cm = crust::ConnectionManager::new(crust_sender);
        let _ = cm.start_accepting(vec![Port::Tcp(5483u16)]);
        let accepting_on = cm.get_own_endpoints();

        let core = RoutingCore::new(event_sender.clone());
        debug!("RoutingNode {:?} listens on {:?}", core.our_address(), accepting_on);

        RoutingNode {
            crust_receiver      : crust_receiver,
            connection_manager  : cm,
            accepting_on        : accepting_on,
            client_restriction  : client_restriction,
            action_sender       : action_sender.clone(),
            action_receiver     : action_receiver,
            event_sender        : event_sender,
            wakeup              : WakeUpCaller::new(action_sender),
            filter              : MessageFilter::with_expiry_duration(Duration::minutes(20)),
            core                : core,
            public_id_cache     : LruCache::with_expiry_duration(Duration::minutes(10)),
            connection_cache    : BTreeMap::new(),
            accumulator         : MessageAccumulator::new(),
        }
    }

    #[allow(unused_assignments)]
    pub fn run(&mut self) {
        let mut crust_event_counter : u8 = 0;
        self.wakeup.start(10);
        self.connection_manager.bootstrap(MAX_BOOTSTRAP_CONNECTIONS);
        debug!("RoutingNode started running and started bootstrap");
        loop {
            match self.action_receiver.recv() {
                Err(_) => {},
                Ok(Action::SendMessage(signed_message)) => {
                    ignore(self.message_received(signed_message));
                },
                Ok(Action::SendContent(to_authority, content)) => {
                    let _ = self.send_content(to_authority, content);
                },
                Ok(Action::WakeUp) => {
                    // ensure that the loop is blocked for maximally 10ms
                },
                Ok(Action::Terminate) => {
                    debug!("routing node terminated");
                    self.connection_manager.stop();
                    break;
                },
            };
            loop {
                crust_event_counter = 0;
                match self.crust_receiver.try_recv() {
                    Err(_) => {
                        // FIXME (ben 16/08/2015) other reasons could induce an error
                        // main error assumed now to be no new crust events
                        break;
                    },
                    Ok(crust::Event::NewMessage(endpoint, bytes)) => {
                        match decode::<SignedMessage>(&bytes) {
                            Ok(message) => {
                                // handle SignedMessage for any identified endpoint
                                match self.core.lookup_endpoint(&endpoint) {
                                    Some(ConnectionName::Unidentified(_, _)) => debug!("message
                                    from unidentified connection"),
                                    None => debug!("message from unknown endpoint"),
                                    _ => ignore(self.message_received(message)),
                                };
                            },
                            // The message received is not a Signed Routing Message,
                            // expect it to be an Hello message to identify a connection
                            Err(_) => {
                                let _ = self.handle_hello(&endpoint, bytes);
                            },
                        };
                    },
                    Ok(crust::Event::NewConnection(endpoint)) => {
                        self.handle_new_connection(endpoint);
                    },
                    Ok(crust::Event::LostConnection(endpoint)) => {
                        self.handle_lost_connection(endpoint);
                    },
                    Ok(crust::Event::NewBootstrapConnection(endpoint)) => {
                        self.handle_new_bootstrap_connection(endpoint);
                    }
                };
                crust_event_counter += 1;
                if crust_event_counter >= MAX_CRUST_EVENT_COUNTER {
                    debug!("Breaking to yield to Actions.");
                    break; };
            }
        }
    }

    /// When CRUST receives a connect to our listening port and establishes a new connection,
    /// the endpoint is given here as new connection
    fn handle_new_connection(&mut self, endpoint : Endpoint) {
        debug!("New connection on {:?}", endpoint);
        // only accept new connections if we are a full node
        // FIXME(dirvine) I am not sure we should not accept connections here :16/08/2015
        let has_bootstrap_endpoints = self.core.has_bootstrap_endpoints();
        if !self.core.is_node() {
            if has_bootstrap_endpoints {
                // we are bootstrapping, refuse all normal connections
                self.connection_manager.drop_node(endpoint);
                return;
            } else {
                let assigned_name = NameType::new(crypto::hash::sha512::hash(
                    &self.core.id().name().0).0);
                let _ = self.core.assign_name(&assigned_name);
            }
        }

        if !self.core.add_peer(ConnectionName::Unidentified(endpoint.clone(), false),
            endpoint.clone(), None) {
            // only fails if relay_map is full for unidentified connections
            self.connection_manager.drop_node(endpoint.clone());
        }
        ignore(self.send_hello(endpoint, None));
    }

    /// When CRUST reports a lost connection, ensure we remove the endpoint anywhere
    fn handle_lost_connection(&mut self, endpoint : Endpoint) {
        debug!("Lost connection on {:?}", endpoint);
        let connection_name = self.core.lookup_endpoint(&endpoint);
          if connection_name.is_some() { self.core.drop_peer(&connection_name.unwrap()); }
    }

    fn handle_new_bootstrap_connection(&mut self, endpoint : Endpoint) {
        debug!("New bootstrap connection on {:?}", endpoint);
        if !self.core.is_node() {
            if !self.core.add_peer(ConnectionName::Unidentified(endpoint.clone(), true),
                endpoint.clone(), None) {
                // only fails if relay_map is full for unidentified connections
                error!("New bootstrap connection on {:?} failed to be labeled as unidentified",
                    endpoint);
                self.connection_manager.drop_node(endpoint.clone());
                return;
            }
        } else {
            // if core is a full node, don't accept new bootstrap connections
            error!("New bootstrap connection on {:?} but we are a node",
                endpoint);
            self.connection_manager.drop_node(endpoint);
            return;
        }
        ignore(self.send_hello(endpoint, None));
    }

    // ---- Hello connection identification -------------------------------------------------------

    fn send_hello(&mut self, endpoint: Endpoint, confirmed_address : Option<Address>)
        -> RoutingResult {
        let message = try!(encode(&Hello {
            address       : self.core.our_address(),
            public_id     : PublicId::new(self.core.id()),
            confirmed_you : confirmed_address.clone()}));
        debug!("Saying hello I am {:?} on {:?}, confirming {:?}", self.core.our_address(),
            endpoint, confirmed_address);
        ignore(self.connection_manager.send(endpoint, message));
        Ok(())
    }

    fn handle_hello(&mut self, endpoint: &Endpoint, serialised_message: Bytes)
        -> RoutingResult {
        match decode::<Hello>(&serialised_message) {
            Ok(hello) => {
                debug!("Hello, it is {:?} on {:?}", hello.address, endpoint);
                let old_identity = match self.core.lookup_endpoint(&endpoint) {
                    // if already connected through the routing table, just confirm or destroy
                    Some(ConnectionName::Routing(known_name)) => {
                        debug!("Endpoint {:?} registered to routing node {:?}", endpoint,
                            known_name);
                        match hello.address {
                            // FIXME (ben 11/08/2015) Hello messages need to be signed and
                            // we also need to check the match with the PublicId stored in RT
                            Address::Node(known_name) =>
                                return Ok(()),
                            _ => {
                                // the endpoint does not match with the routing information
                                // we know about it; drop it
                                let _ = self.core.drop_peer(&ConnectionName::Routing(known_name));
                                self.connection_manager.drop_node(endpoint.clone());
                                return Err(RoutingError::RejectedPublicId);
                            },
                        }
                    },
                    // a connection should have been labeled as Unidentified
                    None => None,
                    Some(relay_connection_name) => Some(relay_connection_name),
                };
                // FIXME (ben 14/08/2015) temporary copy until Debug is
                // implemented for ConnectionName
                let hello_address = hello.address.clone();
                // if set to true we will take the initiative to drop the connection,
                // if refused from core;
                // if alpha is false we will leave the connection unidentified,
                // only adding the new identity when it is confirmed by the other side
                // (hello.confirmed_you set to our address), which has to send a confirmed hello
                let mut alpha = false;
                // construct the new identity from Hello
                let new_identity = match (hello.address, self.core.our_address()) {
                    (Address::Node(his_name), Address::Node(our_name)) => {
                    // He is a node, and we are a node, establish a routing table connection
                    // FIXME (ben 11/08/2015) we need to check his PublicId against the network
                    // but this requires an additional RFC so currently leave out such check
                    // refer to https://github.com/maidsafe/routing/issues/387
                        alpha = &self.core.id().name() < &his_name;
                        ConnectionName::Routing(his_name)
                    },
                    (Address::Client(his_public_key), Address::Node(our_name)) => {
                    // He is a client, we are a node, establish a relay connection
                        debug!("Connection {:?} will be labeled as a relay to {:?}",
                            endpoint, Address::Client(his_public_key));
                        alpha = true;
                        ConnectionName::Relay(Address::Client(his_public_key))
                    },
                    (Address::Node(his_name), Address::Client(our_public_key)) => {
                    // He is a node, we are a client, establish a bootstrap connection
                        debug!("Connection {:?} will be labeled as a bootstrap node name {:?}",
                            endpoint, his_name);
                        ConnectionName::Bootstrap(his_name)
                    },
                    (Address::Client(his_public_key), Address::Client(our_public_key)) => {
                    // He is a client, we are a client, no-go
                        match old_identity {
                            Some(old_connection_name) => {
                                let _ = self.core.drop_peer(&old_connection_name); },
                            None => {},
                        };
                        self.connection_manager.drop_node(endpoint.clone());
                        return Err(RoutingError::BadAuthority);
                    },
                };
                let confirmed = match hello.confirmed_you {
                    Some(address) => {
                        if self.core.is_us(&address) {
                            debug!("This hello message successfully confirmed our address, {:?}",
                                address);
                            true
                        } else {
                            self.connection_manager.drop_node(endpoint.clone());
                            error!("Wrongfully confirmed as {:?} on {:?} and dropped the connection",
                                address, endpoint);
                            return Err(RoutingError::RejectedPublicId);
                        }
                    },
                    None => false,
                };
                if alpha || confirmed {
                    // we know it's not a routing connection, remove it from the relay map
                    let dropped_peer = match &old_identity {
                        &Some(ConnectionName::Routing(_)) => unreachable!(),
                        // drop any relay connection in favour of new to-be-determined identity
                        &Some(ref old_connection_name) => {
                            self.core.drop_peer(old_connection_name)
                        },
                        &None => None,
                    };
                    // add the new identity, or drop the connection
                    if self.core.add_peer(new_identity.clone(), endpoint.clone(),
                        Some(hello.public_id)) {
                        debug!("Added {:?} to the core on {:?}", hello_address, endpoint);
                        if alpha {
                            ignore(self.send_hello(endpoint.clone(), Some(hello_address)));
                        };
                        match new_identity {
                            ConnectionName::Bootstrap(bootstrap_name) => {
                                ignore(self.request_network_name(&bootstrap_name, endpoint));
                            },
                            _ => {},
                        };
                    } else {
                        // depending on the identity of the connection, follow the rules on dropping
                        // to avoid both sides drop the other connection, possibly leaving none
                        self.connection_manager.drop_node(endpoint.clone());
                        debug!("Core refused {:?} on {:?} and dropped the connection",
                            hello_address, endpoint);
                    };
                } else {
                    debug!("We are not alpha and the hello was not confirmed yet, awaiting alpha.");
                }
                Ok(())
            },
            Err(_) => Err(RoutingError::UnknownMessageType)
        }
    }


    /// This the fundamental functional function in routing.
    /// It only handles messages received from connections in our routing table;
    /// i.e. this is a pure SAFE message (and does not function as the start of a relay).
    /// If we are the relay node for a message from the SAFE network to a node we relay for,
    /// then we will pass out the message to the client or bootstrapping node;
    /// no relay-messages enter the SAFE network here.
    fn message_received(&mut self, message_wrap : SignedMessage) -> RoutingResult {

        let message = message_wrap.get_routing_message().clone();

        // filter check
        if self.filter.check(message_wrap.signature()) {
            // should just return quietly
            return Err(RoutingError::FilterCheckFailed);
        }
        debug!("message {:?} from {:?} to {:?}", message.content,
            message.source(), message.destination());
        // add to filter
        self.filter.add(message_wrap.signature().clone());

        // Forward
        if self.core.is_connected_node() { ignore(self.send(message_wrap.clone())); }

        // if !self.core.name_in_range(&message.destination().get_location()) {
        //     debug!("Not for us, destination {:?} out of range",
        //         message.destination().get_location());
        //     return Ok(()); };

        // check if our calculated authority matches the destination authority of the message
        if self.core.our_authority(&message)
            .map(|our_auth| message.to_authority != our_auth).unwrap_or(false) {
            debug!("Destination authority {:?} is while our authority is {:?}",
                message.to_authority, self.core.our_authority(&message));
            return Err(RoutingError::BadAuthority);
        }

        // Accumulate message
        let (message, opt_token) = match self.accumulate(message_wrap) {
            Some((message, opt_token)) => (message, opt_token),
            None => return Ok(()),
        };

        match message.content {
            Content::InternalRequest(request) => {
                match request {
                    InternalRequest::Connect(_) => {
                        match opt_token {
                            Some(response_token) => self.handle_connect_request(request,
                                message.from_authority, message.to_authority, response_token),
                            None => return Err(RoutingError::UnknownMessageType),
                        }
                    },
                    InternalRequest::RequestNetworkName(_) => {
                        match opt_token {
                            Some(response_token) => self.handle_request_network_name(request,
                                message.from_authority, message.to_authority, response_token),
                            None => return Err(RoutingError::UnknownMessageType),
                        }
                    },
                    InternalRequest::CacheNetworkName(_, _) => {
                        self.handle_cache_network_name(request, message.from_authority,
                            message.to_authority)
                    },
                    InternalRequest::Refresh(_, _) => {
                        Ok(())
                        // TODO (ben 13/08/2015) implement self.handle_refresh()
                    },
                }
            },
            Content::InternalResponse(response) => {
                match response {
                    InternalResponse::Connect(_, _) => {
                        self.handle_connect_response(response, message.from_authority,
                            message.to_authority)
                    },
                    InternalResponse::CacheNetworkName(_, _, _) => {
                        self.handle_cache_network_name_response(response, message.from_authority,
                            message.to_authority)
                    }
                }
            },
            Content::ExternalRequest(request) => {
                self.send_to_user(Event::Request {
                    request        : request,
                    our_authority  : message.to_authority,
                    from_authority : message.from_authority,
                    response_token : opt_token,
                });
                Ok(())
            }
            Content::ExternalResponse(response) => {
                self.handle_external_response(response, message.to_authority,
                    message.from_authority)
            }
        }
    }

    fn accumulate(&mut self, signed_message: SignedMessage)
        -> Option<(RoutingMessage, Option<SignedToken>)> {
        let message = signed_message.get_routing_message().clone();

        if !message.from_authority.is_group() {
            debug!("Message didn't come from a group ({:?}), returning with SignedToken",
                message.from_authority);
            // TODO: If not from a group, then use client's public key to check
            // the signature.
            let token = match signed_message.as_token() {
                Ok(token) => token,
                Err(_)    => {
                  error!("Failed to generate signed token, message {:?} is dropped.",
                      message);
                  return None; },
            };
            return Some((message, Some(token)));
        }

        let skip_accumulator = match message.content {
            Content::InternalResponse(ref response) => {
                match *response {
                    InternalResponse::CacheNetworkName(_,_,_) => true,
                    _ => false,
                }
            },
            _ => false
        };

        if skip_accumulator {
            debug!("Skipping accumulator for message {:?}", message);
            return Some((message, None)); }

        let threshold = min(types::GROUP_SIZE,
                            (self.core.routing_table_size() as f32 * 0.8) as usize);
        debug!("Accumulator threshold is at {:?}", threshold);

        let claimant : NameType = match *signed_message.claimant() {
            Address::Node(ref claimant) => claimant.clone(),
            Address::Client(_) => {
                error!("Claimant is a Client, but passed into accumulator for a group. dropped.");
                debug_assert!(false);
                return None;
            }
        };

        self.accumulator.add_message(threshold as usize, claimant, message)
                        .map(|msg| (msg, None))
    }

    // ---- Request Network Name ------------------------------------------------------------------

    fn request_network_name(&mut self, bootstrap_name : &NameType,
        bootstrap_endpoint : &Endpoint) -> RoutingResult {
        debug!("Will request a network name from bootstrap node {:?} on {:?}", bootstrap_name,
            bootstrap_endpoint);
        // if RoutingNode is restricted from becoming a node,
        // it suffices to never request a network name.
        if self.client_restriction { return Ok(()) }
        if self.core.is_node() { return Err(RoutingError::AlreadyConnected); };
        let core_id = self.core.id();
        let routing_message = RoutingMessage {
            from_authority : Authority::Client(bootstrap_name.clone(),
                core_id.signing_public_key()),
            to_authority   : Authority::NaeManager(core_id.name()),
            content        : Content::InternalRequest(InternalRequest::RequestNetworkName(
                PublicId::new(core_id))),
        };
        match SignedMessage::new(Address::Client(core_id.signing_public_key()),
            routing_message, core_id.signing_private_key()) {
            Ok(signed_message) => ignore(self.send(signed_message)),
            Err(e) => return Err(RoutingError::Cbor(e)),
        };
        Ok(())
    }

    fn handle_request_network_name(&self, request        : InternalRequest,
                                          from_authority : Authority,
                                          to_authority   : Authority,
                                          response_token : SignedToken) -> RoutingResult {
        match request {
            InternalRequest::RequestNetworkName(public_id) => {
                match (&from_authority, &to_authority) {
                    (&Authority::Client(_, ref public_key), &Authority::NaeManager(name)) => {
                        let mut network_public_id = public_id.clone();
                        match self.core.our_close_group() {
                            Some(close_group) => {
                                let relocated_name = try!(utils::calculate_relocated_name(
                                    close_group, &public_id.name()));
                                debug!("Got a request for a network name from {:?}, assigning {:?}",
                                    from_authority, relocated_name);
                                network_public_id.assign_relocated_name(relocated_name.clone());
                                let routing_message = RoutingMessage {
                                    from_authority : to_authority,
                                    to_authority   : Authority::NaeManager(relocated_name.clone()),
                                    content        : Content::InternalRequest(
                                        InternalRequest::CacheNetworkName(network_public_id,
                                        response_token)),
                                };
                                match SignedMessage::new(Address::Node(self.core.id().name()),
                                    routing_message, self.core.id().signing_private_key()) {
                                    Ok(signed_message) => ignore(self.send(signed_message)),
                                    Err(e) => return Err(RoutingError::Cbor(e)),
                                };
                                Ok(())
                            },
                            None => return Err(RoutingError::BadAuthority),
                        }
                    },
                    _ => return Err(RoutingError::BadAuthority),
                }
            },
            _ => return Err(RoutingError::BadAuthority),
        }
    }

    fn handle_cache_network_name(&mut self, request        : InternalRequest,
                                            from_authority : Authority,
                                            to_authority   : Authority,
                                            ) -> RoutingResult {
        match request {
            InternalRequest::CacheNetworkName(network_public_id, response_token) => {
                match (from_authority, &to_authority) {
                    (Authority::NaeManager(from_name), &Authority::NaeManager(name)) => {
                        let request_network_name = try!(SignedMessage::new_from_token(
                            response_token.clone()));
                        let _ = self.public_id_cache.insert(network_public_id.name(),
                            network_public_id.clone());
                        match self.core.our_close_group_with_public_ids() {
                            Some(close_group) => {
                                debug!("Network request to accept name {:?},
                                    responding with our close group to {:?}", network_public_id.name(),
                                    request_network_name.get_routing_message().source());
                                let routing_message = RoutingMessage {
                                    from_authority : to_authority,
                                    to_authority   : request_network_name.get_routing_message().source(),
                                    content        : Content::InternalResponse(
                                        InternalResponse::CacheNetworkName(network_public_id,
                                        close_group, response_token)),
                                };
                                match SignedMessage::new(Address::Node(self.core.id().name()),
                                    routing_message, self.core.id().signing_private_key()) {
                                    Ok(signed_message) => ignore(self.send(signed_message)),
                                    Err(e) => return Err(RoutingError::Cbor(e)),
                                };
                                Ok(())
                            },
                            None => return Err(RoutingError::BadAuthority),
                        }
                    },
                    _ => return Err(RoutingError::BadAuthority),
                }
            },
            _ => return Err(RoutingError::BadAuthority),
        }
    }

    fn handle_cache_network_name_response(&mut self,
                                          response       : InternalResponse,
                                          from_authority : Authority,
                                          to_authority   : Authority) -> RoutingResult {
        // An additional blockage on acting to restrict RoutingNode from becoming a full node
        if self.client_restriction { return Ok(()) };
        match response {
            InternalResponse::CacheNetworkName(network_public_id, group, signed_token) => {
                if !signed_token.verify_signature(&self.core.id().signing_public_key()) {
                    return Err(RoutingError::FailedSignature)};
                let request = try!(SignedMessage::new_from_token(signed_token));
                match request.get_routing_message().content {
                    Content::InternalRequest(InternalRequest::RequestNetworkName(ref original_public_id)) => {
                        let mut our_public_id = PublicId::new(self.core.id());
                        if &our_public_id != original_public_id { return Err(RoutingError::BadAuthority); };
                        our_public_id.set_name(network_public_id.name());
                        if our_public_id != network_public_id { return Err(RoutingError::BadAuthority); };
                        let _ = self.core.assign_network_name(&network_public_id.name());
                        debug!("Assigned network name {:?} and our address now is {:?}",
                            network_public_id.name(), self.core.our_address());
                        for peer in group {
                            // TODO (ben 12/08/2015) self.public_id_cache.insert()
                            // or hold off till RFC on removing public_id_cache
                            self.refresh_routing_table(peer.name());
                        }
                        Ok(())
                    },
                    _ => return Err(RoutingError::UnknownMessageType),
                }
            },
            _ => return Err(RoutingError::BadAuthority),
        }
    }

    // ---- Connect Requests and Responses --------------------------------------------------------

    /// Scan all passing messages for the existance of nodes in the address space.
    /// If a node is detected with a name that would improve our routing table,
    /// then try to connect.  During a delay of 5 seconds, we collapse
    /// all re-occurances of this name, and block a new connect request
    /// TODO: The behaviour of this function has been adapted to serve as a filter
    /// to cover for the lack of a filter on FindGroupResponse
    fn refresh_routing_table(&mut self, from_node : NameType) {
      // disable refresh when scanning on small routing_table size
      let time_now = SteadyTime::now();
      if !self.connection_cache.contains_key(&from_node) {
          if self.core.check_node(&ConnectionName::Routing(from_node)) {
              ignore(self.send_connect_request(&from_node));
          }
          self.connection_cache.entry(from_node.clone())
              .or_insert(time_now);
       }

       let mut prune_blockage : Vec<NameType> = Vec::new();
       for (blocked_node, time) in self.connection_cache.iter_mut() {
           // clear block for nodes
           if time_now - *time > Duration::seconds(10) {
               prune_blockage.push(blocked_node.clone());
           }
       }
       for prune_name in prune_blockage {
           self.connection_cache.remove(&prune_name);
       }
    }

    fn send_connect_request(&mut self, peer_name: &NameType) -> RoutingResult {
        // FIXME (ben) We're sending all accepting connections as local since we don't differentiate
        // between local and external yet.
        // FIXME (ben 13/08/2015) We are forced to make this split as the routing message
        // needs to contain a relay name if we are not yet connected to routing nodes
        // under our own name.
        if !self.core.is_connected_node() {
            match self.get_a_bootstrap_name() {
                Some(bootstrap_name) => {
                    // TODO (ben 13/08/2015) for now just take the first bootstrap peer as our relay
                    let routing_message = RoutingMessage {
                        from_authority : Authority::Client(bootstrap_name,
                            self.core.id().signing_public_key()),
                        to_authority   : Authority::ManagedNode(peer_name.clone()),
                        content        : Content::InternalRequest(
                            InternalRequest::Connect(ConnectRequest {
                                local_endpoints    : self.accepting_on.clone(),
                                external_endpoints : vec![],
                                requester_fob      : PublicId::new(self.core.id()),
                            }
                        )),
                    };
                    match SignedMessage::new(Address::Client(
                        self.core.id().signing_public_key()), routing_message,
                        self.core.id().signing_private_key()) {
                        Ok(signed_message) => ignore(self.send(signed_message)),
                        Err(e) => return Err(RoutingError::Cbor(e)),
                    };
                    Ok(())
                },
                None => return Err(RoutingError::NotBootstrapped),
            }
        } else {  // we are a connected node
            let routing_message = RoutingMessage {
                from_authority : Authority::ManagedNode(self.core.id().name()),
                to_authority   : Authority::ManagedNode(peer_name.clone()),
                content        : Content::InternalRequest(
                    InternalRequest::Connect(ConnectRequest {
                        local_endpoints    : self.accepting_on.clone(),
                        external_endpoints : vec![],
                        requester_fob      : PublicId::new(self.core.id()),
                    }
                )),
            };
            match SignedMessage::new(Address::Node(self.core.id().name()),
                routing_message, self.core.id().signing_private_key()) {
                Ok(signed_message) => ignore(self.send(signed_message)),
                Err(e) => return Err(RoutingError::Cbor(e)),
            };
            Ok(())
        }
    }

    fn handle_connect_request(&mut self,
                              request        : InternalRequest,
                              from_authority : Authority,
                              to_authority   : Authority,
                              response_token : SignedToken) -> RoutingResult {
        debug!("handle ConnectRequest");
        match request {
            InternalRequest::Connect(connect_request) => {
                if !connect_request.requester_fob.is_relocated() {
                    return Err(RoutingError::RejectedPublicId); };
                // first verify that the message is correctly self-signed
                if !response_token.verify_signature(&connect_request.requester_fob
                    .signing_public_key()) {
                    return Err(RoutingError::FailedSignature); };
                if !self.core.check_node(&ConnectionName::Routing(
                    connect_request.requester_fob.name())) {
                    return Err(RoutingError::RefusedFromRoutingTable); };
                // TODO (ben 13/08/2015) use public_id_cache or result of future RFC
                // to validate the public_id from the network
                self.connection_manager.connect(connect_request.local_endpoints.clone());
                self.connection_manager.connect(connect_request.external_endpoints.clone());
                self.connection_cache.entry(connect_request.requester_fob.name())
                    .or_insert(SteadyTime::now());
                let routing_message = RoutingMessage {
                    from_authority : Authority::ManagedNode(self.core.id().name()),
                    to_authority   : from_authority,
                    content        : Content::InternalResponse(
                        InternalResponse::Connect(ConnectResponse {
                            local_endpoints    : self.accepting_on.clone(),
                            external_endpoints : vec![],
                            receiver_fob       : PublicId::new(self.core.id()),
                        }, response_token)
                    ),
                };
                match SignedMessage::new(Address::Node(self.core.id().name()),
                    routing_message, self.core.id().signing_private_key()) {
                    Ok(signed_message) => ignore(self.send(signed_message)),
                    Err(e) => return Err(RoutingError::Cbor(e)),
                };
                Ok(())
            },
            _ => return Err(RoutingError::BadAuthority),
        }
    }

    fn handle_connect_response(&mut self,
                               response       : InternalResponse,
                               from_authority : Authority,
                               to_authority   : Authority) -> RoutingResult {
        debug!("handle ConnectResponse");
        match response {
            InternalResponse::Connect(connect_response, signed_token) => {
                if !signed_token.verify_signature(&self.core.id().signing_public_key()) {
                    return Err(RoutingError::FailedSignature); };
                let connect_request = try!(SignedMessage::new_from_token(signed_token));
                if connect_request.get_routing_message().from_authority.get_location()
                    != &self.core.id().name() { return Err(RoutingError::BadAuthority); };
                if !self.core.check_node(&ConnectionName::Routing(
                    connect_response.receiver_fob.name())) {
                    return Err(RoutingError::RefusedFromRoutingTable); };
                // self.connection_manager.connect(connect_response.local_endpoints.clone());
                // self.connection_manager.connect(connect_response.external_endpoints.clone());
                self.connection_cache.entry(connect_response.receiver_fob.name())
                    .or_insert(SteadyTime::now());
                Ok(())
            },
            _ => return Err(RoutingError::BadAuthority),
        }
    }

    // ----- Send Functions -----------------------------------------------------------------------

    fn send_to_user(&self, event: Event) {
        if self.event_sender.send(event).is_err() {
            let _ = self.action_sender.send(Action::Terminate);
        }
    }

    fn send_content(&self, to_authority : Authority, content : Content) -> RoutingResult {
        match self.core.our_address() {
            Address::Node(name) => {
                let our_authority = {
                    match self.core.our_authority(& RoutingMessage {
                        from_authority : Authority::ManagedNode(name),
                        to_authority   : to_authority.clone(),
                        content        : content.clone(),
                        }) {
                        Some(authority) => authority,
                        None => Authority::ManagedNode(name),
                    }
                };
                let routing_message = RoutingMessage {
                    from_authority : our_authority,
                    to_authority   : to_authority,
                    content        : content,
                };
                match SignedMessage::new(Address::Node(self.core.id().name()),
                    routing_message, self.core.id().signing_private_key()) {
                    Ok(signed_message) => ignore(self.send(signed_message)),
                    Err(e) => return Err(RoutingError::Cbor(e)),
                };
                Ok(())
            },
            Address::Client(public_key) => {
                // FIXME (ben 14/08/2015) we need a proper function to retrieve a bootstrap_name
                let bootstrap_name = match self.get_a_bootstrap_name() {
                    Some(name) => name,
                    None => return Err(RoutingError::NotBootstrapped),
                };
                let routing_message = RoutingMessage {
                    from_authority : Authority::Client(bootstrap_name, public_key),
                    to_authority   : to_authority,
                    content        : content,
                };
                match SignedMessage::new(Address::Client(self.core.id().signing_public_key()),
                    routing_message, self.core.id().signing_private_key()) {
                    Ok(signed_message) => ignore(self.send(signed_message)),
                    Err(e) => return Err(RoutingError::Cbor(e)),
                };
                Ok(())
            },
        }
    }

    /// Send a SignedMessage out to the destination
    /// 1. if it can be directly relayed to a Client, then it will
    /// 2. if we can forward it to nodes closer to the destination, it will be sent in parallel
    /// 3. if the destination is in range for us, then send it to all our close group nodes
    /// 4. if all the above failed, try sending it over all available bootstrap connections
    /// 5. finally, if we are a node and the message concerns us, queue it for processing later.
    fn send(&self, signed_message : SignedMessage) -> RoutingResult {
        let destination = signed_message.get_routing_message().destination();
        debug!("Sending signed message to {:?}", destination);
        let bytes = try!(encode(&signed_message));
        // query the routing table for parallel or swarm
        let endpoints = self.core.target_endpoints(&destination);
        debug!("Sending to {:?} target connection(s)", endpoints.len());
        if !endpoints.is_empty() {
            for endpoint in endpoints {
                // TODO(ben 10/08/2015) drop endpoints that fail to send
                ignore(self.connection_manager.send(endpoint, bytes.clone()));
            }
        }

        match self.core.bootstrap_endpoints() {
            Some(bootstrap_peers) => {
                debug!("Falling back to {:?} bootstrap connections to send.",
                    bootstrap_peers.len());
                // TODO (ben 10/08/2015) Strictly speaking we do not have to validate that
                // the relay_name in from_authority Client(relay_name, client_public_key) is
                // the name of the bootstrap connection we're sending it on.  Although this might
                // open a window for attacking a node, in v0.3.* we can leave this unresolved.
                for bootstrap_peer in bootstrap_peers {
                    // TODO(ben 10/08/2015) drop bootstrap endpoints that fail to send
                    debug!("Sending to bootstrap node {:?}", bootstrap_peer.identity());
                    ignore(self.connection_manager.send(bootstrap_peer.endpoint().clone(),
                        bytes.clone()));
                }
            },
            None => {},
        }

        // If we need handle this message, move this copy into the channel for later processing.
        if self.core.name_in_range(&destination.get_location()) {
            if let Authority::Client(relay, public_key) = destination { return Ok(()); };
            debug!("Queuing message for processing ourselves");
            ignore(self.action_sender.send(Action::SendMessage(signed_message)));
        }
        Ok(())
    }

    // -----Message Handlers from Routing Table connections----------------------------------------

    fn handle_external_response(&self, response       : ExternalResponse,
                                       to_authority   : Authority,
                                       from_authority : Authority) -> RoutingResult {

        // Request token is only set if it came from a non-group entity.
        // If it came from a group, then sentinel guarantees message validity.
        let has_invalid_signature = {
            if let &Some(ref token) = response.get_signed_token() {
                !token.verify_signature(&self.core.id().signing_public_key())
            }
            else { false }
        };

        if has_invalid_signature {
            return Err(RoutingError::FailedSignature);
        }

        self.send_to_user(Event::Response {
            response       : response,
            our_authority  : to_authority,
            from_authority : from_authority,
        });

        Ok(())
    }

    fn handle_refresh(&mut self, message: RoutingMessage, tag: u64, payload: Vec<u8>) -> RoutingResult {
        unimplemented!()
    }

    // ------ FIXME -------------------------------------------------------------------------------

    fn get_a_bootstrap_name(&self) -> Option<NameType> {
        match self.core.bootstrap_endpoints() {
            Some(bootstrap_peers) => {
                // TODO (ben 13/08/2015) for now just take the first bootstrap peer as our relay
                match bootstrap_peers.first() {
                    Some(bootstrap_peer) => {
                        match *bootstrap_peer.identity() {
                            ConnectionName::Bootstrap(bootstrap_name) => Some(bootstrap_name),
                            _ => None,
                        }
                    },
                    None => None,
                }
            },
            None => None,
        }
    }
}

fn ignore<R,E>(_result: Result<R,E>) {}
