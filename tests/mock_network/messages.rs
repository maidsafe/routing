// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use super::{create_connected_nodes, gen_elder_index, gen_vec, poll_all};
use rand::Rng;
use routing::{
    event::Event, mock::Environment, quorum_count, DstLocation, NetworkParams, SrcLocation,
};

#[test]
fn send() {
    let elder_size = 8;
    let safe_section_size = 8;
    let quorum = quorum_count(elder_size);
    let env = Environment::new(NetworkParams {
        elder_size,
        safe_section_size,
    });
    let mut rng = env.new_rng();
    let mut nodes = create_connected_nodes(&env, elder_size + 1);

    let sender_index = gen_elder_index(&mut rng, &nodes);
    let src = SrcLocation::Node(nodes[sender_index].id());
    let dst = DstLocation::Section(rng.gen());
    let content = gen_vec(&mut rng, 1024);
    assert!(nodes[sender_index]
        .inner
        .send_message(src, dst, content.clone())
        .is_ok());

    let _ = poll_all(&mut nodes);

    let mut message_received_count = 0;
    for node in nodes
        .iter_mut()
        .filter(|n| n.inner.is_elder() && n.in_dst_location(&dst))
    {
        loop {
            match node.try_recv_event() {
                Some(Event::MessageReceived {
                    content: ref req_content,
                    ..
                }) => {
                    message_received_count += 1;
                    if content == *req_content {
                        break;
                    }
                }
                Some(_) => (),
                _ => panic!("{} - Event::MessageReceived not received", node.name()),
            }
        }
    }

    assert!(message_received_count >= quorum);
}

#[test]
fn send_and_receive() {
    let elder_size = 8;
    let safe_section_size = 8;
    let quorum = quorum_count(elder_size);
    let env = Environment::new(NetworkParams {
        elder_size,
        safe_section_size,
    });
    let mut rng = env.new_rng();
    let mut nodes = create_connected_nodes(&env, elder_size + 1);

    let sender_index = gen_elder_index(&mut rng, &nodes);
    let src = SrcLocation::Node(nodes[sender_index].id());
    let dst = DstLocation::Section(rng.gen());

    let req_content = gen_vec(&mut rng, 10);
    let res_content = gen_vec(&mut rng, 11);

    assert!(nodes[sender_index]
        .inner
        .send_message(src, dst, req_content.clone())
        .is_ok());

    let _ = poll_all(&mut nodes);

    let mut request_received_count = 0;

    for node in nodes
        .iter_mut()
        .filter(|n| n.inner.is_elder() && n.in_dst_location(&dst))
    {
        loop {
            match node.try_recv_event() {
                Some(Event::MessageReceived { content, src, .. }) => {
                    request_received_count += 1;
                    if req_content == content {
                        let res_src = SrcLocation::Section(*node.our_prefix());
                        let res_dst = match src {
                            SrcLocation::Node(id) => DstLocation::Node(*id.name()),
                            _ => panic!("Unexpected src location: {:?}", src),
                        };

                        if let Err(err) =
                            node.inner
                                .send_message(res_src, res_dst, res_content.clone())
                        {
                            trace!("Failed to send message: {:?}", err);
                        }
                        break;
                    }
                }
                Some(_) => (),
                _ => panic!("Event::MessageReceived not received"),
            }
        }
    }

    assert!(request_received_count >= quorum);

    let _ = poll_all(&mut nodes);

    let mut response_received_count = 0;

    loop {
        match nodes[sender_index].try_recv_event() {
            Some(Event::MessageReceived { content, .. }) => {
                response_received_count += 1;
                if res_content == content {
                    break;
                }
            }
            Some(_) => (),
            _ => panic!("Event::MessageReceived not received"),
        }
    }

    assert_eq!(response_received_count, 1);
}
