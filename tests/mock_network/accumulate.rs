// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use super::{
    create_connected_nodes, gen_immutable_data, poll_all, sort_nodes_by_distance_to, TestNode,
};
use routing::{
    mock::Network, Authority, Event, EventStream, MessageId, Response, XorName,
    THRESHOLD_DENOMINATOR, THRESHOLD_NUMERATOR,
};


#[test]
fn messages_accumulate_with_quorum() {
    let section_size = 15;
    let network = Network::new(8, None);
    let mut rng = network.new_rng();
    let mut nodes = create_connected_nodes(&network, section_size);

    let data = gen_immutable_data(&mut rng, 8);
    let src = Authority::NaeManager(*data.name()); // The data's NaeManager.
    sort_nodes_by_distance_to(&mut nodes, &src.name());

    let send = |node: &mut TestNode, dst: &Authority<XorName>, message_id: MessageId| {
        assert!(node
            .inner
            .send_get_idata_response(src, *dst, Ok(data.clone()), message_id)
            .is_ok());
    };

    let dst = Authority::ManagedNode(nodes[0].name()); // The closest node.
                                                       // The smallest number such that
                                                       // `quorum * QUORUM_DENOMINATOR > section_size * QUORUM_NUMERATOR`:
    let quorum = 1 + section_size * THRESHOLD_NUMERATOR / THRESHOLD_DENOMINATOR;

    // Send a message from the section `src` to the node `dst`.
    // Only the `quorum`-th sender should cause accumulation and a
    // `Response` event. The event should only occur once.
    let message_id = MessageId::new();
    for node in nodes.iter_mut().take(quorum - 1) {
        send(node, &dst, message_id);
    }
    let _ = poll_all(&mut nodes, &mut []);
    expect_no_event!(nodes[0]);
    send(&mut nodes[quorum - 1], &dst, message_id);
    let _ = poll_all(&mut nodes, &mut []);
    expect_next_event!(nodes[0],
        Event::ResponseReceived { response: Response::GetIData { res: Ok(_), .. }, .. });
    send(&mut nodes[quorum], &dst, message_id);
    let _ = poll_all(&mut nodes, &mut []);
    expect_no_event!(nodes[0]);

    let dst_grp = Authority::Section(src.name()); // The whole section.

    // Send a message from the section `src` to the section `dst_grp`. Only the `quorum`-th sender
    // should cause accumulation and a `Response` event. The event should only occur once.
    let message_id = MessageId::new();
    for node in nodes.iter_mut().take(quorum - 1) {
        send(node, &dst_grp, message_id);
    }
    let _ = poll_all(&mut nodes, &mut []);
    for node in &mut *nodes {
        expect_no_event!(node);
    }
    send(&mut nodes[quorum - 1], &dst_grp, message_id);
    let _ = poll_all(&mut nodes, &mut []);
    for node in &mut *nodes {
        expect_next_event!(node,
            Event::ResponseReceived { response: Response::GetIData { res: Ok(_), .. }, .. });
    }
    send(&mut nodes[quorum], &dst_grp, message_id);
    let _ = poll_all(&mut nodes, &mut []);
    for node in &mut *nodes {
        expect_no_event!(node);
    }
}
