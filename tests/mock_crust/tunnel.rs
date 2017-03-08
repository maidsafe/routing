// Copyright 2017 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement.  This, along with the Licenses can be
// found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

use super::{TestNode, add_connected_nodes_until_split, create_connected_nodes, poll_all,
            poll_and_resend, verify_invariant_for_all_nodes};
use itertools::Itertools;
use routing::{Event, EventStream, XOR_NAME_LEN, XorName};
use routing::mock_crust::{Config, Endpoint, Network};
use routing::mock_crust::crust::{self, PeerId};

#[test]
fn failing_connections_ring() {
    let min_section_size = 8;
    let network = Network::new(min_section_size, None);
    let len = min_section_size * 2;
    for i in 0..(len - 1) {
        let ep0 = Endpoint(1 + i);
        let ep1 = Endpoint(1 + (i % len));

        network.block_connection(ep0, ep1);
        network.block_connection(ep1, ep0);
    }
    let nodes = create_connected_nodes(&network, len);
    verify_invariant_for_all_nodes(&nodes);
}

#[test]
fn failing_connections_bidirectional() {
    let min_section_size = 4;
    let network = Network::new(min_section_size, None);
    network.block_connection(Endpoint(2), Endpoint(3));
    network.block_connection(Endpoint(3), Endpoint(2));

    let nodes = create_connected_nodes(&network, min_section_size);
    verify_invariant_for_all_nodes(&nodes);
}

#[test]
fn failing_connections_unidirectional() {
    let min_section_size = 8;
    let network = Network::new(min_section_size, None);
    network.block_connection(Endpoint(1), Endpoint(6));
    network.block_connection(Endpoint(1), Endpoint(7));
    network.block_connection(Endpoint(6), Endpoint(7));

    let nodes = create_connected_nodes(&network, min_section_size);
    verify_invariant_for_all_nodes(&nodes);
}

// Removes nodes from the specified section so that this section will merge with another section.
fn remove_nodes_from_section_till_merge(prefix_name: &XorName,
                                        nodes: &mut Vec<TestNode>,
                                        min_section_size: usize) {
    let section_indexes: Vec<usize> = nodes.iter()
        .enumerate()
        .rev()
        .filter_map(|(index, node)| if node.routing_table().our_prefix().matches(prefix_name) {
                        Some(index)
                    } else {
                        None
                    })
        .collect();
    section_indexes.iter()
        .take(section_indexes.len() - min_section_size + 1)
        .foreach(|index| { let _ = nodes.remove(*index); });
    poll_and_resend(nodes, &mut []);
}

// Adds a pair of nodes with specified names into the network. Also blocks direct connection between
// these them if `is_tunnel` is true. Returns the endpoints of the nodes.
fn add_a_pair(network: &Network,
              nodes: &mut Vec<TestNode>,
              name0: XorName,
              name1: XorName,
              is_tunnel: bool)
              -> (Endpoint, Endpoint) {
    let config = Config::with_contacts(&[nodes[0].handle.endpoint()]);

    nodes.iter_mut().foreach(|node| node.inner.set_next_node_name(name0));
    nodes.push(TestNode::builder(network).config(config.clone()).create());
    poll_and_resend(nodes, &mut []);

    nodes.iter_mut().foreach(|node| node.inner.set_next_node_name(name1));
    nodes.push(TestNode::builder(network).config(config.clone()).create());

    let endpoints = (Endpoint(nodes.len() - 2), Endpoint(nodes.len() - 1));
    if is_tunnel {
        network.block_connection(endpoints.0, endpoints.1);
        network.block_connection(endpoints.1, endpoints.0);
    }

    poll_and_resend(nodes, &mut []);
    endpoints
}

fn locate_tunnel_node(nodes: &[TestNode], client_1: PeerId, client_2: PeerId) -> Option<usize> {
    let tunnel_node_indexes: Vec<usize> = nodes.iter()
        .enumerate()
        .filter_map(|(index, node)| if node.inner.has_tunnel_clients(client_1, client_2) {
                        Some(index)
                    } else {
                        None
                    })
        .collect();
    // There shall be only one tunnel_node for a pair of tunnel_clients across the network
    // Or None if they are directly connected or one of them are no longer in the network
    assert!(tunnel_node_indexes.len() <= 1);
    if tunnel_node_indexes.is_empty() {
        None
    } else {
        Some(tunnel_node_indexes[0])
    }
}

#[test]
fn tunnel_clients() {
    let min_section_size = 3;
    let network = Network::new(min_section_size, None);
    let mut nodes = create_connected_nodes(&network, min_section_size);

    let direct_pair = add_a_pair(&network,
                                 &mut nodes,
                                 XorName([0u8; XOR_NAME_LEN]),
                                 XorName([253u8; XOR_NAME_LEN]),
                                 false);
    let direct_pair_peer_ids = (PeerId(nodes.len() - 1), PeerId(nodes.len() - 2));
    let _ = add_a_pair(&network,
                       &mut nodes,
                       XorName([1u8; XOR_NAME_LEN]),
                       XorName([255u8; XOR_NAME_LEN]),
                       true);
    let tunnel_pair_1_peer_ids = (PeerId(nodes.len() - 1), PeerId(nodes.len() - 2));
    let tunnel_pair = add_a_pair(&network,
                                 &mut nodes,
                                 XorName([2u8; XOR_NAME_LEN]),
                                 XorName([254u8; XOR_NAME_LEN]),
                                 true);
    let tunnel_pair_2_peer_ids = (PeerId(nodes.len() - 1), PeerId(nodes.len() - 2));
    verify_invariant_for_all_nodes(&nodes);
    assert!(locate_tunnel_node(&nodes, direct_pair_peer_ids.0, direct_pair_peer_ids.1).is_none());
    assert!(locate_tunnel_node(&nodes, tunnel_pair_1_peer_ids.0, tunnel_pair_1_peer_ids.1)
                .is_some());
    assert!(locate_tunnel_node(&nodes, tunnel_pair_2_peer_ids.0, tunnel_pair_2_peer_ids.1)
                .is_some());

    add_connected_nodes_until_split(&network, &mut nodes, vec![2, 2, 2, 2], false);
    verify_invariant_for_all_nodes(&nodes);

    network.unblock_connection(tunnel_pair.0, tunnel_pair.1);
    network.unblock_connection(tunnel_pair.1, tunnel_pair.0);
    network.block_connection(direct_pair.0, direct_pair.1);
    network.block_connection(direct_pair.1, direct_pair.0);

    remove_nodes_from_section_till_merge(&XorName([64u8; XOR_NAME_LEN]),
                                         &mut nodes,
                                         min_section_size);
    verify_invariant_for_all_nodes(&nodes);
    assert!(locate_tunnel_node(&nodes, direct_pair_peer_ids.0, direct_pair_peer_ids.1).is_some());
    assert!(locate_tunnel_node(&nodes, tunnel_pair_1_peer_ids.0, tunnel_pair_1_peer_ids.1)
                .is_some());
    assert!(locate_tunnel_node(&nodes, tunnel_pair_2_peer_ids.0, tunnel_pair_2_peer_ids.1)
                .is_none());
}

#[test]
// The purpose of this test is to confirm that as in the logs of MAID-1951, once the tunnel got
// established, a ConnectFailure of a tunnel_client to peer tunnel_client won't incur any action
fn tunnel_client_connect_failure() {
    let min_section_size = 5;
    let network = Network::new(min_section_size, None);
    network.block_connection(Endpoint(2), Endpoint(3));
    network.block_connection(Endpoint(3), Endpoint(2));
    let mut nodes = create_connected_nodes(&network, min_section_size);
    let tunnel_node_index = unwrap!(locate_tunnel_node(&nodes, PeerId(2), PeerId(3)));

    network.send_crust_event(Endpoint(2), crust::Event::ConnectFailure(PeerId(3)));
    let _ = poll_all(&mut nodes, &mut []);
    verify_invariant_for_all_nodes(&nodes);
    assert_eq!(tunnel_node_index,
               unwrap!(locate_tunnel_node(&nodes, PeerId(2), PeerId(3))));
}

fn verify_tunnel_switch(nodes: &mut Vec<TestNode>, node: usize, client_1: usize, client_2: usize) {
    let mut event_count = 0;
    while let Ok(event) = nodes[client_1].inner.try_next_ev() {
        match event {
            Event::NodeLost(name, _) => {
                assert!(name == nodes[node].name() || name == nodes[client_2].name());
                event_count += 1;
            }
            Event::NodeAdded(name, _) => {
                assert!(name == nodes[client_2].name());
                assert_eq!(event_count, 2);
            }
            _ => {
                panic!("{:?} received unexpected event {:?}",
                       nodes[client_1].name(),
                       event)
            }
        }
    }
    event_count = 0;
    while let Ok(event) = nodes[client_2].inner.try_next_ev() {
        match event {
            Event::NodeLost(name, _) => {
                assert!(name == nodes[client_1].name());
                event_count += 1;
            }
            Event::NodeAdded(name, _) => {
                assert!(name == nodes[client_1].name());
                assert_eq!(event_count, 1);
            }
            _ => {
                panic!("{:?} received unexpected event {:?}",
                       nodes[client_2].name(),
                       event)
            }
        }
    }
}

#[test]
fn tunnel_node_disrupted() {
    let min_section_size = 5;
    let network = Network::new(min_section_size, None);
    network.block_connection(Endpoint(2), Endpoint(3));
    network.block_connection(Endpoint(3), Endpoint(2));
    let mut nodes = create_connected_nodes(&network, min_section_size);
    let tunnel_node_index = unwrap!(locate_tunnel_node(&nodes, PeerId(2), PeerId(3)));

    network.send_crust_event(Endpoint(2),
                             crust::Event::LostPeer(PeerId(tunnel_node_index)));
    network.send_crust_event(Endpoint(tunnel_node_index),
                             crust::Event::LostPeer(PeerId(2)));
    poll_and_resend(&mut nodes, &mut []);
    verify_tunnel_switch(&mut nodes, tunnel_node_index, 2, 3);
    assert!(tunnel_node_index != unwrap!(locate_tunnel_node(&nodes, PeerId(2), PeerId(3))));
}

#[test]
fn tunnel_node_blocked() {
    let min_section_size = 5;
    let network = Network::new(min_section_size, None);
    network.block_connection(Endpoint(2), Endpoint(3));
    network.block_connection(Endpoint(3), Endpoint(2));
    let mut nodes = create_connected_nodes(&network, min_section_size);
    let tunnel_node_index = unwrap!(locate_tunnel_node(&nodes, PeerId(2), PeerId(3)));

    network.block_connection(Endpoint(2), Endpoint(tunnel_node_index));
    network.block_connection(Endpoint(tunnel_node_index), Endpoint(2));
    network.send_crust_event(Endpoint(2),
                             crust::Event::LostPeer(PeerId(tunnel_node_index)));
    network.send_crust_event(Endpoint(tunnel_node_index),
                             crust::Event::LostPeer(PeerId(2)));
    poll_and_resend(&mut nodes, &mut []);
    verify_tunnel_switch(&mut nodes, tunnel_node_index, 2, 3);
    assert!(tunnel_node_index != unwrap!(locate_tunnel_node(&nodes, PeerId(2), PeerId(3))));
}
