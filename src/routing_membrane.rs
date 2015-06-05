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

//! This is a fresh start of routing_node.rs and should upon successful completion replace
//! the original routing_node.rs file, which in turn then is the owner of routing membrane and
//! routing core.
//! Routing membrane is a single thread responsible for the in- and outgoing messages.
//! It accepts messages received from CRUST.
//! The membrane evaluates whether a message is to be forwarded, or
//! accepted into the membrane as a request where Sentinel holds it until verified and resolved.
//! Requests resolved by Sentinel, will be handed on to the Interface for actioning.
//! A limited number of messages are deliberatly for Routing and network management purposes.
//! Some network management messages are directly handled without Sentinel resolution.
//! Other network management messages are handled by Routing after Sentinel resolution.

#[allow(unused_imports)]
use cbor::{Decoder, Encoder, CborError};
use rand;
use rustc_serialize::{Decodable, Encodable};
use sodiumoxide;
use sodiumoxide::crypto::sign::verify_detached;
use std::collections::{BTreeMap, HashMap};
use std::sync::mpsc;
use std::boxed::Box;
use std::ops::DerefMut;
use std::sync::mpsc::Receiver;
use time::{Duration, SteadyTime};

use crust;
use lru_time_cache::LruCache;
use message_filter::MessageFilter;
use NameType;
use name_type::{closer_to_target_or_equal, NAME_TYPE_LEN};
use node_interface;
use node_interface::Interface;
use routing_table::{RoutingTable, NodeInfo};
use relay::RelayMap;
use sendable::Sendable;
use types;
use types::{MessageId, NameAndTypeId, Signature, Bytes, DestinationAddress};
use authority::{Authority, our_authority};
use message_header::MessageHeader;
// use messages::bootstrap_id_request::BootstrapIdRequest;
// use messages::bootstrap_id_response::BootstrapIdResponse;
use messages::get_data::GetData;
use messages::get_data_response::GetDataResponse;
use messages::put_data::PutData;
use messages::put_data_response::PutDataResponse;
use messages::connect_request::ConnectRequest;
use messages::connect_response::ConnectResponse;
// use messages::connect_success::ConnectSuccess;
use messages::find_group::FindGroup;
use messages::find_group_response::FindGroupResponse;
use messages::get_group_key::GetGroupKey;
use messages::get_group_key_response::GetGroupKeyResponse;
use messages::post::Post;
use messages::get_client_key::GetKey;
use messages::get_client_key_response::GetKeyResponse;
use messages::put_public_id::PutPublicId;
use messages::{RoutingMessage, MessageTypeTag};
use types::{MessageAction};
use error::{RoutingError, InterfaceError, ResponseError};

// use std::convert::From;


type ConnectionManager = crust::ConnectionManager;
type Event = crust::Event;
type Endpoint = crust::Endpoint;
type PortAndProtocol = crust::Port;

type RoutingResult = Result<(), RoutingError>;

enum ConnectionName {
    Relay(NameType),
    Routing(NameType)
}

/// Routing Membrane
pub struct RoutingMembrane<F : Interface> {
    // for CRUST
    event_input: Receiver<Event>,
    connection_manager: ConnectionManager,
    accepting_on: Vec<Endpoint>,
    // for Routing
    id: types::Id,
    own_name: NameType,
    routing_table: RoutingTable,
    relay_map: RelayMap,
    next_message_id: MessageId,
    filter: MessageFilter<types::FilterType>,
    public_id_cache: LruCache<NameType, types::PublicId>,
    connection_cache: BTreeMap<NameType, SteadyTime>,
    // for Persona logic
    interface: Box<F>
}

impl<F> RoutingMembrane<F> where F: Interface {
    pub fn new(personas : F) -> RoutingMembrane<F> {
        sodiumoxide::init();  // enable shared global (i.e. safe to multithread now)
        let (event_output, event_input) = mpsc::channel();
        let id = types::Id::new();
        let own_name = id.get_name();
        let mut cm = crust::ConnectionManager::new(event_output);
        // TODO: Default Protocol and Port need to be passed down
        let ports_and_protocols : Vec<PortAndProtocol> = Vec::new();
        // TODO: Beacon port should be passed down
        let beacon_port = Some(5483u16);
        let listeners = match cm.start_listening(ports_and_protocols, beacon_port) {
            Err(reason) => {
                println!("Failed to start listening: {:?}", reason);
                (vec![], None)
            }
            Ok(listeners_and_beacon) => listeners_and_beacon
        };
        println!("{:?}  -- listening on : {:?}", own_name, listeners.0);
        RoutingMembrane {
                      id : id,
                      own_name : own_name.clone(),
                      event_input: event_input,
                      connection_manager: cm,
                      routing_table : RoutingTable::new(&own_name),
                      relay_map: RelayMap::new(&own_name),
                      accepting_on: listeners.0,
                      next_message_id: rand::random::<MessageId>(),
                      filter: MessageFilter::with_expiry_duration(Duration::minutes(20)),
                      public_id_cache: LruCache::with_expiry_duration(Duration::minutes(10)),
                      connection_cache: BTreeMap::new(),
                      interface : Box::new(personas)
                    }
    }

    /// Retrieve something from the network (non mutating) - Direct call
    pub fn get(&mut self, type_id: u64, name: NameType) {
        let destination = types::DestinationAddress{ dest: NameType::new(name.get_id()),
                                                     reply_to: None };
        let header = MessageHeader::new(self.get_next_message_id(),
                                        destination, self.our_source_address(),
                                        Authority::Client);
        let request = GetData{ requester: self.our_source_address(),
                               name_and_type_id: NameAndTypeId{name: NameType::new(name.get_id()),
                                                               type_id: type_id} };
        let message = RoutingMessage::new(MessageTypeTag::GetData, header,
                                          request, &self.id.get_crypto_secret_sign_key());

        // FIXME: We might want to return the result.
        ignore(encode(&message).map(|msg| self.send_swarm_or_parallel(&name, &msg)));
    }

    /// Add something to the network, will always go via ClientManager group
    pub fn put(&mut self, destination: NameType, content: Box<Sendable>) {
        let destination = types::DestinationAddress{ dest: destination, reply_to: None };
        let request = PutData{ name: content.name(), data: content.serialised_contents() };
        let header = MessageHeader::new(self.get_next_message_id(),
                                        destination, self.our_source_address(),
                                        Authority::ManagedNode);
        let message = RoutingMessage::new(MessageTypeTag::PutData, header,
                request, &self.id.get_crypto_secret_sign_key());

        // FIXME: We might want to return the result.
        ignore(encode(&message).map(|msg| self.send_swarm_or_parallel(&self.own_name, &msg)));
    }

    /// Add something to the network
    pub fn unauthorised_put(&mut self, destination: NameType, content: Box<Sendable>) {
        let destination = types::DestinationAddress{ dest: destination, reply_to: None };
        let request = PutData{ name: content.name(), data: content.serialised_contents() };
        let header = MessageHeader::new(self.get_next_message_id(), destination,
                                        self.our_source_address(), Authority::Unknown);
        let message = RoutingMessage::new(MessageTypeTag::UnauthorisedPut, header,
                request, &self.id.get_crypto_secret_sign_key());

        // FIXME: We might want to return the result.
        ignore(encode(&message).map(|msg| self.send_swarm_or_parallel(&self.own_name, &msg)));
    }

    /// RoutingMembrane::Run starts the membrane
    pub fn run(&mut self) {
        loop {
            match self.event_input.recv() {
                Err(_) => (),
                Ok(crust::Event::NewMessage(endpoint, bytes)) => {
                    match self.lookup_endpoint(&endpoint) {
                        // we hold an active connection to this endpoint,
                        // mapped to a name in our routing table
                        Some(ConnectionName::Routing(name)) => {
                            self.message_received(&ConnectionName::Routing(name),
                                bytes);
                        },
                        // we hold an active connection to this endpoint,
                        // mapped to a name in our relay map
                        Some(ConnectionName::Relay(name)) => {},
                        None => {
                            // If we don't know the sender, only accept a connect request
                            self.handle_unknown_connect_request(&endpoint, bytes);
                        }
                    }
                },
                Ok(crust::Event::NewConnection(endpoint)) => {
                    self.handle_new_connection(endpoint);
                },
                Ok(crust::Event::LostConnection(endpoint)) => {
                    self.handle_lost_connection(endpoint);
                }
            };
        }
    }

    ///
    fn handle_unknown_connect_request(&mut self, endpoint: &Endpoint, serialised_msg : Bytes)
        -> RoutingResult {
        let message = try!(decode::<RoutingMessage>(&serialised_msg));
        let header = message.message_header;
        let body = message.serialised_body;
        let signature = message.signature;
        //  from unknown endpoints only accept ConnectRequest messages
        let connect_request = try!(decode::<ConnectRequest>(&body));
        // first verify that the message is correctly self-signed
        if !verify_detached(&signature.get_crypto_signature(),
                            &body[..], &connect_request.requester_fob.public_sign_key
                                                       .get_crypto_public_sign_key()) {
            return Err(RoutingError::Response(ResponseError::InvalidRequest));
        }
        // only accept unrelocated Ids from unknown connections
        if connect_request.requester_fob.is_relocated() {
            return Err(RoutingError::RejectedPublicId); }
        // if the PublicId is not relocated,
        // only accept the connection into the RelayMap.
        // This will enable this connection to bootstrap or act as a client.
        let routing_msg = self.construct_connect_response_msg(&header, &body,
            &signature, &connect_request);
        let serialised_message = try!(encode(&routing_msg));
        // Try to connect to the peer.
        // when CRUST succeeds at establishing a connection,
        // we use this register to retrieve the PublicId
        self.relay_map.register_accepted_connect_request(&connect_request.external_endpoints,
            &connect_request.requester_fob);
        self.connection_manager.connect(connect_request.external_endpoints);
        self.relay_map.register_accepted_connect_request(&connect_request.local_endpoints,
            &connect_request.requester_fob);
        self.connection_manager.connect(connect_request.local_endpoints);
        // Send the response containing our details.
        // FIXME: Verify that CRUST can send a message back and does not drop it,
        // simply because it is not established a connection yet.
        debug_assert!(self.connection_manager.send(endpoint.clone(), serialised_message)
            .is_ok());
        Ok(())
    }

    /// When CRUST establishes a two-way connection
    /// after exchanging details in ConnectRequest and ConnectResponse
    ///  - we can either add it to RelayMap (if the id was not-relocated,
    ///    and cached in relay_map)
    ///  - or we can mark it as connected in routing table (if the id was relocated,
    ///    and stored in public_id_cache after successful put_public_id handler,
    ///    after wich on ConnectRequest it will have been given to RT to consider adding).
    //  FIXME: two lines are marked as relevant for state-change;
    //  remainder is exhausting logic for debug purposes.
    //  TODO: add churn trigger
    fn handle_new_connection(&mut self, endpoint : Endpoint) {
        match self.lookup_endpoint(&endpoint) {
            Some(ConnectionName::Routing(name)) => {
        // IMPORTANT: the only state-change is in marking the node connected; rest is debug printout
                match self.routing_table.mark_as_connected(&endpoint) {
                    Some(peer_name) => {
                        println!("RT (size : {:?}) Marked peer {:?} as connected on endpoint {:?}",
                                 self.routing_table.size(), peer_name, endpoint);
                        // FIXME: the presence of this debug assert indicates
                        // that the logic for unconnected RT nodes is not quite right.
                        debug_assert!(peer_name == name);
                    },
                    None => {
                        // this is purely for debug purposes; no relevant state changes
                        match self.routing_table.lookup_endpoint(&endpoint) {
                            Some(peer_name) => {
                                println!("RT (size : {:?}) peer {:?} was already connected on endpoint {:?}",
                                         self.routing_table.size(), peer_name, endpoint);
                            },
                            None => {
                              println!("FAILED: dropping connection on endpoint {:?};
                                        no peer found in RT for this endpoint
                                        and as such also not already connected.", endpoint);
                              // FIXME: This is a logical error because we are twice looking up
                              // the same endpoint in the same RT::lookup_endpoint; should never occur
                              self.connection_manager.drop_node(endpoint);
                            }
                        };
                    }
                };
            },
            Some(ConnectionName::Relay(name)) => {
                // this endpoint is already present in the relay lookup_map
                // nothing to do
            },
            None => {
                // Connect requests for relays do not get stored in the relay map,
                // as we want to avoid state; instead we keep an LruCache to recover the public_id.
                // This either is a client or an un-relocated node bootstrapping.
                match self.relay_map.pop_accepted_connect_request(&endpoint) {
                    Some(public_id) => {
                        // a relocated Id should not be in the cache for un-relocated Ids
                        if public_id.is_relocated() {
                            println!("FAILURE: logical code error, a relocated Id should not have made
                                      its way into this cache.");
                            return; }
        // IMPORTANT: only state-change is here by adding it to the relay_map
                        self.relay_map.add_ip_node(public_id, endpoint);
                    },
                    None => {
                        // Note: we assume that the connect_request precedes
                        // a CRUST::new_connection event and has registered a PublicId
                        // with all desired endpoints it has.
                        // As such, for a membrane we do not accept an unknown endpoint.
                        // If the order on these events is not logically guaranteed by CRUST,
                        // this branch has to be expanded.
                        println!("Refused unknown connection from {:?}", endpoint);
                        self.connection_manager.drop_node(endpoint);
                    }
                };
            }
        };
    }

    /// When CRUST reports a lost connection, ensure we remove the endpoint anywhere
    /// TODO: A churn event might be triggered
    fn handle_lost_connection(&mut self, endpoint : Endpoint) {
        // Make sure the endpoint is dropped anywhere
        // The relay map will automatically drop the Name if the last endpoint to it is dropped
        self.relay_map.drop_endpoint(&endpoint);
        let mut trigger_churn = false;
        match self.routing_table.lookup_endpoint(&endpoint) {
            Some(name) => {
                trigger_churn = self.routing_table.address_in_our_close_group_range(&name);
                self.routing_table.drop_node(&name);
            },
            None => {}
        };
        // TODO: trigger churn on boolean
    }

    /// This the fundamental functional function in routing.
    /// It only handles messages received from connections in our routing table;
    /// i.e. this is a pure SAFE message (and does not function as the start of a relay).
    /// If we are the relay node for a message from the SAFE network to a node we relay for,
    /// then we will pass out the message to the client or bootstrapping node;
    /// no relay-messages enter the SAFE network here.
    fn message_received(&mut self, received_from : &ConnectionName,
        serialised_msg : Bytes) -> RoutingResult {
        match received_from {
            &ConnectionName::Routing(_) => {},
            _ => return Err(RoutingError::Response(ResponseError::InvalidRequest))
        };
        // Parse
        let message = try!(decode::<RoutingMessage>(&serialised_msg));
        let header = message.message_header;
        let body = message.serialised_body;

        // filter check
        if self.filter.check(&header.get_filter()) {
            // should just return quietly
            return Err(RoutingError::FilterCheckFailed);
        }
        // add to filter
        self.filter.add(header.get_filter());

        // check if we can add source to rt
        self.refresh_routing_table(&header.source.from_node);

        // // add to cache
        // if message.message_type == MessageTypeTag::GetDataResponse {
        //     let get_data_response = try!(decode::<GetDataResponse>(&body));
        //     let _ = get_data_response.data.map(|data| {
        //         if data.len() != 0 {
        //             let _ = self.mut_interface().handle_cache_put(
        //                 header.from_authority(), header.from(), data);
        //         }
        //     });
        // }
        //
        // // cache check / response
        // if message.message_type == MessageTypeTag::GetData {
        //     let get_data = try!(decode::<GetData>(&body));
        //
        //     let retrieved_data = self.mut_interface().handle_cache_get(
        //         get_data.name_and_type_id.type_id.clone() as u64,
        //         get_data.name_and_type_id.name.clone(),
        //         header.from_authority(),
        //         header.from());
        //
        //     match retrieved_data {
        //         Ok(action) => match action {
        //             MessageAction::Reply(data) => {
        //                 let reply = self.construct_get_data_response_msg(&header, &get_data, data);
        //                 return encode(&reply).map(|reply| {
        //                     self.send_swarm_or_parallel(&header.send_to().dest, &reply);
        //                 }).map_err(From::from);
        //             },
        //             _ => (),
        //         },
        //         Err(_) => (),
        //     };
        // }
        //
        // self.send_swarm_or_parallel(&header.destination.dest, &serialised_msg);
        //
        // // handle relay request/response
        // if header.destination.dest == self.own_name {
        //     self.send_by_name(header.destination.reply_to.iter(), serialised_msg);
        // }
        //
        // if !self.address_in_close_group_range(&header.destination.dest) {
        //     println!("{:?} not for us ", self.own_name);
        //     return Ok(());
        // }
        //
        // // Drop message before Sentinel check if it is a direct message type (Connect, ConnectResponse)
        // // and this node is in the group but the message destination is another group member node.
        // if message.message_type == MessageTypeTag::ConnectRequest || message.message_type == MessageTypeTag::ConnectResponse {
        //     if header.destination.dest != self.own_name &&
        //         (header.destination.reply_to.is_none() ||
        //          header.destination.reply_to != Some(self.own_name.clone())) { // "not for me"
        //         return Ok(());
        //     }
        // }
        //
        // // pre-sentinel message handling
        // match message.message_type {
        //     MessageTypeTag::UnauthorisedPut => self.handle_put_data(header, body),
        //     MessageTypeTag::GetKey => self.handle_get_key(header, body),
        //     MessageTypeTag::GetGroupKey => self.handle_get_group_key(header, body),
        //     _ => {
        //         // Sentinel check
        //
        //         // switch message type
        //         match message.message_type {
        //             MessageTypeTag::ConnectRequest => self.handle_connect_request(header, body, message.signature),
        //             MessageTypeTag::ConnectResponse => self.handle_connect_response(body),
        //             MessageTypeTag::FindGroup => self.handle_find_group(header, body),
        //             MessageTypeTag::FindGroupResponse => self.handle_find_group_response(header, body),
        //             MessageTypeTag::GetData => self.handle_get_data(header, body),
        //             MessageTypeTag::GetDataResponse => self.handle_get_data_response(header, body),
        //             MessageTypeTag::Post => self.handle_post(header, body),
        //             MessageTypeTag::PostResponse => self.handle_post_response(header, body),
        //             MessageTypeTag::PutData => self.handle_put_data(header, body),
        //             MessageTypeTag::PutDataResponse => self.handle_put_data_response(header, body),
        //             MessageTypeTag::PutPublicId => self.handle_put_public_id(header, body),
        //             //PutKey,
        //             _ => {
        //                 println!("unhandled message from {:?}", received_from.0);
        //                 Err(RoutingError::UnknownMessageType)
        //             }
        //         }
        //     }
        // }
        Ok(())
    }

    /// Scan all passing messages for the existance of nodes in the address space.
    /// If a node is detected with a name that would improve our routing table,
    /// then we cache this name.  During a delay of 5 seconds, we collapse
    /// all re-occurances of this name, after which we send out a connect_request
    /// if the name is still of interest to us at that point in time.
    /// The large delay of 5 seconds is justified, because this is only a passive
    /// mechanism, second to active FindGroup requests.
    fn refresh_routing_table(&mut self, from_node : &NameType) {
      if self.routing_table.check_node(from_node) {
          // FIXME: add correction for already connected, but not-online close node
          let mut next_connect_request : Option<NameType> = None;
          let time_now = SteadyTime::now();
          self.connection_cache.entry(from_node.clone())
                               .or_insert(time_now);
          for (new_node, time) in self.connection_cache.iter() {
              // note that the first method to establish the close group
              // is through explicit FindGroup messages.
              // This refresh on scanning messages is secondary, hence the long delay.
              if time_now - *time > Duration::seconds(5) {
                  next_connect_request = Some(new_node.clone());
                  break;
              }
          }
          match next_connect_request {
              Some(connect_to_node) => {
                  self.connection_cache.remove(&connect_to_node);
                  // check whether it is still valid to add this node.
                  if self.routing_table.check_node(&connect_to_node) {
                      ignore(self.send_connect_request_msg(&connect_to_node));
                  }
              },
              None => ()
          }
       }
    }

    // Main send function, pass iterator of targets and message to clone.
    // FIXME: CRUST does not provide delivery promise.
    fn send<'a, I>(&self, targets: I, message: &Bytes) where I: Iterator<Item=&'a Endpoint> {
        for target in targets {
            ignore(self.connection_manager.send(target.clone(), message.clone()));
        }
    }

    fn send_swarm_or_parallel(&self, name : &NameType, msg: &Bytes) {
        for peer in self.routing_table.target_nodes(name) {
            match peer.connected_endpoint {
                Some(peer_endpoint) => {
                    ignore(self.connection_manager.send(peer_endpoint, msg.clone()));
                },
                None => {}
            };
        }
    }

    fn send_connect_request_msg(&mut self, peer_id: &NameType) -> RoutingResult {
        let routing_msg = self.construct_connect_request_msg(&peer_id);
        let serialised_message = try!(encode(&routing_msg));
        self.send_swarm_or_parallel(peer_id, &serialised_message);
        Ok(())
    }

    // TODO: add optional group; fix bootstrapping/relay
    fn our_source_address(&self) -> types::SourceAddress {
        // if self.bootstrap_endpoint.is_some() {
        //     let id = self.all_connections.0.get(&self.bootstrap_endpoint.clone().unwrap());
        //     if id.is_some() {
        //         return types::SourceAddress{ from_node: id.unwrap().clone(),
        //                                      from_group: None,
        //                                      reply_to: Some(self.own_name.clone()) }
        //     }
        // }
        return types::SourceAddress{ from_node: self.own_name.clone(),
                                     from_group: None,
                                     reply_to: None }
    }

    fn get_next_message_id(&mut self) -> MessageId {
        let temp = self.next_message_id;
        self.next_message_id = self.next_message_id.wrapping_add(1);
        return temp;
    }

    fn lookup_endpoint(&self, endpoint: &Endpoint) -> Option<ConnectionName> {
        // prioritise routing table
        match self.routing_table.lookup_endpoint(&endpoint) {
            Some(name) => Some(ConnectionName::Routing(name)),
            // secondly look in the relay_map
            None => match self.relay_map.lookup_endpoint(&endpoint) {
                Some(name) => Some(ConnectionName::Relay(name)),
                None => None
            }
        }
    }

    fn mut_interface(&mut self) -> &mut F { self.interface.deref_mut() }

    // -----Message Handlers from Routing Table connections----------------------------------------

    fn handle_connect_request(&mut self, original_header: MessageHeader, body: Bytes, signature: Signature) -> RoutingResult {
        println!("{:?} received ConnectRequest ", self.own_name);
        let connect_request = try!(decode::<ConnectRequest>(&body));
        if !connect_request.requester_fob.is_relocated() {
            return Err(RoutingError::RejectedPublicId); }
        // first verify that the message is correctly self-signed
        if !verify_detached(&signature.get_crypto_signature(),
                            &body[..], &connect_request.requester_fob.public_sign_key
                                                       .get_crypto_public_sign_key()) {
            return Err(RoutingError::Response(ResponseError::InvalidRequest));
        }
        // if the PublicId claims to be relocated,
        // check whether we have a temporary record of this relocated Id,
        // which we would have stored after the sentinel group consensus
        // of the relocated Id. If the fobs match, add it to routing_table.
        match self.public_id_cache.remove(&connect_request.requester_fob.name()) {
            Some(public_id) => {
                // check the full fob received corresponds, not just the names
                if public_id == connect_request.requester_fob {
                    // Collect the local and external endpoints into a single vector to construct a NodeInfo
                    let mut peer_endpoints = connect_request.local_endpoints.clone();
                    peer_endpoints.extend(connect_request.external_endpoints.clone().into_iter());
                    let peer_node_info =
                        NodeInfo::new(connect_request.requester_fob.clone(), peer_endpoints, None);
                    // Try to add to the routing table.  If unsuccessful, no need to continue.
                    let (added, _) = self.routing_table.add_node(peer_node_info.clone());
                    if !added {
                        return Err(RoutingError::RefusedFromRoutingTable); }
                    println!("RT (size : {:?}) added {:?} ", self.routing_table.size(), peer_node_info.fob.name());
                    // Try to connect to the peer.
                    self.connection_manager.connect(connect_request.local_endpoints.clone());
                    self.connection_manager.connect(connect_request.external_endpoints.clone());
                    // Send the response containing our details,
                    // and add the original signature as proof of the request
                    let routing_msg = self.construct_connect_response_msg(&original_header, &body, &signature, &connect_request);
                    let serialised_message = try!(encode(&routing_msg));

                    self.send_swarm_or_parallel(&routing_msg.message_header.destination.dest,
                        &serialised_message);
                }
            },
            None => {}
        };
        Ok(())
    }
    // -----Message Constructors-----------------------------------------------

    fn construct_connect_request_msg(&mut self, peer_id: &NameType) -> RoutingMessage {
        let header = MessageHeader::new(self.get_next_message_id(),
            types::DestinationAddress {dest: peer_id.clone(), reply_to: None },
            self.our_source_address(), Authority::ManagedNode);

        // FIXME: We're sending all accepting connections as local since we don't differentiate
        // between local and external yet.
        let connect_request = ConnectRequest {
            local_endpoints: self.accepting_on.clone(),
            external_endpoints: vec![],
            requester_id: self.own_name.clone(),
            receiver_id: peer_id.clone(),
            requester_fob: types::PublicId::new(&self.id),
        };

        RoutingMessage::new(MessageTypeTag::ConnectRequest, header, connect_request,
            &self.id.get_crypto_secret_sign_key())
    }

    fn construct_connect_response_msg(&mut self, original_header : &MessageHeader, body: &Bytes, signature: &Signature,
                                      connect_request: &ConnectRequest) -> RoutingMessage {
        println!("{:?} construct_connect_response_msg ", self.own_name);
        debug_assert!(connect_request.receiver_id == self.own_name, format!("{:?} == {:?} failed", self.own_name, connect_request.receiver_id));

        // FIXME: re-use message_id
        let header = MessageHeader::new(original_header.message_id(),
            original_header.send_to(), self.our_source_address(),
            Authority::ManagedNode);

        // FIXME: We're sending all accepting connections as local since we don't differentiate
        // between local and external yet.
        let connect_response = ConnectResponse {
            requester_local_endpoints: connect_request.local_endpoints.clone(),
            requester_external_endpoints: connect_request.external_endpoints.clone(),
            receiver_local_endpoints: self.accepting_on.clone(),
            receiver_external_endpoints: vec![],
            requester_id: connect_request.requester_id.clone(),
            receiver_id: self.own_name.clone(),
            receiver_fob: types::PublicId::new(&self.id),
            serialised_connect_request: body.clone(),
            connect_request_signature: signature.clone() };

        RoutingMessage::new(MessageTypeTag::ConnectResponse, header,
            connect_response, &self.id.get_crypto_secret_sign_key())
    }
}

fn encode<T>(value: &T) -> Result<Bytes, CborError> where T: Encodable {
    let mut enc = Encoder::from_memory();
    try!(enc.encode(&[value]));
    Ok(enc.into_bytes())
}

fn decode<T>(bytes: &Bytes) -> Result<T, CborError> where T: Decodable {
    let mut dec = Decoder::from_bytes(&bytes[..]);
    match dec.decode().next() {
        Some(result) => result,
        None => Err(CborError::UnexpectedEOF)
    }
}

fn ignore<R,E>(_: Result<R,E>) {}
