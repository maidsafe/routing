// Copyright 2015 MaidSafe.net limited.
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

//! Standalone CI test runner which starts a routing network with each node running in its own
//! thread.

// For explanation of lint checks, run `rustc -W help` or see
// https://github.com/maidsafe/QA/blob/master/Documentation/Rust%20Lint%20Checks.md
#![forbid(bad_style, exceeding_bitshifts, mutable_transmutes, no_mangle_const_items,
          unknown_crate_types, warnings)]
#![deny(deprecated, drop_with_repr_extern, improper_ctypes, missing_docs,
      non_shorthand_field_patterns, overflowing_literals, plugin_as_library,
      private_no_mangle_fns, private_no_mangle_statics, stable_features, unconditional_recursion,
      unknown_lints, unsafe_code, unused, unused_allocation, unused_attributes,
      unused_comparisons, unused_features, unused_parens, while_true)]
#![warn(trivial_casts, trivial_numeric_casts, unused_extern_crates, unused_import_braces,
        unused_qualifications, unused_results, variant_size_differences)]
#![allow(box_pointers, fat_ptr_transmutes, missing_copy_implementations,
         missing_debug_implementations)]

#![cfg(not(feature = "use-mock-crust"))]

#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]
#![cfg_attr(feature="clippy", deny(clippy, clippy_pedantic))]
#![cfg_attr(feature="clippy", allow(use_debug))]

extern crate itertools;
extern crate kademlia_routing_table;
#[cfg(target_os = "macos")]
extern crate libc;
#[macro_use]
extern crate maidsafe_utilities;
extern crate rand;
extern crate routing;
extern crate sodiumoxide;
extern crate xor_name;

mod utils;

use std::cmp::Ordering::{Greater, Less};
use std::collections::HashSet;
#[cfg(target_os = "macos")]
use std::io;
use std::{iter, thread};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use itertools::Itertools;
use kademlia_routing_table::GROUP_SIZE;
use maidsafe_utilities::serialisation;
use maidsafe_utilities::thread::RaiiThreadJoiner;
use routing::{Authority, Client, Data, Event, FullId, MessageId, Node, PlainData, RequestContent,
              RequestMessage, ResponseContent, ResponseMessage};
use sodiumoxide::crypto;
use sodiumoxide::crypto::hash::sha512;
use utils::recv_with_timeout;
use xor_name::XorName;

const QUORUM_SIZE: usize = 5;

#[derive(Debug)]
struct TestEvent(usize, Event);

struct TestNode {
    node: Node,
    _thread_joiner: RaiiThreadJoiner,
}

impl TestNode {
    fn new(index: usize, main_sender: Sender<TestEvent>) -> Self {
        let thread_name = format!("TestNode {} event sender", index);
        let (sender, joiner) = spawn_select_thread(index, main_sender, thread_name);

        TestNode {
            node: unwrap_result!(Node::new(sender, false)),
            _thread_joiner: joiner,
        }
    }

    fn name(&self) -> XorName {
        unwrap_result!(self.node.name())
    }
}

struct TestClient {
    index: usize,
    full_id: FullId,
    client: Client,
    _thread_joiner: RaiiThreadJoiner,
}

impl TestClient {
    fn new(index: usize, main_sender: Sender<TestEvent>) -> Self {
        let thread_name = format!("TestClient {} event sender", index);
        let (sender, joiner) = spawn_select_thread(index, main_sender, thread_name);

        let sign_keys = crypto::sign::gen_keypair();
        let encrypt_keys = crypto::box_::gen_keypair();
        let full_id = FullId::with_keys(encrypt_keys, sign_keys);

        TestClient {
            index: index,
            full_id: full_id.clone(),
            client: unwrap_result!(Client::new(sender, Some(full_id), false)),
            _thread_joiner: joiner,
        }
    }

    pub fn name(&self) -> &XorName {
        self.full_id.public_id().name()
    }
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn get_open_file_limits() -> io::Result<libc::rlimit> {
    unsafe {
        let mut result = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut result) != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(result)
    }
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn set_open_file_limits(limits: libc::rlimit) -> io::Result<()> {
    unsafe {
        if libc::setrlimit(libc::RLIMIT_NOFILE, &limits) != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn init() {
    unwrap_result!(maidsafe_utilities::log::init(true));
    let mut limits = unwrap_result!(get_open_file_limits());
    if limits.rlim_cur < 1024 {
        limits.rlim_cur = 1024;
        unwrap_result!(set_open_file_limits(limits));
    }
}

#[cfg(not(target_os = "macos"))]
fn init() {
    unwrap_result!(maidsafe_utilities::log::init(true));
}

// Spawns a thread that received events from a node a routes them to the main channel.
fn spawn_select_thread(index: usize,
                       main_sender: Sender<TestEvent>,
                       thread_name: String)
                       -> (Sender<Event>, RaiiThreadJoiner) {
    let (sender, receiver) = mpsc::channel();

    let thread_handle = thread!(thread_name, move || {
        for event in receiver.iter() {
            let _ = unwrap_result!(main_sender.send(TestEvent(index, event)));
        }
    });

    (sender, RaiiThreadJoiner::new(thread_handle))
}

fn wait_for_nodes_to_connect(nodes: &[TestNode],
                             connection_counts: &mut [usize],
                             event_receiver: &Receiver<TestEvent>) {
    // Wait for each node to connect to all the other nodes by counting churns.
    loop {
        if let Ok(test_event) = recv_with_timeout(&event_receiver, Duration::from_secs(30)) {
            if let TestEvent(index, Event::NodeAdded(_)) = test_event {
                connection_counts[index] += 1;

                let k = nodes.len();
                let all_events_received = (0..k)
                                              .map(|i| connection_counts[i])
                                              .all(|n| n >= k - 1 || n >= GROUP_SIZE - 1);
                if all_events_received {
                    break;
                }
            }
        } else {
            panic!("Timeout");
        }
    }
}

fn create_connected_nodes(count: usize,
                          event_sender: Sender<TestEvent>,
                          event_receiver: &Receiver<TestEvent>)
                          -> Vec<TestNode> {
    let mut nodes = Vec::with_capacity(count);
    let mut connection_counts = iter::repeat(0).take(count).collect::<Vec<usize>>();

    // Bootstrap node
    nodes.push(TestNode::new(0, event_sender.clone()));

    // HACK: wait until the above node switches to accepting mode. Would be
    // nice to know exactly when it happens instead of having to thread::sleep...
    thread::sleep(Duration::from_secs(2));

    // For each node, wait until it fully connects to the previous nodes before
    // continuing.
    for _ in 1..count {
        let index = nodes.len();
        nodes.push(TestNode::new(index, event_sender.clone()));
        wait_for_nodes_to_connect(&nodes, &mut connection_counts, event_receiver);
    }

    nodes
}

fn gen_plain_data() -> Data {
    let key: String = (0..10).map(|_| rand::random::<u8>() as char).collect();
    let value: String = (0..10).map(|_| rand::random::<u8>() as char).collect();
    let name = XorName::new(sha512::hash(key.as_bytes()).0);
    let data = unwrap_result!(serialisation::serialise(&(key, value)));

    Data::Plain(PlainData::new(name, data))
}

fn closest_nodes(node_names: &[XorName], target: &XorName) -> Vec<XorName> {
    node_names.iter()
              .sorted_by(|a, b| {
                  if xor_name::closer_to_target(a, b, target) {
                      Less
                  } else {
                      Greater
                  }
              })
              .into_iter()
              .take(GROUP_SIZE)
              .cloned()
              .collect()
}

// TODO: Extract the individual tests into their own functions.
#[cfg_attr(feature="clippy", allow(cyclomatic_complexity))]
fn core() {
    let (event_sender, event_receiver) = mpsc::channel();
    let mut nodes = create_connected_nodes(GROUP_SIZE + 1, event_sender.clone(), &event_receiver);

    {
        // request and response
        let client = TestClient::new(nodes.len(), event_sender.clone());
        let data = gen_plain_data();
        let message_id = MessageId::new();

        loop {
            if let Ok(test_event) = recv_with_timeout(&event_receiver, Duration::from_secs(20)) {
                match test_event {
                    TestEvent(index, Event::Connected) if index == client.index => {
                        // The client is connected now. Send some request.
                        let src = Authority::ClientManager(*client.name());
                        let result = client.client.send_put_request(src, data.clone(), message_id);
                        assert!(result.is_ok());
                    }

                    TestEvent(index, Event::Request(message)) => {
                        // A node received request from the client. Reply with a success.
                        if let RequestContent::Put(_, ref id) = message.content {
                            let encoded = unwrap_result!(serialisation::serialise(&message));
                            let node = &nodes[index].node;

                            unwrap_result!(node.send_put_success(message.dst,
                                                                 message.src,
                                                                 sha512::hash(&encoded),
                                                                 id.clone()));
                        }
                    }

                    TestEvent(index,
                              Event::Response(ResponseMessage{
                                content: ResponseContent::PutSuccess(_, id), .. }))
                        if index == client.index => {
                        // The client received response to its request. We are done.
                        assert_eq!(message_id, id);
                        break;
                    }

                    _ => (),
                }
            } else {
                panic!("Timeout");
            }
        }
    }

    {
        // request to group authority
        let node_names = nodes.iter().map(|node| node.name()).collect_vec();
        let client = TestClient::new(nodes.len(), event_sender.clone());
        let data = gen_plain_data();
        let mut close_group = closest_nodes(&node_names, client.name());

        loop {
            if let Ok(test_event) = recv_with_timeout(&event_receiver, Duration::from_secs(20)) {
                match test_event {
                    TestEvent(index, Event::Connected) if index == client.index => {
                        assert!(client.client
                                      .send_put_request(Authority::ClientManager(*client.name()),
                                                        data.clone(),
                                                        MessageId::new())
                                      .is_ok());
                    }
                    TestEvent(index,
                              Event::Request(RequestMessage{
                        content: RequestContent::Put(..), ..
                    })) => {
                        close_group.retain(|&name| name != nodes[index].name());

                        if close_group.is_empty() {
                            break;
                        }
                    }
                    _ => (),
                }
            } else {
                panic!("Timeout");
            }
        }

        assert!(close_group.is_empty());
    }

    {
        // response from group authority
        let node_names = nodes.iter().map(|node| node.name()).collect_vec();
        let client = TestClient::new(nodes.len(), event_sender.clone());
        let data = gen_plain_data();
        let mut close_group = closest_nodes(&node_names, client.name());

        loop {
            if let Ok(test_event) = recv_with_timeout(&event_receiver, Duration::from_secs(20)) {
                match test_event {
                    TestEvent(index, Event::Connected) if index == client.index => {
                        assert!(client.client
                                      .send_put_request(Authority::ClientManager(*client.name()),
                                                        data.clone(),
                                                        MessageId::new())
                                      .is_ok());
                    }
                    TestEvent(index,
                              Event::Request(RequestMessage{
                        src: Authority::Client{ .. },
                        dst: Authority::ClientManager(name),
                        content: RequestContent::Put(data, id),
                    })) => {
                        let src = Authority::ClientManager(name);
                        let dst = Authority::NaeManager(data.name().clone());
                        unwrap_result!(nodes[index]
                                           .node
                                           .send_put_request(src, dst, data.clone(), id.clone()));
                    }
                    TestEvent(index, Event::Request(ref msg)) => {
                        if let RequestContent::Put(_, ref id) = msg.content {
                            unwrap_result!(nodes[index].node.send_put_failure(msg.dst.clone(),
                                                                              msg.src.clone(),
                                                                              msg.clone(),
                                                                              vec![],
                                                                              id.clone()));
                        }
                    }
                    TestEvent(index,
                              Event::Response(ResponseMessage{
                                  content: ResponseContent::PutFailure{ .. },
                                  ..
                              })) => {
                        close_group.retain(|&name| name != nodes[index].name());

                        if close_group.is_empty() {
                            break;
                        }
                    }
                    _ => (),
                }
            } else {
                panic!("Timeout");
            }
        }

        assert!(close_group.is_empty());
    }

    {
        // leaving nodes cause churn
        let mut churns = iter::repeat(false).take(nodes.len() - 1).collect::<Vec<_>>();
        // a node leaves...
        let node = unwrap_option!(nodes.pop(), "No more nodes left.");
        let name = node.name();
        drop(node);

        loop {
            if let Ok(test_event) = recv_with_timeout(&event_receiver, Duration::from_secs(20)) {
                match test_event {
                    TestEvent(index, Event::NodeLost(lost_name)) if index < nodes.len() &&
                                                                    lost_name == name => {
                        churns[index] = true;
                        if churns.iter().all(|b| *b) {
                            break;
                        }
                    }

                    _ => (),
                }
            } else {
                panic!("Timeout");
            }
        }
    }

    {
        // joining nodes cause churn
        let nodes_len = nodes.len();
        let mut churns = iter::repeat(false).take(nodes_len + 1).collect::<Vec<_>>();
        // a node joins...
        nodes.push(TestNode::new(nodes_len, event_sender.clone()));

        loop {
            if let Ok(test_event) = recv_with_timeout(&event_receiver, Duration::from_secs(20)) {
                match test_event {
                    TestEvent(index, Event::NodeAdded(_)) if index < nodes.len() => {
                        churns[index] = true;
                        if churns.iter().all(|b| *b) {
                            break;
                        }
                    }

                    _ => (),
                }
            } else {
                panic!("Timeout");
            }
        }
    }

    {
        // message from quorum - 1 group members
        let client = TestClient::new(nodes.len(), event_sender.clone());
        let data = gen_plain_data();

        while let Ok(test_event) = recv_with_timeout(&event_receiver, Duration::from_secs(5)) {
            match test_event {
                TestEvent(index, Event::Connected) if index == client.index => {
                    assert!(client.client
                                  .send_put_request(Authority::ClientManager(*client.name()),
                                                    data.clone(),
                                                    MessageId::new())
                                  .is_ok());
                }
                TestEvent(index,
                          Event::Request(RequestMessage{ src: Authority::Client{ .. },
                                                        dst: Authority::ClientManager(name),
                                                        content: RequestContent::Put(data, id)
                          })) => {
                    let src = Authority::ClientManager(name);
                    let dst = Authority::NaeManager(data.name().clone());
                    unwrap_result!(nodes[index]
                                       .node
                                       .send_put_request(src, dst, data.clone(), id.clone()));
                }
                TestEvent(index, Event::Request(ref msg)) => {
                    if let RequestContent::Put(_, ref id) = msg.content {
                        if index < QUORUM_SIZE - 1 {
                            unwrap_result!(nodes[index].node.send_put_failure(msg.dst.clone(),
                                                                              msg.src.clone(),
                                                                              msg.clone(),
                                                                              vec![],
                                                                              id.clone()));
                        }
                    }
                }
                TestEvent(_index,
                          Event::Response(ResponseMessage{
                              content: ResponseContent::PutFailure{ .. },
                              ..
                          })) => {
                    panic!("Unexpected response.");
                }
                _ => (),
            }
        }
    }

    {
        // message from more than quorum group members
        let client = TestClient::new(nodes.len(), event_sender.clone());
        let data = gen_plain_data();
        let mut sent_ids = HashSet::new();
        let mut received_ids = HashSet::new();

        loop {
            if let Ok(test_event) = recv_with_timeout(&event_receiver, Duration::from_secs(5)) {
                match test_event {
                    TestEvent(index, Event::Connected) if index == client.index => {
                        // The client is connected now. Send some request.
                        let src = Authority::ClientManager(*client.name());
                        let message_id = MessageId::new();
                        let result = client.client.send_put_request(src, data.clone(), message_id);
                        assert!(result.is_ok());
                        sent_ids.insert(message_id);
                    }
                    TestEvent(index, Event::Request(message)) => {
                        // A node received request from the client. Reply with a success.
                        if let RequestContent::Put(_, id) = message.content {
                            let encoded = unwrap_result!(serialisation::serialise(&message));

                            unwrap_result!(nodes[index]
                                               .node
                                               .send_put_success(message.dst,
                                                                 message.src,
                                                                 sha512::hash(&encoded),
                                                                 id));
                        }
                    }
                    TestEvent(index,
                              Event::Response(ResponseMessage{
                                content: ResponseContent::PutSuccess(_, id), .. }))
                        if index == client.index => {
                        assert!(received_ids.insert(id));
                    }
                    _ => (),
                }
            } else {
                assert_eq!(1, received_ids.len());
                assert_eq!(sent_ids, received_ids);
                break;
            }
        }
    }
}

#[test]
fn main() {
    init();
    core();
}
