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

//! This module handle all connections that are not managed by the routing table.
//!
//! As such the relay module handles messages that need to flow in or out of the SAFE network.
//! These messages include bootstrap actions by starting nodes or relay messages for clients.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use time::SteadyTime;
use crust::Endpoint;
use id::Id;
use public_id::PublicId;
use types::Address;
use NameType;
use peer::Peer;
use routing_core::ConnectionName;

const MAX_RELAY : usize = 100;

/// The relay map is used to maintain a list of contacts for whom
/// we are relaying messages, when we are ourselves connected to the network.
/// These have to identify as Client(sign::PublicKey)
pub struct RelayMap {
    relay_map: BTreeMap<ConnectionName, Peer>,
    lookup_map: HashMap<Endpoint, ConnectionName>,
}

impl RelayMap {
    /// This creates a new RelayMap.
    pub fn new() -> RelayMap {
        RelayMap { relay_map: BTreeMap::new(), lookup_map: HashMap::new() }
    }

    /// Adds a Peer to the relay map if the relay map has open
    /// slots, and the Peer is not marked for RoutingTable.
    /// This returns true if the Peer was addded.
    /// Returns true is the endpoint is newly added, or was already present.
    /// Returns false if the threshold was reached or identity already exists.
    /// Returns false if the endpoint is already assigned (to a different name).
    pub fn add_peer(&mut self,
                    identity: ConnectionName,
                    endpoint: Endpoint,
                    public_id: Option<PublicId>)
                    -> bool {
        // reject Routing peers from relay_map
        match identity {
            ConnectionName::Routing(_) => return false,
            _ => {}
        };
        // impose limit on number of relay nodes active
        if !self.relay_map.contains_key(&identity) && self.relay_map.len() >= MAX_RELAY {
            error!(REJECTED because of MAX_RELAY);
            return false;
        }
        // check if endpoint already exists
        if self.lookup_map.contains_key(&endpoint) {
            return false;
        }
        // for now don't allow multiple endpoints on a Peer
        if self.relay_map.contains_key(&identity) {
            return false;
        }
        self.lookup_map.entry(endpoint.clone())
                       .or_insert(identity.clone());
        let new_peer = || Peer::new(identity.clone(), endpoint, public_id);
        self.relay_map.entry(identity.clone())
                      .or_insert_with(new_peer);
        true
    }

    /// This removes the provided endpoint and returns the Peer this endpoint
    /// was registered to; otherwise returns None.
    //  TODO (ben 6/08/2015) drop_endpoint has been simplified for a single endpoint per Peer
    //  find the archived version on 628febf879a9d3684f69967e00b5a45dc880c6e3 for reference
    pub fn drop_endpoint(&mut self, endpoint_to_drop: &Endpoint) -> Option<Peer> {
        match self.lookup_map.remove(endpoint_to_drop) {
            Some(identity) => self.relay_map.remove(&identity),
            None => None,
        }
    }

    /// Removes the provided ConnectionName from the relay map, providing the Peer as removed.
    pub fn drop_connection_name(&mut self, connection_name: &ConnectionName) -> Option<Peer> {
        match self.relay_map.remove(connection_name) {
            Some(peer) => {
                let _ = self.lookup_map.remove(peer.endpoint());
                Some(peer)
            }
            None => None,
        }
    }

    /// Returns true if we keep relay endpoints for given name.
    // FIXME(ben) this needs to be used 16/07/2015
    #[allow(dead_code)]
    pub fn contains_identity(&self, identity: &ConnectionName) -> bool {
        self.relay_map.contains_key(identity)
    }

    /// Returns true if we already have a name associated with this endpoint.
    #[allow(dead_code)]
    pub fn contains_endpoint(&self, endpoint: &Endpoint) -> bool {
        self.lookup_map.contains_key(endpoint)
    }

    /// Returns Option<&Peer> if an endpoint is found
    pub fn lookup_endpoint(&self, endpoint: &Endpoint) -> Option<&Peer> {
        match self.lookup_map.get(endpoint) {
            Some(identity) => self.relay_map.get(&identity),
            None => None,
        }
    }

    // Returns the ConnectionName if either a Relay(Address::Node(name))
    // or Bootstrap(name) is found in the relay map.
    pub fn lookup_name(&self, name: &NameType) -> Option<ConnectionName> {
        let relay_name = match self.relay_map.get(
            &ConnectionName::Relay(Address::Node(name.clone()))) {
            Some(peer) => Some(peer.identity().clone()),
            None => None,
        };
        match relay_name {
            None => match self.relay_map.get(&ConnectionName::Bootstrap(name.clone())) {
                Some(peer) => Some(peer.identity().clone()),
                None => None,
            },
            Some(found_name) => Some(found_name),
        }
    }

    /// Returns the Peer associated to ConnectionName.
    pub fn lookup_connection_name(&self, identity: &ConnectionName) -> Option<&Peer> {
        self.relay_map.get(identity)
    }

    /// Returns true if the length of the relay map is bigger or equal to the maximum
    /// allowed connections.
    pub fn is_full(&self) -> bool {
        self.relay_map.len() >= MAX_RELAY
    }

    /// Returns a vector of all bootstrap connections listed. If none found, returns empty.
    pub fn bootstrap_connections(&self) -> Vec<Peer> {
        let mut bootstrap_connections : Vec<Peer> = Vec::new();
        for (_, peer) in self.relay_map.iter()
            .filter(|ref entry| match *entry.1.identity() {
                ConnectionName::Bootstrap(_) => true, _ => false }) {
            bootstrap_connections.push(peer.clone());
        }
        bootstrap_connections
    }

    /// Returns true if bootstrap connections are listed.
    pub fn has_bootstrap_connections(&self) -> bool {
        for _ in self.relay_map.iter()
            .filter(|ref entry| match *entry.1.identity() {
                ConnectionName::Bootstrap(_) => true, _ => false }) {
            return true;
        }
        false
    }
}

// #[cfg(test)]
// mod test {
//     use super::*;
//     use crust::Endpoint;
//     use id::Id;
//     use public_id::PublicId;
//     use types::Address;
//     use std::net::SocketAddr;
//     use std::str::FromStr;
//     use rand::random;
//
//     fn generate_random_endpoint() -> Endpoint {
//         Endpoint::Tcp(SocketAddr::from_str(&format!("127.0.0.1:{}", random::<u16>())).unwrap())
//     }
//
//     fn drop_ip_node(relay_map: &mut RelayMap, ip_node_to_drop: &Address) {
//         match relay_map.relay_map.get(&ip_node_to_drop) {
//             Some(relay_entry) => {
//                 for endpoint in relay_entry.1.iter() {
//                     relay_map.lookup_map.remove(endpoint);
//                 }
//             },
//             None => return
//         };
//         relay_map.relay_map.remove(ip_node_to_drop);
//     }
//
//     #[test]
//     fn add() {
//         let our_id : Id = Id::new();
//         let our_public_id = PublicId::new(&our_id);
//         let mut relay_map = RelayMap::new(&our_id);
//         assert_eq!(false, relay_map.add_client(our_public_id.clone(), generate_random_endpoint()));
//         assert_eq!(0, relay_map.relay_map.len());
//         assert_eq!(0, relay_map.lookup_map.len());
//         while relay_map.relay_map.len() < super::MAX_RELAY {
//             let new_endpoint = generate_random_endpoint();
//             if !relay_map.contains_endpoint(&new_endpoint) {
//                 assert_eq!(true, relay_map.add_client(PublicId::new(&Id::new()),
//                     new_endpoint)); };
//         }
//         assert_eq!(false, relay_map.add_client(PublicId::new(&Id::new()),
//                           generate_random_endpoint()));
//     }
//
//     #[test]
//     fn drop() {
//         let our_id : Id = Id::new();
//         let mut relay_map = RelayMap::new(&our_id);
//         let test_public_id = PublicId::new(&Id::new());
//         let test_id = Address::Client(test_public_id.signing_public_key());
//         let test_endpoint = generate_random_endpoint();
//         assert_eq!(true, relay_map.add_client(test_public_id.clone(),
//                                                test_endpoint.clone()));
//         assert_eq!(true, relay_map.contains_relay_for(&test_id));
//         assert_eq!(true, relay_map.contains_endpoint(&test_endpoint));
//         drop_ip_node(&mut relay_map, &test_id);
//         assert_eq!(false, relay_map.contains_relay_for(&test_id));
//         assert_eq!(false, relay_map.contains_endpoint(&test_endpoint));
//         assert_eq!(None, relay_map.get_endpoints(&test_id));
//     }
//
//     #[test]
//     fn add_conflicting_endpoints() {
//         let our_id : Id = Id::new();
//         let mut relay_map = RelayMap::new(&our_id);
//         let test_public_id = PublicId::new(&Id::new());
//         let test_id = Address::Client(test_public_id.signing_public_key());
//         let test_endpoint = generate_random_endpoint();
//         let test_conflicting_public_id = PublicId::new(&Id::new());
//         let test_conflicting_id = Address::Client(test_conflicting_public_id.signing_public_key());
//         assert_eq!(true, relay_map.add_client(test_public_id.clone(),
//                                                test_endpoint.clone()));
//         assert_eq!(true, relay_map.contains_relay_for(&test_id));
//         assert_eq!(true, relay_map.contains_endpoint(&test_endpoint));
//         assert_eq!(false, relay_map.add_client(test_conflicting_public_id.clone(),
//                                                 test_endpoint.clone()));
//         assert_eq!(false, relay_map.contains_relay_for(&test_conflicting_id))
//     }
//
//     // TODO (ben 6/08/2015) multiple endpoints are not supported by RelayMap
//     // until Peer supports it.
//     // #[test]
//     // fn add_multiple_endpoints() {
//     //     let our_id : Id = Id::new();
//     //     let mut relay_map = RelayMap::new(&our_id);
//     //     assert!(super::MAX_RELAY - 1 > 0);
//     //     // ensure relay_map is all but full, so multiple endpoints are not counted as different
//     //     // relays.
//     //     while relay_map.relay_map.len() < super::MAX_RELAY - 1 {
//     //         let new_endpoint = generate_random_endpoint();
//     //         if !relay_map.contains_endpoint(&new_endpoint) {
//     //             assert_eq!(true, relay_map.add_client(PublicId::new(&Id::new()),
//     //                 new_endpoint)); };
//     //     }
//     //     let test_public_id = PublicId::new(&Id::new());
//     //     let test_id = Address::Client(test_public_id.signing_public_key());
//     //
//     //     let mut test_endpoint_1 = generate_random_endpoint();
//     //     let mut test_endpoint_2 = generate_random_endpoint();
//     //     loop {
//     //         if !relay_map.contains_endpoint(&test_endpoint_1) { break; }
//     //         test_endpoint_1 = generate_random_endpoint(); };
//     //     loop {
//     //         if !relay_map.contains_endpoint(&test_endpoint_2) { break; }
//     //         test_endpoint_2 = generate_random_endpoint(); };
//     //     assert_eq!(true, relay_map.add_client(test_public_id.clone(),
//     //                                            test_endpoint_1.clone()));
//     //     assert_eq!(true, relay_map.contains_relay_for(&test_id));
//     //     assert_eq!(true, relay_map.contains_endpoint(&test_endpoint_1));
//     //     assert_eq!(false, relay_map.add_client(test_public_id.clone(),
//     //                                             test_endpoint_1.clone()));
//     //     assert_eq!(true, relay_map.add_client(test_public_id.clone(),
//     //                                            test_endpoint_2.clone()));
//     //     assert!(relay_map.get_endpoints(&test_id).unwrap().1
//     //                      .contains(&test_endpoint_1));
//     //     assert!(relay_map.get_endpoints(&test_id).unwrap().1
//     //                      .contains(&test_endpoint_2));
//     // }
//
//     // TODO: add test for drop_endpoint
//
//     // TODO: add tests for unknown_connections
// }
