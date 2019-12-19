// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use crate::{
    crypto::Digest256,
    messages::SignedRoutingMessage,
    time::{Duration, Instant},
};
use itertools::Itertools;
use std::collections::HashMap;

/// Time (in seconds) within which a message and a quorum of signatures need to arrive to
/// accumulate.
pub const ACCUMULATION_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Default)]
pub struct SignatureAccumulator {
    msgs: HashMap<Digest256, (Option<SignedRoutingMessage>, Instant)>,
}

impl SignatureAccumulator {
    /// Adds the given signature to the list of pending signatures or to the appropriate
    /// `SignedMessage`. Returns the message, if it has enough signatures now.
    pub fn add_proof(&mut self, msg: SignedRoutingMessage) -> Option<SignedRoutingMessage> {
        self.remove_expired();
        let hash = match msg.routing_message().hash() {
            Ok(hash) => hash,
            _ => {
                return None;
            }
        };
        if let Some(&mut (ref mut existing_msg, _)) = self.msgs.get_mut(&hash) {
            if let Some(existing_msg) = existing_msg {
                existing_msg.add_signature_shares(msg);
            }
        } else {
            let _ = self.msgs.insert(hash, (Some(msg), Instant::now()));
        }
        self.remove_if_complete(&hash)
    }

    fn remove_expired(&mut self) {
        let expired_msgs = self
            .msgs
            .iter()
            .filter(|&(_, &(_, ref time))| time.elapsed() > ACCUMULATION_TIMEOUT)
            .map(|(hash, _)| *hash)
            .collect_vec();
        for hash in expired_msgs {
            if let Some((Some(existing_msg), clock)) = self.msgs.remove(&hash) {
                error!(
                    "Remove unaccumulated expired message clock {:?}, msg {:?}",
                    clock, existing_msg,
                );
            }
        }
    }

    fn remove_if_complete(&mut self, hash: &Digest256) -> Option<SignedRoutingMessage> {
        self.msgs.get_mut(hash).and_then(|&mut (ref mut msg, _)| {
            if !msg.as_mut().map_or(false, |msg| msg.check_fully_signed()) {
                None
            } else {
                msg.take().map(|mut msg| {
                    msg.combine_signatures();
                    msg
                })
            }
        })
    }
}

#[cfg(test)]
#[cfg(feature = "mock_base")]
mod tests {
    use super::*;
    use crate::{
        chain::{EldersInfo, SectionKeyInfo, SectionKeyShare, SectionProofChain},
        id::{FullId, P2pNode},
        messages::{
            DirectMessage, MessageContent, RoutingMessage, SignedDirectMessage,
            SignedRoutingMessage,
        },
        parsec::generate_bls_threshold_secret_key,
        rng,
        routing_table::Authority,
        BlsPublicKeySet, ConnectionInfo, Prefix, XorName,
    };
    use itertools::Itertools;
    use rand;
    use std::collections::BTreeMap;
    use std::net::SocketAddr;
    use unwrap::unwrap;

    struct MessageAndSignatures {
        signed_msg: SignedRoutingMessage,
        signature_msgs: Vec<SignedDirectMessage>,
    }

    impl MessageAndSignatures {
        fn new(
            secret_ids: &BTreeMap<XorName, FullId>,
            all_nodes: &BTreeMap<XorName, P2pNode>,
            secret_bls_ids: &BTreeMap<XorName, SectionKeyShare>,
            pk_set: &BlsPublicKeySet,
        ) -> MessageAndSignatures {
            let routing_msg = RoutingMessage {
                src: Authority::Section(rand::random()),
                dst: Authority::Section(rand::random()),
                content: MessageContent::UserMessage(vec![
                    rand::random(),
                    rand::random(),
                    rand::random(),
                ]),
            };

            let msg_sender_secret_bls = unwrap!(secret_bls_ids.values().next());
            let other_ids = secret_ids.values().zip(secret_bls_ids.values()).skip(1);

            let prefix = Prefix::new(0, *unwrap!(all_nodes.keys().next()));
            let elders_info = unwrap!(EldersInfo::new(all_nodes.clone(), prefix, None));
            let key_info = SectionKeyInfo::from_elders_info(&elders_info, pk_set.public_key());
            let proof = SectionProofChain::from_genesis(key_info);
            let signed_msg = unwrap!(SignedRoutingMessage::new(
                routing_msg.clone(),
                msg_sender_secret_bls,
                pk_set.clone(),
                proof.clone(),
            ));
            let signature_msgs = other_ids
                .map(|(id, bls_id)| {
                    unwrap!(SignedDirectMessage::new(
                        DirectMessage::MessageSignature(unwrap!(SignedRoutingMessage::new(
                            routing_msg.clone(),
                            bls_id,
                            pk_set.clone(),
                            proof.clone(),
                        ))),
                        id,
                    ))
                })
                .collect();

            MessageAndSignatures {
                signed_msg,
                signature_msgs,
            }
        }
    }

    struct Env {
        msgs_and_sigs: Vec<MessageAndSignatures>,
    }

    impl Env {
        fn new() -> Env {
            let mut rng = rng::new();

            let socket_addr: SocketAddr = ([127, 0, 0, 1], 9999).into();
            let connection_info = ConnectionInfo::from(socket_addr);

            let keys = generate_bls_threshold_secret_key(&mut rng, 9);
            let full_ids: BTreeMap<_, _> = (0..9)
                .map(|_| {
                    let full_id = FullId::gen(&mut rng);
                    (*full_id.public_id().name(), full_id)
                })
                .collect();

            let pub_ids: BTreeMap<_, _> = full_ids
                .iter()
                .map(|(name, full_id)| {
                    (
                        *name,
                        P2pNode::new(*full_id.public_id(), connection_info.clone()),
                    )
                })
                .collect();

            let secret_ids: BTreeMap<_, _> = pub_ids
                .keys()
                .enumerate()
                .map(|(idx, name)| {
                    let share = SectionKeyShare::new_with_position(idx, keys.secret_key_share(idx));
                    (*name, share)
                })
                .collect();

            let pk_set = keys.public_keys();

            let msgs_and_sigs = (0..5)
                .map(|_| MessageAndSignatures::new(&full_ids, &pub_ids, &secret_ids, &pk_set))
                .collect();
            Env {
                msgs_and_sigs: msgs_and_sigs,
            }
        }
    }

    #[test]
    fn section_src_add_signature_last() {
        use fake_clock::FakeClock;

        let mut sig_accumulator = SignatureAccumulator::default();
        let env = Env::new();

        // Add each message with the section list added - none should accumulate.
        env.msgs_and_sigs.iter().foreach(|msg_and_sigs| {
            let signed_msg = msg_and_sigs.signed_msg.clone();
            let result = sig_accumulator.add_proof(signed_msg);
            assert!(result.is_none());
        });
        let expected_msgs_count = env.msgs_and_sigs.len();
        assert_eq!(sig_accumulator.msgs.len(), expected_msgs_count);

        // Add each message's signatures - each should accumulate once quorum has been reached.
        let mut count = 0;
        env.msgs_and_sigs.iter().foreach(|msg_and_sigs| {
            msg_and_sigs.signature_msgs.iter().foreach(|signature_msg| {
                let old_num_msgs = sig_accumulator.msgs.len();

                let result = match signature_msg.content() {
                    DirectMessage::MessageSignature(msg) => sig_accumulator.add_proof(msg.clone()),
                    ref unexpected_msg => panic!("Unexpected message: {:?}", unexpected_msg),
                };

                if let Some(mut returned_msg) = result {
                    // the message hash is not being removed upon accumulation, only when it
                    // expires
                    assert_eq!(sig_accumulator.msgs.len(), old_num_msgs);
                    assert_eq!(
                        msg_and_sigs.signed_msg.routing_message(),
                        returned_msg.routing_message()
                    );
                    assert!(returned_msg.check_fully_signed());
                    count += 1;
                }
            });
        });

        assert_eq!(count, expected_msgs_count);

        FakeClock::advance_time(ACCUMULATION_TIMEOUT.as_secs() * 1000 + 1000);

        sig_accumulator.remove_expired();
        assert!(sig_accumulator.msgs.is_empty());
    }
}
