// Copyright 2016 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.1.  This, along with the
// Licenses can be found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

use itertools::Itertools;
use rand::Rng;
use routing::{Authority, DataIdentifier, Destination, Event, MessageId, QUORUM, Request, XorName};
use routing::mock_crust::{Config, Network};
use std::cmp;
use std::collections::{HashMap, HashSet};
use super::{TestNode, create_connected_nodes, gen_range_except, poll_and_resend,
            verify_invariant_for_all_nodes};

// Randomly add or remove some nodes, causing churn.
// If a new node was added, returns the index of this node. Otherwise
// returns `None` (it never adds more than one node).
//
// Note: it's necessary to call `poll_all` afterwards, as this function doesn't
// call it itself.
fn random_churn<R: Rng>(rng: &mut R,
                        network: &Network,
                        nodes: &mut Vec<TestNode>)
                        -> Option<usize> {
    let len = nodes.len();

    if len > network.min_group_size() + 2 && rng.gen_weighted_bool(3) {
        let _ = nodes.remove(rng.gen_range(0, len));
        let _ = nodes.remove(rng.gen_range(0, len - 1));
        let _ = nodes.remove(rng.gen_range(0, len - 2));

        None
    } else {
        let proxy = rng.gen_range(0, len);
        let index = rng.gen_range(0, len + 1);
        let config = Config::with_contacts(&[nodes[proxy].handle.endpoint()]);

        nodes.insert(index, TestNode::builder(network).config(config).create());
        Some(index)
    }
}

/// The entries of a Get request: the data ID, message ID, source and destination authority.
type GetKey = (DataIdentifier, MessageId, Authority, Authority);

/// A set of expectations: Which nodes and groups are supposed to receive Get requests.
#[derive(Default)]
struct ExpectedGets {
    /// The Get requests expected to be received.
    messages: HashSet<GetKey>,
    /// The group members of the receiving groups, at the time of sending.
    groups: HashMap<Authority, HashSet<XorName>>,
}

impl ExpectedGets {
    /// Sends a request using the nodes specified by `src`, and adds the expectation that the nodes
    /// belonging to `dst` receive the message. Panics if not enough nodes sent a group message, or
    /// if an individual sending node could not be found.
    fn send_and_expect(&mut self,
                       data_id: DataIdentifier,
                       src: Authority,
                       dst: Authority,
                       nodes: &[TestNode],
                       min_group_size: usize) {
        let src_dest = src.to_destination();
        let dst_dest = dst.to_destination();
        let msg_id = MessageId::new();
        let mut sent_count = 0;
        for node in nodes.iter().filter(|node| node.is_recipient(&src_dest)) {
            unwrap!(node.inner.send_get_request(src, dst, data_id, msg_id));
            sent_count += 1;
        }
        match src_dest {
            Destination::Group(_) => assert!(100 * sent_count >= QUORUM * min_group_size),
            Destination::Node(_) => assert_eq!(sent_count, 1),
        }
        if dst.is_group() && !self.groups.contains_key(&dst) {
            let is_recipient = |n: &&TestNode| n.is_recipient(&dst_dest);
            let group = nodes.iter().filter(is_recipient).map(TestNode::name).collect();
            let _ = self.groups.insert(dst, group);
        }
        self.messages.insert((data_id, msg_id, src, dst));
    }

    /// Verifies that all sent messages have been received by the appropriate nodes.
    fn verify(mut self, nodes: &[TestNode]) {
        // The minimum of the group lengths when sending and now. If a churn event happened, both
        // cases are valid: that the message was received before or after that. The number of
        // recipients thus only needs to reach a quorum for the smaller of the group sizes.
        let group_sizes: HashMap<_, _> = self.groups
            .iter_mut()
            .map(|(dst, group)| {
                let dst_dest = dst.to_destination();
                let is_recipient = |n: &&TestNode| n.is_recipient(&dst_dest);
                let new_group = nodes.iter().filter(is_recipient).map(TestNode::name).collect_vec();
                let count = cmp::min(group.len(), new_group.len());
                group.extend(new_group);
                (*dst, count)
            })
            .collect();
        let mut group_msgs_received = HashMap::new(); // The count of received group messages.
        for node in nodes {
            while let Ok(event) = node.event_rx.try_recv() {
                if let Event::Request { request: Request::Get(data_id, msg_id), src, dst } = event {
                    let key = (data_id, msg_id, src, dst);
                    if dst.is_group() {
                        assert!(self.groups
                                    .get(&key.3)
                                    .map_or(false, |entry| entry.contains(&node.name())),
                                "Unexpected request for node {:?}: {:?}",
                                node.name(),
                                key);
                        *group_msgs_received.entry(key).or_insert(0usize) += 1;
                    } else {
                        assert_eq!(node.name(), *dst.name());
                        assert!(self.messages.remove(&key),
                                "Unexpected request for node {:?}: {:?}",
                                node.name(),
                                key);
                    }
                }
            }
        }
        for key in self.messages {
            // All received messages for single nodes were removed: if any are left, they failed.
            assert!(key.3.is_group(), "Failed to receive request {:?}", key);
            let group_size = group_sizes[&key.3];
            let count = group_msgs_received.remove(&key).unwrap_or(0);
            assert!(100 * count >= QUORUM * group_size,
                    "Only received {} out of {} messages {:?}.",
                    count,
                    group_size,
                    key);
        }
    }
}

const CHURN_ITERATIONS: usize = 100;

#[test]
fn churn() {
    let min_group_size = 8;
    let network = Network::new(min_group_size, None);
    let mut rng = network.new_rng();
    let mut nodes = create_connected_nodes(&network, 20);

    for i in 0..CHURN_ITERATIONS {
        trace!("Iteration {}", i);
        let added_index = random_churn(&mut rng, &network, &mut nodes);

        // Create random data ID and pick random sending and receiving nodes.
        let data_id = DataIdentifier::Immutable(rng.gen());
        let index0 = gen_range_except(&mut rng, 0, nodes.len(), added_index);
        let index1 = gen_range_except(&mut rng, 0, nodes.len(), added_index);
        let auth_n0 = Authority::ManagedNode(nodes[index0].name());
        let auth_n1 = Authority::ManagedNode(nodes[index1].name());
        let auth_g0 = Authority::NaeManager(rng.gen());
        let auth_g1 = Authority::NaeManager(rng.gen());

        let mut expected_gets = ExpectedGets::default();

        // Test messages from a node to itself, another node and a group ...
        expected_gets.send_and_expect(data_id, auth_n0, auth_n0, &nodes, min_group_size);
        expected_gets.send_and_expect(data_id, auth_n0, auth_n1, &nodes, min_group_size);
        expected_gets.send_and_expect(data_id, auth_n0, auth_g0, &nodes, min_group_size);
        // ... and from a group to itself, another group and a node.
        expected_gets.send_and_expect(data_id, auth_g0, auth_g0, &nodes, min_group_size);
        expected_gets.send_and_expect(data_id, auth_g0, auth_g1, &nodes, min_group_size);
        expected_gets.send_and_expect(data_id, auth_g0, auth_n0, &nodes, min_group_size);

        poll_and_resend(&mut nodes, &mut []);

        expected_gets.verify(&nodes);
        verify_invariant_for_all_nodes(&nodes);

        // Every few iterations, clear the nodes' caches, simulating a longer time between events.
        if rng.gen_weighted_bool(5) {
            for node in &mut nodes {
                node.inner.clear_state();
            }
        }
    }
}
