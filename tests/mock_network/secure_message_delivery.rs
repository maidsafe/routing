// Copyright 2019 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use super::{create_connected_nodes_until_split, poll_all, Nodes, TestNode};
use routing::{
    elders_info_for_test, generate_bls_threshold_secret_key, mock::Environment,
    section_proof_slice_for_test, AccumulatingMessage, DstLocation, FullId, Message, NetworkParams,
    P2pNode, PlainMessage, Prefix, SectionKeyShare, Variant, XorName,
};
use std::{collections::BTreeMap, iter, net::SocketAddr};

fn get_prefix(node: &TestNode) -> Prefix<XorName> {
    *unwrap!(node.inner.our_prefix())
}

fn get_position_with_other_prefix(nodes: &Nodes, prefix: &Prefix<XorName>) -> usize {
    unwrap!(nodes.iter().position(|node| get_prefix(node) != *prefix))
}

fn send_message(nodes: &mut Nodes, src: usize, dst: usize, message: Message) {
    let connection_info = unwrap!(nodes[dst].inner.our_connection_info());
    let targets = vec![connection_info];

    let _ = nodes[src]
        .inner
        .send_message_to_targets(&targets, 1, message);
}

enum FailType {
    TrustedProofInvalidSig,
    UntrustedProofValidSig,
}

// Create 2 sections, and then send a NeighbourInfo message from one to the other with
// a bad new SectionInfo signed by an unknown BLS Key. Either with a SectionProofChain containing
// that bad BLS key for `UntrustedProofValidSig` or a trusted SectionProofChain not containing it
// for `TrustedProofInvalidSig`.
fn message_with_invalid_security(fail_type: FailType) {
    // Arrange
    //
    let elder_size = 3;
    let safe_section_size = 3;
    let mut env = Environment::new(NetworkParams {
        elder_size,
        safe_section_size,
    });
    env.expect_panic();
    let mut rng = env.new_rng();

    let mut nodes = create_connected_nodes_until_split(&env, &[1, 1]);

    let their_node_pos = 0;
    let their_prefix = get_prefix(&nodes[their_node_pos]);

    let our_node_pos = get_position_with_other_prefix(&nodes, &their_prefix);
    let our_prefix = get_prefix(&nodes[our_node_pos]);

    let fake_full = FullId::gen(&mut env.new_rng());
    let bls_keys = generate_bls_threshold_secret_key(&mut rng, 1);
    let bls_secret_key_share = SectionKeyShare::new_with_position(0, bls_keys.secret_key_share(0));

    let socket_addr: SocketAddr = unwrap!("127.0.0.1:9999".parse());
    let members: BTreeMap<_, _> = iter::once((
        *fake_full.public_id(),
        P2pNode::new(*fake_full.public_id(), socket_addr),
    ))
    .collect();
    let new_info = unwrap!(elders_info_for_test(members, our_prefix, 10001));

    let content = PlainMessage {
        src: our_prefix,
        dst: DstLocation::Prefix(their_prefix),
        variant: Variant::NeighbourInfo(new_info),
    };

    let message = {
        let proof = match fail_type {
            FailType::TrustedProofInvalidSig => unwrap!(nodes[our_node_pos]
                .inner
                .prove(&DstLocation::Prefix(their_prefix))),
            FailType::UntrustedProofValidSig => {
                let invalid_prefix = our_prefix;
                section_proof_slice_for_test(0, invalid_prefix, bls_keys.public_keys().public_key())
            }
        };
        let pk_set = bls_keys.public_keys();

        let msg = unwrap!(AccumulatingMessage::new(
            content,
            &bls_secret_key_share,
            pk_set,
            proof
        ));
        unwrap!(msg.combine_signatures())
    };

    // Act/Assert:
    // poll_all will panic, when the receiving node process the message
    // and detect an invalid signature or proof.
    //
    send_message(&mut nodes, our_node_pos, their_node_pos, message);
    let _ = poll_all(&mut nodes);
}

#[test]
#[should_panic(expected = "FailedSignature")]
fn message_with_invalid_signature() {
    message_with_invalid_security(FailType::TrustedProofInvalidSig);
}

#[test]
#[should_panic(expected = "UntrustedMessage")]
fn message_with_invalid_proof() {
    message_with_invalid_security(FailType::UntrustedProofValidSig);
}
