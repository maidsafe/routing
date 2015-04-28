// Copyright 2015 MaidSafe.net limited
//
// This Safe Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the Safe Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.0, found in the root
// directory of this project at LICENSE, COPYING and CONTRIBUTOR respectively and also
// available at: http://maidsafe.net/network-platform-licensing
//
// Unless required by applicable law or agreed to in writing, the Safe Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS
// OF ANY KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations relating to
// use of the Safe Network Software.

use cbor::{Decoder, Encoder};
use core::iter::FromIterator;
use rand;
use rustc_serialize::{Decodable, Encodable};
use sodiumoxide;
use sodiumoxide::crypto;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{Arc, mpsc, Mutex};
use std::sync::mpsc::Receiver;
use time::Duration;

use crust;
use crust::Endpoint::Tcp;
use generic_sendable_type::GenericSendableType;
use message_filter::MessageFilter;
use NameType;
use name_type::closer_to_target;
use node_interface::Interface;
use routing_table::{RoutingTable, NodeInfo};
use sendable::Sendable;
use types;
use types::{MessageId, RoutingTrait, Authority};
use message_header::MessageHeader;
use messages;
use messages::get_data::GetData;
use messages::get_data_response::GetDataResponse;
use messages::put_data::PutData;
use messages::put_data_response::PutDataResponse;
use messages::close_peer_lost::ClosePeerLost;
use messages::connect_request::ConnectRequest;
use messages::connect_response::ConnectResponse;
use messages::connect_success::ConnectSuccess;
use messages::find_group::FindGroup;
use messages::find_group_response::FindGroupResponse;
use messages::{RoutingMessage, MessageTypeTag};
use super::{Action, RoutingError};

type ConnectionManager = crust::ConnectionManager;
type Event = crust::Event;
pub type Endpoint = crust::Endpoint;
type PortAndProtocol = crust::Port;
type Bytes = Vec<u8>;
type RecvResult = Result<(), ()>;

/// DHT node
pub struct RoutingNode<F: Interface> {
    interface: Arc<Mutex<F>>,
    pmid: types::Pmid,
    own_id: NameType,
    event_input: Receiver<Event>,
    connection_manager: ConnectionManager,
    pending_connections: HashSet<Endpoint>,
    all_connections: (HashMap<Endpoint, NameType>, BTreeMap<NameType, Endpoint>),
    routing_table: RoutingTable,
    accepting_on: Option<Vec<Endpoint>>,
    listening_for_broadcasts_on_port: Option<u16>,
    next_message_id: MessageId,
    bootstrap_node_id: Option<Endpoint>,
    filter: MessageFilter<types::FilterType>,
}

impl<F> RoutingNode<F> where F: Interface {
    pub fn new(my_interface: F) -> RoutingNode<F> {
        sodiumoxide::init();  // enable shared global (i.e. safe to multithread now)
        let (event_output, event_input) = mpsc::channel();
        let pmid = types::Pmid::new();
        let own_id = pmid.get_name();
        let mut cm = crust::ConnectionManager::new(event_output);
        // TODO: Default Protocol and Port need to be passed down
        let ports_and_protocols : Vec<PortAndProtocol> = Vec::new();
        // TODO: Beacon port should be passed down
        let beacon_port = Some(5483u16);
        let listeners = match cm.start_listening(ports_and_protocols, beacon_port) {
            Err(reason) => {
                println!("Failed to start listening: {:?}", reason);
                (None, None)
            }
            Ok(listeners_and_beacon) => {
                (Some(listeners_and_beacon.0), listeners_and_beacon.1)
            }
        };

        RoutingNode { interface: Arc::new(Mutex::new(my_interface)),
                      pmid : pmid,
                      own_id : own_id.clone(),
                      event_input: event_input,
                      connection_manager: cm,
                      pending_connections : HashSet::new(),
                      all_connections: (HashMap::new(), BTreeMap::new()),
                      routing_table : RoutingTable::new(own_id),
                      accepting_on: listeners.0,
                      listening_for_broadcasts_on_port: listeners.1,
                      next_message_id: rand::random::<MessageId>(),
                      bootstrap_node_id: None,
                      filter: MessageFilter::with_expiry_duration(Duration::minutes(20))
                    }
    }

    /// Retrieve something from the network (non mutating) - Direct call
    pub fn get(&self, type_id: u64, name: NameType) { unimplemented!() }

    /// Add something to the network, will always go via ClientManager group
    pub fn put<T>(&mut self, destination: NameType, content: T) where T: Sendable {
        let message_id = self.get_next_message_id();
        let destination = types::DestinationAddress{ dest: self.id(), reply_to: None };
        let source = types::SourceAddress{ from_node: self.id(), from_group: None, reply_to: None };
        let authority = types::Authority::Client;
        let crypto_signature = crypto::sign::sign_detached(
                &content.serialised_contents(), &self.pmid.get_crypto_secret_sign_key());
        let signature = types::Signature::new(crypto_signature);
        let header = MessageHeader::new(message_id, destination, source, authority, Some(signature));
        let request = PutData{ name: content.name(), data: content.serialised_contents() };
        let message = RoutingMessage::new(MessageTypeTag::PutData, header, request);
        let mut e = Encoder::from_memory();

        e.encode(&[message]).unwrap();
        self.send_swarm_or_parallel(&self.id(), &e.into_bytes());
    }

    /// Mutate something on the network (you must prove ownership) - Direct call
    pub fn post(&self, destination: NameType, content: Vec<u8>) { unimplemented!() }

    pub fn bootstrap(&mut self, bootstrap_list: Option<Vec<Endpoint>>,
                     beacon_port: Option<u16>) -> Result<(), RoutingError> {
        match self.connection_manager.bootstrap(bootstrap_list, beacon_port) {
            Err(reason) => {
                println!("Failed to bootstrap: {:?}", reason);
                Err(RoutingError::FailedToBootstrap)
            }
            Ok(bootstrapped_to) => {
                self.bootstrap_node_id = Some(bootstrapped_to);
                Ok(())
            }
        }
    }

    pub fn run(&mut self) {
        loop {
            let event = self.event_input.recv();

            if event.is_err() { return; }

            match event.unwrap() {
                crust::Event::NewMessage(endpoint, bytes) => {
                    if self.all_connections.0.contains_key(&endpoint) {
                        let peer_id = self.all_connections.0.get(&endpoint).unwrap().clone();
                        if self.message_received(&peer_id, bytes).is_err() {
                            println!("failed to Parse message !!! check  from - {:?} ", peer_id);
                            // let _ = self.connection_manager.drop_node(id);  // discuss : no need to drop
                        }
                    }
                },
                crust::Event::NewConnection(endpoint) => {
                    self.handle_connect(endpoint);
                },
                // crust::Event::Accept(id, bytes) => {
                //     self.handle_accept(id.clone(), bytes);
                // },
                crust::Event::LostConnection(endpoint) => {
                    self.handle_lost_connection(endpoint);
                }
            }
        }
    }

    fn accepting_on(&self) -> Option<Vec<crust::Endpoint>> {
        self.accepting_on.clone().and_then(|endpoints| {
            Some(endpoints)
        })
    }

    fn next_endpoint_pair(&self) -> Option<(Vec<Endpoint>, Vec<Endpoint>)> {
        // FIXME: Set the second argument to 'external' address
        // when known.
        self.accepting_on().and_then(|addr| Some((addr.clone(), addr)))
    }

    fn handle_connect(&mut self, peer_endpoint: Endpoint) {
        if self.all_connections.0.contains_key(&peer_endpoint) ||
           self.pending_connections.contains(&peer_endpoint) {
            // ignore further request once received request or has added
            return;
        }
        self.pending_connections.insert(peer_endpoint.clone());
        self.bootstrap_node_id = Some(peer_endpoint.clone());
        // println!("{:?} bootstrap_node_id added : {:?}", self.own_id, peer_id);
        // send find group
        let msg = self.construct_find_group_msg();
        let msg = self.encode(&msg);
        debug_assert!(self.bootstrap_node_id.is_some());
        let _ = self.connection_manager.send(peer_endpoint, msg);
    }

    fn handle_accept(&mut self, peer_endpoint: Endpoint, peer_id: NameType, bytes: Bytes) {
        // println!("In handle accept of {:?}", self.own_id);
        if self.all_connections.0.contains_key(&peer_endpoint) ||
           !self.pending_connections.contains(&peer_endpoint) {
            // ignore further request once added or not in sequence (not recorded as pending)
            return;
        }
        self.pending_connections.remove(&peer_endpoint);
        self.all_connections.0.insert(peer_endpoint.clone(), peer_id.clone());
        self.all_connections.1.insert(peer_id.clone(), peer_endpoint.clone());

        let connect_succcess_msg = self.decode::<ConnectSuccess>(&bytes);

        if connect_succcess_msg.is_none() {  // TODO handle non routing connection here
            if self.bootstrap_node_id.is_none() &&
             (self.all_connections.0.len() == 1) && (self.all_connections.0.contains_key(&peer_endpoint)) { // zero state only`
                self.bootstrap_node_id = Some(peer_endpoint.clone());
                // println!("{:?} bootstrap_node_id added : {:?}", self.own_id, peer_endpoint);
            }
            return;
        }
        let connect_succcess_msg = connect_succcess_msg.unwrap();
        let peer_node_info = NodeInfo::new(connect_succcess_msg.peer_fob, true);
        let result = self.routing_table.add_node(peer_node_info);
        if result.0 {
          println!("{:?} added {:?} <RT size:{}>", self.own_id, connect_succcess_msg.peer_id, self.routing_table.size());
        } else {
           println!("{:?} failed to add {:?}", self.own_id, connect_succcess_msg.peer_id);
        }
    }

    fn handle_lost_connection_by_id(&mut self, peer_id: NameType) {
        let peer_endpoint = match self.all_connections.1.get(&peer_id) {
            None => return,
            Some(peer) => peer.clone(),
        };
        self.handle_lost_connection(peer_endpoint);
    }

    fn handle_lost_connection(&mut self, peer_endpoint: Endpoint) {
        self.pending_connections.remove(&peer_endpoint);
        let peer_id = match self.all_connections.0.remove(&peer_endpoint) {
            None => return,
            Some(peer) => peer,
        };
        match self.all_connections.1.remove(&peer_id) {
            None => panic!("Mismatch in containers of `all_connections`"),
            Some(peer) => assert!(peer == peer_endpoint),
        }
        // TODO - it would be more efficient if routing_table.drop_node returned a bool to indicate
        // whether the dropped node was in the close group or not - we wouldn't need to compare
        // routing_table.our_close_group before and after to find out.
        let close_group_before = BTreeSet::<NameType>::from_iter(
            self.routing_table.our_close_group().iter().map(
                |ref node_info| node_info.id.clone()));
        self.routing_table.drop_node(&peer_id);
        let close_group_after = self.routing_table.our_close_group();
        self.all_connections.1.remove(&peer_id);

        // do account transfers
        let mut accounts: Vec<GenericSendableType>;
        {
            let mut self_interface = match self.interface.lock() {
                Err(_) => return,
                Ok(interface) => interface,
            };
            let current_close_group = Vec::<NameType>::from_iter(close_group_after.iter().map(
                |ref node_info| node_info.id.clone()));
            accounts = self_interface.handle_churn(current_close_group);
        }
        for account in accounts.iter() {
            self.put(account.name(), account.clone());
        }

        // notify peers of lost connection if it was in our close group
        let current_close_group = BTreeSet::<NameType>::from_iter(close_group_after.iter().map(
            |ref node_info| node_info.id.clone()));
        let difference: Vec<NameType> =
            close_group_before.difference(&current_close_group).cloned().collect();
        assert!(difference.len() < 2);
        if difference.is_empty() {
            return;
        }
        self.send_close_peer_lost(&difference[0]);
    }

    fn message_received(&mut self, peer_id: &NameType, serialised_message: Bytes) -> RecvResult {
        // Parse
        let message = match self.decode::<RoutingMessage>(&serialised_message) {
            None => {
                println!("Problem parsing message of size {} from {:?}",
                         serialised_message.len(), peer_id);
                return Err(());
            },
            Some(msg) => msg,
        };

        let header = message.message_header;
        let body = message.serialised_body;
        // filter check
        if self.filter.check(&header.get_filter()) {
            // should just return quietly
            return Err(());
        }
        // add to filter
        self.filter.add(header.get_filter());

        // add to cache
        if message.message_type == MessageTypeTag::GetDataResponse {
            let get_data_response = try!(self.decode::<GetDataResponse>(&body).ok_or(()));
            if get_data_response.data.len() != 0 {
                let mut self_interface = match self.interface.lock() {
                    Err(_) => return Err(()),
                    Ok(interface) => interface,
                };
                let _ = self_interface.handle_cache_put(header.from_authority(), header.from(),
                                                        get_data_response.data);
            }
        }

        // cache check / response
        if message.message_type == MessageTypeTag::GetData {
            let get_data = try!(self.decode::<GetData>(&body).ok_or(()));
            let mut retrieved_data: Result<Action, RoutingError>;
            {
                let mut self_interface = match self.interface.lock() {
                    Err(_) => return Err(()),
                    Ok(interface) => interface,
                };
                let get_data_copy = get_data.clone();
                retrieved_data = self_interface.handle_cache_get(
                    get_data_copy.name_and_type_id.type_id as u64,
                    get_data_copy.name_and_type_id.name, header.from_authority(), header.from());
            }
            match retrieved_data {
                Err(_) => (),
                Ok(action) => match action {
                    Action::Reply(data) => {
                        let reply = self.construct_get_data_response_msg(&header, &get_data, data);
                        let serialised_reply = self.encode(&reply);
                        self.send_swarm_or_parallel(&header.send_to().dest, &serialised_reply);
                        return Ok(());
                    },
                    _ => (),
                },
            };
        }

        self.send_swarm_or_parallel(&header.destination.dest, &serialised_message);
        // handle relay request/response

        let relay_response = header.destination.reply_to.is_some() &&
                             header.destination.dest == self.own_id;
        if relay_response {
            println!("{:?} relay response sent to nrt {:?}", self.own_id, header.destination.reply_to);
            // TODO : what shall happen to relaying message ? routing_node choosing a closest node ?
            for key in self.all_connections.0.keys() {
                let _ = self.connection_manager.send(key.clone(), serialised_message);
                return Ok(());
            }
        }

        // TODO(prakash)

        if !self.address_in_close_group_range(&header.destination.dest) {
            println!("{:?} not for us ", self.own_id);
            return Ok(());
        }

        // Drop message before Sentinel check if it is a direct message type (Connect, ConnectResponse)
        // and this node is in the group but the message destination is another group member node.
        // "not for me"

        // Sentinel check

        // switch message type
        match message.message_type {
            MessageTypeTag::ClosePeerLost => self.handle_close_peer_lost(body),
            MessageTypeTag::ConnectRequest => self.handle_connect_request(header, body),
            MessageTypeTag::ConnectResponse => self.handle_connect_response(body),
            MessageTypeTag::FindGroup => self.handle_find_group(header, body),
            MessageTypeTag::FindGroupResponse => self.handle_find_group_response(header, body),
            //GetData,
            //GetDataResponse,
            //GetClientKey,
            //GetClientKeyResponse,
            //GetGroupKey,
            //GetGroupKeyResponse,
            //Post,
            //PostResponse,
            MessageTypeTag::PutData => self.handle_put_data(header, body),
            MessageTypeTag::PutDataResponse => self.handle_put_data_response(header, body),
            //PutKey,
            _ => {
                println!("unhandled message from {:?}", peer_id);
                Err(())
            }
        }
    }

    /// This returns our calculated authority with regards
    /// to the element passed in from the message and the message header.
    /// Note that the message has first to pass Sentinel as to be verified.
    /// a) if the message is not from a group,
    ///       the originating node is within our close group range
    ///       and the element is not the destination
    ///    -> Client Manager
    /// b) if the element is within our close group range
    ///       and the destination is the element
    ///    -> Network-Addressable-Element Manager
    /// c) if the message is from a group,
    ///       the destination is within our close group,
    ///       and our id is not the destination
    ///    -> Node Manager
    /// d) if the message is from a group,
    ///       the group is within our close group range,
    ///       and the destination is our id
    ///    -> Managed Node
    /// e) otherwise return Unknown Authority
    fn our_authority(&self, element : &NameType, header : &MessageHeader) -> Authority {
        if !header.is_from_group()
           && self.routing_table.address_in_our_close_group_range(&header.from_node())
           && header.destination.dest != *element {
            return Authority::ClientManager; }
        else if self.routing_table.address_in_our_close_group_range(element)
           && header.destination.dest == *element {
            return Authority::NaeManager; }
        else if header.is_from_group()
           && self.routing_table.address_in_our_close_group_range(&header.destination.dest)
           && header.destination.dest != self.own_id {
            return Authority::NodeManager; }
        else if header.from_group()
                      .and_then(|group| Some(self.routing_table
                                                 .address_in_our_close_group_range(&group)))
                      .unwrap_or(false)
           && header.destination.dest == self.own_id {
            return Authority::ManagedNode; }
        return Authority::Unknown;
    }

    fn handle_close_peer_lost(&mut self, body: Bytes) -> RecvResult {
        let close_peer_lost = try!(self.decode::<ClosePeerLost>(&body).ok_or(()));
        self.handle_lost_connection_by_id(close_peer_lost.peer_id);
        Ok(())
    }

    fn handle_connect_request(&mut self, original_header: MessageHeader, body: Bytes) -> RecvResult {
        println!("{:?} received ConnectRequest ", self.own_id);
        let connect_request = try!(self.decode::<ConnectRequest>(&body).ok_or(()));
        if !(self.routing_table.check_node(&connect_request.requester_id)) {
           return Err(());
        }
        //let (receiver_local, receiver_external) = try!(self.next_endpoint_pair().ok_or(()));  //FIXME this is correct place

        let routing_msg = self.construct_connect_response_msg(&original_header, &connect_request);
        // FIXME(Peter) below method is needed
        // send_swarm_or_parallel();

        if original_header.source.reply_to.is_some() {
            let reply_to_address = original_header.source.reply_to.unwrap();
            if self.all_connections.1.contains_key(&reply_to_address) {
                let _ = self.connection_manager.send(self.all_connections.1.get(&reply_to_address).unwrap().clone(),
                                                     self.encode(&routing_msg));
            } else {
                return Err(());
            }
        }
        Ok(())
    }

    fn handle_connect_response(&mut self, body: Bytes) -> RecvResult {
        println!("{:?} received ConnectResponse", self.own_id);
        let connect_response = try!(self.decode::<ConnectResponse>(&body).ok_or(()));
        if !(self.routing_table.check_node(&connect_response.receiver_id)) {
           return Ok(())
        }

        // The following code block is no longer required due to the changes in crust
        // let success_msg = self.construct_success_msg();
        // let msg = self.encode(&success_msg);
        // let _ = self.connection_manager.connect(msg);

        // workaround for zero state
        if (self.all_connections.0.len() == 1) && (self.all_connections.1.contains_key(&connect_response.receiver_id)) {
            let peer_node_info = NodeInfo::new(connect_response.receiver_fob, true);
            let result = self.routing_table.add_node(peer_node_info);
            if result.0 {
                println!("{:?} added {:?} <RT size:{}>", self.own_id, connect_response.receiver_id, self.routing_table.size());
            } else {
                println!("{:?} failed to add {:?}", self.own_id, connect_response.receiver_id);
            }
        }
        Ok(())
    }

    fn handle_find_group(&mut self, original_header: MessageHeader, body: Bytes) -> RecvResult {
        //println!("{:?} received FindGroup", self.own_id);
        let find_group = try!(self.decode::<FindGroup>(&body).ok_or(()));
        let close_group = self.routing_table.our_close_group();
        let mut group: Vec<types::PublicPmid> = vec![];
        for x in close_group {
            group.push(x.fob);
        }
        // add ourselves
        group.push(types::PublicPmid::new(&self.pmid));
        let routing_msg = self.construct_find_group_response_msg(&original_header, &find_group, group);

        // FIXME(Peter) below method is needed
        // send_swarm_or_parallel();
        // if node in my group && in non routing list send it to non_routnig list as well
        if original_header.source.reply_to.is_some() {
            let reply_to_address = original_header.source.reply_to.unwrap();
            if self.all_connections.1.contains_key(&reply_to_address) {
                let _ = self.connection_manager.send(self.all_connections.1.get(&reply_to_address).unwrap().clone(),
                                                     self.encode(&routing_msg));
            } else {
                return Err(());
            }
        }
        Ok(())
    }

    fn handle_find_group_response(&mut self, original_header: MessageHeader, body: Bytes) -> RecvResult {
        //println!("{:?} received FindGroupResponse", self.own_id);
        let find_group_response = try!(self.decode::<FindGroupResponse>(&body).ok_or(()));
        for peer in find_group_response.group {
            if !self.routing_table.check_node(&peer.name) {
                continue;
            }
            let routing_msg = self.construct_connect_request_msg(&peer.name);
            if self.bootstrap_node_id.is_some() {
                let bootstrap_node = self.bootstrap_node_id.clone();
                let _ = self.connection_manager.send(bootstrap_node.unwrap(), self.encode(&routing_msg));
            }
            // SendSwarmOrParallel  // FIXME
        }
        Ok(())
    }

    fn handle_get_data(get_data: GetData, original_header: MessageHeader) {
        unimplemented!();
    }

    fn handle_get_data_response(get_data_response: GetDataResponse, original_header: MessageHeader) {
        // need to call interface handle_get_response
        unimplemented!();
    }

    // // for clients, below methods are required
    fn handle_put_data(&self, header: MessageHeader, body: Bytes) -> RecvResult {
        let put_data = try!(self.decode::<PutData>(&body).ok_or(()));
        let our_authority = self.our_authority(&put_data.name, &header);
        let from_authority = header.from_authority();
        let from = header.from();
        let to = header.send_to();

        let mut interface = self.interface.lock().unwrap();
        match interface.handle_put(our_authority, from_authority, from, to, put_data.data) {
            Ok(_) => Ok(()),
            Err(_) => Err(())
        }
    }

    fn handle_put_data_response(&self, header: MessageHeader, body: Bytes) -> RecvResult {
        let put_data_response = try!(self.decode::<PutDataResponse>(&body).ok_or(()));
        let from_authority = header.from_authority();
        let from = header.from();

        let response;
        if put_data_response.data.len() != 0 {
            response = Ok(put_data_response.data);
        } else {
            response = Err(RoutingError::IncorrectData(put_data_response.error));
        }

        let mut interface = self.interface.lock().unwrap();
        interface.handle_put_response(from_authority, from, response);
        Ok(())
    }

    fn decode<T>(&self, bytes: &Bytes) -> Option<T> where T: Decodable {
        let mut dec = Decoder::from_bytes(&bytes[..]);
        dec.decode().next().and_then(|result| result.ok())
    }

    fn encode<T>(&self, value: &T) -> Bytes where T: Encodable {
        let mut enc = Encoder::from_memory();
        let _ = enc.encode(&[value]);
        enc.into_bytes()
    }

    fn our_source_address(&self) -> types::SourceAddress {
        if self.bootstrap_node_id.is_some() {
            return types::SourceAddress{ from_node: self.all_connections.0.get(&self.bootstrap_node_id.clone().unwrap()).unwrap().clone(),
                                         from_group: None,
                                         reply_to: Some(self.own_id.clone()) }
        } else {
            return types::SourceAddress{ from_node: self.own_id.clone(),
                                         from_group: None,
                                         reply_to: None }
        }
    }

    fn our_group_address(&self, group_id: NameType) -> types::SourceAddress {
        types::SourceAddress{ from_node: self.own_id.clone(), from_group: Some(group_id.clone()),
                                reply_to: None }
    }

    fn construct_find_group_msg(&mut self) -> RoutingMessage {
        let header = MessageHeader {
            message_id:  self.get_next_message_id(),
            destination: types::DestinationAddress {
                             dest:     self.own_id.clone(),
                             reply_to: None
                         },
            source:      self.our_source_address(),
            authority:   types::Authority::ManagedNode,
            signature:   None
        };
        RoutingMessage{
            message_type:    messages::MessageTypeTag::FindGroup,
            message_header:  header,
            serialised_body: self.encode(&FindGroup{ requester_id: self.own_id.clone(),
                                                     target_id:    self.own_id.clone()
                                                   })
        }
    }

    fn construct_find_group_response_msg(&mut self, original_header : &MessageHeader,
                                         find_group: &FindGroup,
                                         group: Vec<types::PublicPmid>) -> RoutingMessage {
        let header = MessageHeader {
            message_id:  self.get_next_message_id(),
            destination: original_header.send_to(),
            source:      self.our_group_address(find_group.target_id.clone()),
            authority:   types::Authority::NaeManager,
            signature:   None
        };

        RoutingMessage{
            message_type:    messages::MessageTypeTag::FindGroupResponse,
            message_header:  header,
            serialised_body: self.encode(&FindGroupResponse{ target_id: find_group.target_id.clone(),
                                                             group: group
                                                            })
        }
    }

    fn construct_success_msg(&mut self) -> ConnectSuccess {
        let connect_success = ConnectSuccess {
                                                peer_id: self.own_id.clone(),
                                                peer_fob: types::PublicPmid::new(&self.pmid),
                                              };
        return connect_success
    }

    fn construct_connect_request_msg(&mut self, peer_id: &NameType) -> RoutingMessage {
        let header = MessageHeader {
            message_id:  self.get_next_message_id(),
            destination: types::DestinationAddress {dest: peer_id.clone(), reply_to: None },
            source:      self.our_source_address(),
            authority:   types::Authority::ManagedNode,
            signature:   None
        };

        let invalid_addr = vec![Tcp(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0,0,0,0), 0)))];
        let (requester_local, requester_external)
            = self.next_endpoint_pair().unwrap_or((invalid_addr.clone(), invalid_addr));  // FIXME


        let connect_request = ConnectRequest {
            local:          match requester_local[0] { Tcp(local) => local },
            external:       match requester_external[0] { Tcp(local) => local },
            requester_id:   self.own_id.clone(),
            receiver_id:    peer_id.clone(),
            requester_fob:  types::PublicPmid::new(&self.pmid),
        };

        RoutingMessage{
            message_type:    MessageTypeTag::ConnectRequest,
            message_header:  header,
            serialised_body: self.encode(&connect_request)
        }
    }

    fn construct_connect_response_msg(&mut self, original_header : &MessageHeader,
                                      connect_request: &ConnectRequest) -> RoutingMessage {
        println!("{:?} construct_connect_response_msg ", self.own_id);
        debug_assert!(connect_request.receiver_id == self.own_id, format!("{:?} == {:?} failed", self.own_id, connect_request.receiver_id));

        let header = MessageHeader {
            message_id:  self.get_next_message_id(),
            destination: original_header.send_to(),
            source:      self.our_source_address(),
            authority:   types::Authority::ManagedNode,
            signature:   None  // FIXME
        };
        let invalid_addr = vec![Tcp(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0,0,0,0), 0)))];
        let (receiver_local, receiver_external)
            = self.next_endpoint_pair().unwrap_or((invalid_addr.clone(), invalid_addr));  // FIXME

        let connect_response = ConnectResponse {
            requester_local:    connect_request.local,
            requester_external: connect_request.external,
            receiver_local:     match receiver_local[0] { Tcp(local) => local },
            receiver_external:  match receiver_external[0] { Tcp(local) => local },
            requester_id:       connect_request.requester_id.clone(),
            receiver_id:        self.own_id.clone(),
            receiver_fob:       types::PublicPmid::new(&self.pmid) };

        RoutingMessage{
            message_type:    MessageTypeTag::ConnectResponse,
            message_header:  header,
            serialised_body: self.encode(&connect_response)
        }
    }

    fn construct_get_data_response_msg(&mut self, original_header: &MessageHeader,
                                       get_data: &GetData, data: Vec<u8>) -> RoutingMessage {
        let header = MessageHeader {
            message_id: self.get_next_message_id(),
            destination: original_header.send_to(),
            source: self.our_source_address(),
            authority: types::Authority::ManagedNode,
            signature: None  // FIXME
        };
        let get_data_response = GetDataResponse {
            name_and_type_id: get_data.name_and_type_id.clone(), data: data, error: vec![]
        };
        RoutingMessage{
            message_type: MessageTypeTag::GetDataResponse,
            message_header: header,
            serialised_body: self.encode(&get_data_response)
        }
    }

    fn send_close_peer_lost(&mut self, lost_peer: &NameType) {
        let message_id = self.get_next_message_id();
        let destination = types::DestinationAddress{ dest: lost_peer.clone(), reply_to: None };
        let source = types::SourceAddress{ from_node: self.id(),
                                           from_group: Some(lost_peer.clone()), reply_to: None };
        let authority = types::Authority::Client;
        let header = MessageHeader::new(message_id, destination, source, authority, None);
        let request = ClosePeerLost{ peer_id: lost_peer.clone() };
        let message = RoutingMessage::new(MessageTypeTag::ClosePeerLost, header, request);
        let mut encoder = Encoder::from_memory();
        encoder.encode(&[message]).unwrap();
        self.send_swarm_or_parallel(lost_peer, &encoder.into_bytes());
    }

    fn get_next_message_id(&mut self) -> MessageId {
        let current = self.next_message_id;
        self.next_message_id += 1;
        current
    }

    fn send_swarm_or_parallel(&self, target: &NameType, serialised_message: &Bytes) {
        for peer in self.get_connected_target(target) {
            if self.all_connections.1.contains_key(&peer.id) {
                let res = self.connection_manager.send(self.all_connections.1.get(&peer.id).unwrap().clone(),
                                                       serialised_message.clone());
                if res.is_err() {
                    println!("{:?} failed to send to {:?}", self.own_id, peer.id);
                }
            }
        }
    }

    fn get_connected_target(&self, target: &NameType) -> Vec<NodeInfo> {
        let mut nodes = self.routing_table.target_nodes(target.clone());
        //println!("{:?} get_connected_target routing_table.size:{} target:{:?} -> {:?}", self.own_id, self.routing_table.size(), target, nodes);
        nodes.retain(|x| { x.connected });
        nodes
    }

    fn address_in_close_group_range(&self, address: &NameType) -> bool {
        if self.routing_table.size() < RoutingTable::get_group_size() {
            return true;
        }

        let close_group = self.routing_table.our_close_group();
        closer_to_target(&address, &self.routing_table.our_close_group().pop().unwrap().id, &self.own_id)
    }

    pub fn id(&self) -> NameType { self.own_id.clone() }
}

#[cfg(test)]
mod test {
    //use routing_node::{RoutingNode};
    // use node_interface::*;
    // use types::{Authority, DestinationAddress};
    // use name_type::NameType;
    // use super::super::{Action, RoutingError};
    //use std::thread;
    //use std::net::{SocketAddr};
    //use std::str::FromStr;

    struct NullInterface;

    //#[test]
    //fn test_routing_node() {
    //    let f1 = NullInterface;
    //    let f2 = NullInterface;
    //    let f3 = NullInterface;
    //    let n1 = RoutingNode::new(NameType::generate_random(), f1);
    //    let n2 = RoutingNode::new(NameType::generate_random(), f2);
    //    let n3 = RoutingNode::new(NameType::generate_random(), f3);

    //    println!("{:?}->Alice", n1.id());
    //    println!("{:?}->Betty", n2.id());
    //    println!("{:?}->Casey", n3.id());
    //    let n1_ep = n1.accepting_on().unwrap();
    //    let n2_ep = n2.accepting_on().unwrap();
    //    let n3_ep = n3.accepting_on().unwrap();

    //    fn run_node(n: RoutingNode<NullInterface>, my_ep: SocketAddr, his_ep: SocketAddr)
    //        -> thread::JoinHandle
    //    {
    //        thread::spawn(move || {
    //            let mut n = n;
    //            let bootstrap_ep = SocketAddr::from_str(&format!("127.0.0.1:{}", 5483u16)).unwrap();
    //            if my_ep.port() != bootstrap_ep.port() {
    //                n.add_bootstrap(bootstrap_ep);
    //            }
    //            n.run();
    //        })
    //    }

    //    let t1 = run_node(n1, n1_ep.clone(), n2_ep.clone());
    //    let t2 = run_node(n2, n2_ep.clone(), n1_ep.clone());
    //    thread::sleep_ms(1000);
    //    println!("Starting node 3 ... ");
    //    let t3 = run_node(n3, n3_ep.clone(), n1_ep.clone());
    //    assert!(t1.join().is_ok());
    //    assert!(t2.join().is_ok());
    //    assert!(t3.join().is_ok());
    //}
}
#[test]
fn dummy_routing()  {
}
