// Copyright 2019 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use super::{Base, BOUNCE_RESEND_DELAY};
use crate::{
    chain::{
        AccumulatedEvent, AccumulatingEvent, Chain, EldersChange, EldersInfo, MemberState,
        OnlinePayload, PollAccumulated, Proof, ProofSet, SectionKeyInfo, SendAckMessagePayload,
    },
    error::{Result, RoutingError},
    event::Event,
    id::{P2pNode, PublicId},
    messages::{MemberKnowledge, MessageHash, Variant, VerifyStatus},
    outbox::EventBox,
    parsec::{self, Block, DkgResultWrapper, Observation, ParsecMap},
    relocation::{RelocateDetails, SignedRelocateDetails},
    state_machine::Transition,
    xor_space::{Prefix, XorName},
};
use bytes::Bytes;
use itertools::Itertools;
use rand::Rng;
use std::{collections::BTreeSet, net::SocketAddr};

/// Common functionality for node states post resource proof.
pub trait Approved: Base {
    fn parsec_map(&self) -> &ParsecMap;
    fn parsec_map_mut(&mut self) -> &mut ParsecMap;
    fn chain(&self) -> &Chain;
    fn chain_mut(&mut self) -> &mut Chain;
    fn send_event(&mut self, event: Event, outbox: &mut dyn EventBox);
    fn set_pfx_successfully_polled(&mut self, val: bool);
    fn is_pfx_successfully_polled(&self) -> bool;

    /// Handles an accumulated relocation trigger
    fn handle_relocate_polled(&mut self, details: RelocateDetails) -> Result<(), RoutingError>;

    /// Handles an accumulated change to our elders
    fn handle_promote_and_demote_elders(
        &mut self,
        new_infos: Vec<EldersInfo>,
    ) -> Result<(), RoutingError>;

    /// Handles a member added.
    fn handle_member_added(
        &mut self,
        payload: OnlinePayload,
        outbox: &mut dyn EventBox,
    ) -> Result<(), RoutingError>;

    /// Handles a member removed.
    fn handle_member_removed(
        &mut self,
        pub_id: PublicId,
        outbox: &mut dyn EventBox,
    ) -> Result<(), RoutingError>;

    /// Handle a member relocated.
    fn handle_member_relocated(
        &mut self,
        payload: RelocateDetails,
        node_knowledge: u64,
        outbox: &mut dyn EventBox,
    ) -> Result<(), RoutingError>;

    /// Handles a completed DKG.
    fn handle_dkg_result_event(
        &mut self,
        participants: &BTreeSet<PublicId>,
        dkg_result: &DkgResultWrapper,
    ) -> Result<(), RoutingError>;

    /// Handles an accumulated `SectionInfo` event.
    fn handle_section_info_event(
        &mut self,
        old_pfx: Prefix<XorName>,
        was_elder: bool,
        neighbour_change: EldersChange,
        outbox: &mut dyn EventBox,
    ) -> Result<Transition, RoutingError>;

    /// Handles an accumulated `NeighbourInfo` event.
    fn handle_neighbour_info_event(
        &mut self,
        elders_info: EldersInfo,
        neighbour_change: EldersChange,
    ) -> Result<(), RoutingError>;

    /// Handle an accumulated `TheirKeyInfo` event
    fn handle_their_key_info_event(&mut self, key_info: SectionKeyInfo)
        -> Result<(), RoutingError>;

    /// Handle an accumulated `SendAckMessage` event
    fn handle_send_ack_message_event(
        &mut self,
        ack_payload: SendAckMessagePayload,
    ) -> Result<(), RoutingError>;

    /// Handles an accumulated `Offline` event.
    fn handle_relocate_prepare_event(
        &mut self,
        payload: RelocateDetails,
        count_down: i32,
        outbox: &mut dyn EventBox,
    );

    /// Handle an accumulated `User` event
    fn handle_user_event(
        &mut self,
        payload: Vec<u8>,
        outbox: &mut dyn EventBox,
    ) -> Result<(), RoutingError> {
        self.send_event(Event::Consensus(payload), outbox);
        Ok(())
    }

    /// Handles an accumulated `ParsecPrune` event.
    fn handle_prune_event(&mut self) -> Result<(), RoutingError>;

    fn handle_parsec_request(
        &mut self,
        msg_version: u64,
        par_request: parsec::Request,
        p2p_node: P2pNode,
        outbox: &mut dyn EventBox,
    ) -> Result<Transition> {
        trace!(
            "{} - handle parsec request v{} from {} (last: v{})",
            self,
            msg_version,
            p2p_node.public_id(),
            self.parsec_map().last_version(),
        );

        let log_ident = self.log_ident();
        let response = self.parsec_map_mut().handle_request(
            msg_version,
            par_request,
            *p2p_node.public_id(),
            &log_ident,
        );

        if let Some(response) = response {
            trace!(
                "{} - send parsec response v{} to {:?}",
                self,
                msg_version,
                p2p_node,
            );
            self.send_direct_message(p2p_node.peer_addr(), response);
        }

        if msg_version == self.parsec_map().last_version() {
            self.parsec_poll(outbox)
        } else {
            Ok(Transition::Stay)
        }
    }

    fn handle_parsec_response(
        &mut self,
        msg_version: u64,
        par_response: parsec::Response,
        pub_id: PublicId,
        outbox: &mut dyn EventBox,
    ) -> Result<Transition> {
        trace!(
            "{} - handle parsec response v{} from {}",
            self,
            msg_version,
            pub_id
        );

        let log_ident = self.log_ident();
        self.parsec_map_mut()
            .handle_response(msg_version, par_response, pub_id, &log_ident);

        if msg_version == self.parsec_map().last_version() {
            self.parsec_poll(outbox)
        } else {
            Ok(Transition::Stay)
        }
    }

    fn send_parsec_gossip(&mut self, target: Option<(u64, P2pNode)>) {
        let (version, gossip_target) = match target {
            Some((v, p)) => (v, p),
            None => {
                let log_ident = self.log_ident();

                if !self.parsec_map_mut().should_send_gossip(&log_ident) {
                    return;
                }

                if let Some(recipient) = self.choose_gossip_recipient() {
                    let version = self.parsec_map().last_version();
                    (version, recipient)
                } else {
                    return;
                }
            }
        };

        match self
            .parsec_map_mut()
            .create_gossip(version, gossip_target.public_id())
        {
            Ok(msg) => {
                trace!(
                    "{} - send parsec request v{} to {:?}",
                    self,
                    version,
                    gossip_target,
                );
                self.send_direct_message(gossip_target.peer_addr(), msg);
            }
            Err(error) => {
                trace!(
                    "{} - failed to send parsec request v{} to {:?}: {:?}",
                    self,
                    version,
                    gossip_target,
                    error
                );
            }
        }
    }

    fn choose_gossip_recipient(&mut self) -> Option<P2pNode> {
        let recipients = self.parsec_map().gossip_recipients();
        if recipients.is_empty() {
            trace!("{} - not sending parsec request: no recipients", self,);
            return None;
        }

        let mut p2p_recipients: Vec<_> = recipients
            .into_iter()
            .filter_map(|pub_id| self.chain().get_member_p2p_node(pub_id.name()))
            .cloned()
            .collect();

        if p2p_recipients.is_empty() {
            log_or_panic!(
                log::Level::Error,
                "{} - not sending parsec request: not connected to any gossip recipient.",
                self
            );
            return None;
        }

        let rand_index = self.rng().gen_range(0, p2p_recipients.len());
        Some(p2p_recipients.swap_remove(rand_index))
    }

    fn parsec_poll(&mut self, outbox: &mut dyn EventBox) -> Result<Transition, RoutingError> {
        while let Some(block) = self.parsec_map_mut().poll() {
            let parsec_version = self.parsec_map_mut().last_version();
            match block.payload() {
                Observation::Accusation { .. } => {
                    // FIXME: Handle properly
                    unreachable!("...")
                }
                Observation::Genesis {
                    group,
                    related_info,
                } => {
                    // FIXME: Validate with Chain info.

                    trace!(
                        "{} Parsec Genesis {}: group {:?} - related_info {}",
                        self,
                        parsec_version,
                        group,
                        related_info.len()
                    );

                    self.chain_mut().handle_genesis_event(group, related_info)?;
                    self.set_pfx_successfully_polled(true);

                    continue;
                }
                Observation::OpaquePayload(event) => {
                    if let Some(proof) = block.proofs().iter().next().map(|p| Proof {
                        pub_id: *p.public_id(),
                        sig: *p.signature(),
                    }) {
                        trace!(
                            "{} Parsec OpaquePayload {}: {} - {:?}",
                            self,
                            parsec_version,
                            proof.pub_id(),
                            event
                        );
                        self.chain_mut().handle_opaque_event(event, proof)?;
                    }
                }
                Observation::Add { peer_id, .. } => {
                    log_or_panic!(
                        log::Level::Error,
                        "{} Unexpected Parsec Add {}: - {}",
                        self,
                        parsec_version,
                        peer_id
                    );
                }
                Observation::Remove { peer_id, .. } => {
                    log_or_panic!(
                        log::Level::Error,
                        "{} Unexpected Parsec Remove {}: - {}",
                        self,
                        parsec_version,
                        peer_id
                    );
                }
                obs @ Observation::StartDkg(_) | obs @ Observation::DkgMessage(_) => {
                    log_or_panic!(
                        log::Level::Error,
                        "parsec_poll polled internal Observation {}: {:?}",
                        parsec_version,
                        obs
                    );
                }
                Observation::DkgResult {
                    participants,
                    dkg_result,
                } => {
                    self.chain_mut()
                        .handle_dkg_result_event(participants, dkg_result)?;
                    self.handle_dkg_result_event(participants, dkg_result)?;
                }
            }

            match self.chain_poll(outbox)? {
                Transition::Stay => (),
                transition => return Ok(transition),
            }
        }

        self.check_voting_status();

        Ok(Transition::Stay)
    }

    fn chain_poll(&mut self, outbox: &mut dyn EventBox) -> Result<Transition, RoutingError> {
        let mut old_pfx = *self.chain_mut().our_prefix();
        let mut was_elder = self.chain().is_self_elder();

        while let Some(event) = self.chain_mut().poll_accumulated()? {
            match event {
                PollAccumulated::AccumulatedEvent(event) => {
                    match self.handle_accumulated_event(event, old_pfx, was_elder, outbox)? {
                        Transition::Stay => (),
                        transition => return Ok(transition),
                    }
                }
                PollAccumulated::RelocateDetails(details) => {
                    self.handle_relocate_polled(details)?;
                }
                PollAccumulated::PromoteDemoteElders(new_infos) => {
                    self.handle_promote_and_demote_elders(new_infos)?;
                }
            }

            old_pfx = *self.chain_mut().our_prefix();
            was_elder = self.chain().is_self_elder();
        }

        Ok(Transition::Stay)
    }

    fn handle_accumulated_event(
        &mut self,
        event: AccumulatedEvent,
        old_pfx: Prefix<XorName>,
        was_elder: bool,
        outbox: &mut dyn EventBox,
    ) -> Result<Transition, RoutingError> {
        trace!("{} Handle accumulated event: {:?}", self, event);

        match event.content {
            AccumulatingEvent::StartDkg(_) => {
                log_or_panic!(
                    log::Level::Error,
                    "StartDkg came out of Parsec - this shouldn't happen"
                );
            }
            AccumulatingEvent::Online(payload) => {
                self.handle_online_event(payload, outbox)?;
            }
            AccumulatingEvent::Offline(pub_id) => {
                self.handle_offline_event(pub_id, outbox)?;
            }
            AccumulatingEvent::SectionInfo(_, _) => {
                return self.handle_section_info_event(
                    old_pfx,
                    was_elder,
                    event.elders_change,
                    outbox,
                );
            }
            AccumulatingEvent::NeighbourInfo(elders_info) => {
                self.handle_neighbour_info_event(elders_info, event.elders_change)?;
            }
            AccumulatingEvent::TheirKeyInfo(key_info) => {
                self.handle_their_key_info_event(key_info)?
            }
            AccumulatingEvent::AckMessage(_payload) => {
                // Update their_knowledge is handled within the chain.
            }
            AccumulatingEvent::SendAckMessage(payload) => {
                self.handle_send_ack_message_event(payload)?
            }
            AccumulatingEvent::ParsecPrune => self.handle_prune_event()?,
            AccumulatingEvent::Relocate(payload) => self.handle_relocate_event(payload, outbox)?,
            AccumulatingEvent::RelocatePrepare(pub_id, count) => {
                self.handle_relocate_prepare_event(pub_id, count, outbox);
            }
            AccumulatingEvent::User(payload) => self.handle_user_event(payload, outbox)?,
        }

        Ok(Transition::Stay)
    }

    // Checking members vote status and vote to remove those non-resposive nodes.
    fn check_voting_status(&mut self) {
        let unresponsive_nodes = self.chain_mut().check_vote_status();
        let log_ident = self.log_ident();
        for pub_id in &unresponsive_nodes {
            info!("{} Voting for unresponsive node {:?}", log_ident, pub_id);
            self.parsec_map_mut().vote_for(
                AccumulatingEvent::Offline(*pub_id).into_network_event(),
                &log_ident,
            );
        }
    }

    fn disconnect_by_id_lookup(&mut self, pub_id: &PublicId) {
        if let Some(node) = self.chain().get_p2p_node(pub_id.name()) {
            let peer_addr = *node.peer_addr();
            self.network_service_mut().disconnect(peer_addr);
        } else {
            log_or_panic!(
                log::Level::Error,
                "{} - Can't disconnect from node we can't lookup in Chain: {}.",
                self,
                pub_id
            );
        };
    }

    fn handle_online_event(
        &mut self,
        payload: OnlinePayload,
        outbox: &mut dyn EventBox,
    ) -> Result<(), RoutingError> {
        if !self.chain().can_add_member(payload.p2p_node.public_id()) {
            info!("{} - ignore Online: {:?}.", self, payload);
        } else {
            info!("{} - handle Online: {:?}.", self, payload);

            let pub_id = *payload.p2p_node.public_id();
            self.chain_mut()
                .add_member(payload.p2p_node.clone(), payload.age);
            self.chain_mut().increment_age_counters(&pub_id);
            self.handle_member_added(payload, outbox)?;
        }

        Ok(())
    }

    fn handle_offline_event(
        &mut self,
        pub_id: PublicId,
        outbox: &mut dyn EventBox,
    ) -> Result<(), RoutingError> {
        if !self.chain().can_remove_member(&pub_id) {
            info!("{} - ignore Offline: {}.", self, pub_id);
        } else {
            info!("{} - handle Offline: {}.", self, pub_id);

            self.chain_mut().increment_age_counters(&pub_id);
            let _ = self.chain_mut().remove_member(&pub_id);
            self.disconnect_by_id_lookup(&pub_id);
            self.handle_member_removed(pub_id, outbox)?;
        }

        Ok(())
    }

    fn handle_relocate_event(
        &mut self,
        details: RelocateDetails,
        outbox: &mut dyn EventBox,
    ) -> Result<(), RoutingError> {
        if !self.chain().can_remove_member(&details.pub_id) {
            info!("{} - ignore Relocate: {:?} - not a member", self, details);
        } else {
            info!("{} - handle Relocate: {:?}.", self, details);

            match self.chain_mut().remove_member(&details.pub_id) {
                MemberState::Relocating { node_knowledge } => {
                    self.handle_member_relocated(details, node_knowledge, outbox)?;
                }
                state => {
                    log_or_panic!(
                        log::Level::Error,
                        "{} - Expected the state of {} to be Relocating, but was {:?}",
                        self,
                        details.pub_id,
                        state,
                    );
                }
            }
        }

        Ok(())
    }

    fn check_signed_relocation_details(&self, msg: &SignedRelocateDetails) -> bool {
        msg.signed_msg()
            .verify(self.chain().get_their_key_infos())
            .and_then(VerifyStatus::require_full)
            .map_err(|error| {
                self.log_verify_failure(
                    msg.signed_msg(),
                    &error,
                    self.chain().get_their_key_infos(),
                );
                error
            })
            .is_ok()
    }

    fn send_member_knowledge(&mut self) {
        let recipients = self
            .chain()
            .our_info()
            .member_nodes()
            .filter(|node| node.public_id() != self.id())
            .cloned()
            .collect_vec();
        let payload = MemberKnowledge {
            elders_version: self.chain().our_info().version(),
            parsec_version: self.parsec_map().last_version(),
        };

        trace!("{} - Send {:?} to {:?}", self, payload, recipients);

        for recipient in recipients {
            self.send_direct_message(recipient.peer_addr(), Variant::MemberKnowledge(payload))
        }
    }

    fn handle_bounce(&mut self, sender: P2pNode, sender_version: Option<u64>, msg_bytes: Bytes) {
        if let Some((_, version)) = self.chain().find_section_by_member(sender.public_id()) {
            if sender_version
                .map(|sender_version| sender_version < version)
                .unwrap_or(true)
            {
                trace!(
                    "{} - Received Bounce of {:?} from {}. Peer is lagging behind, resending in {:?}",
                    self,
                    MessageHash::from_bytes(&msg_bytes),
                    sender,
                    BOUNCE_RESEND_DELAY
                );
                self.send_message_to_target_later(
                    sender.peer_addr(),
                    msg_bytes,
                    BOUNCE_RESEND_DELAY,
                );
            } else {
                trace!(
                    "{} - Received Bounce of {:?} from {}. Peer has moved on, not resending",
                    self,
                    MessageHash::from_bytes(&msg_bytes),
                    sender
                );
            }
        } else {
            trace!(
                "{} - Received Bounce of {:?} from {}. Peer not known, not resending",
                self,
                MessageHash::from_bytes(&msg_bytes),
                sender
            );
        }
    }

    fn send_bounce(&mut self, recipient: &SocketAddr, msg_bytes: Bytes) {
        let variant = Variant::Bounce {
            elders_version: Some(self.chain().our_info().version()),
            message: msg_bytes,
        };

        self.send_direct_message(recipient, variant)
    }
}

#[allow(unused)]
fn to_proof_set(block: &Block) -> ProofSet {
    let sigs = block
        .proofs()
        .iter()
        .map(|proof| (*proof.public_id(), *proof.signature()))
        .collect();
    ProofSet { sigs }
}
