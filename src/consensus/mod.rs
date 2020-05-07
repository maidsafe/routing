// Copyright 2020 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

mod event_accumulator;
mod genesis_prefix_info;
mod network_event;
mod parsec;
mod proof;

pub use self::{
    event_accumulator::{AccumulatingProof, InsertError},
    genesis_prefix_info::GenesisPrefixInfo,
    network_event::{
        AccumulatingEvent, EventSigPayload, IntoAccumulatingEvent, NeighbourEldersRemoved,
        NetworkEvent, OnlinePayload,
    },
    parsec::{
        generate_bls_threshold_secret_key, generate_first_dkg_result, Block, CreateGossipError,
        DkgResult, DkgResultWrapper, Observation, ParsecNetworkEvent, Request as ParsecRequest,
        Response as ParsecResponse, GOSSIP_PERIOD,
    },
    proof::{Proof, ProofSet},
};

#[cfg(feature = "mock_base")]
pub use self::event_accumulator::{UNRESPONSIVE_THRESHOLD, UNRESPONSIVE_WINDOW};

use self::{
    event_accumulator::{EventAccumulator, RemainingEvents},
    parsec::ParsecMap,
};
use crate::{
    id::{FullId, PublicId},
    messages::Variant,
    rng::MainRng,
    section::EldersInfo,
    time::Duration,
};
use std::collections::BTreeSet;

// Distributed consensus mechanism backed by the Parsec algorithm.
pub struct ConsensusEngine {
    parsec_map: ParsecMap,
    accumulator: EventAccumulator,
}

impl ConsensusEngine {
    pub fn new(
        rng: &mut MainRng,
        full_id: FullId,
        elders_info: &EldersInfo,
        serialised_state: Vec<u8>,
        parsec_version: u64,
    ) -> Self {
        let mut parsec_map = ParsecMap::default();
        parsec_map.init(rng, full_id, elders_info, serialised_state, parsec_version);

        Self {
            parsec_map,
            accumulator: EventAccumulator::default(),
        }
    }

    /// Returns the next consensused and accumulated event, if any.
    pub fn poll(
        &mut self,
        our_elders: &EldersInfo,
    ) -> Option<(AccumulatingEvent, AccumulatingProof)> {
        while let Some(block) = self.parsec_map.poll() {
            if let Some(output) = self.handle_parsec_block(block, our_elders) {
                return Some(output);
            }
        }

        None
    }

    fn handle_parsec_block(
        &mut self,
        block: Block,
        our_elders: &EldersInfo,
    ) -> Option<(AccumulatingEvent, AccumulatingProof)> {
        // TODO: implement Block::into_payload in parsec to avoid cloning.
        match block.payload() {
            Observation::Genesis {
                group,
                related_info,
            } => {
                // FIXME: Validate with Chain info.

                trace!(
                    "Parsec Genesis v{}: group: {:?}, related_info: {}",
                    self.parsec_map.last_version(),
                    group,
                    related_info.len()
                );

                Some((
                    AccumulatingEvent::Genesis {
                        group: group.clone(),
                        related_info: related_info.clone(),
                    },
                    AccumulatingProof::default(),
                ))
            }
            Observation::OpaquePayload(event) => {
                let proof = block.proofs().iter().next()?;
                let proof = Proof {
                    pub_id: *proof.public_id(),
                    sig: *proof.signature(),
                };

                trace!(
                    "Parsec OpaquePayload v{}: {} - {:?}",
                    self.parsec_map.last_version(),
                    proof.pub_id(),
                    event
                );

                let (event, signature) = AccumulatingEvent::from_network_event(event.clone());

                // TODO: merge these three steps (add_proof, incomplete_events, poll_event) into a
                // single one, to make the process less fragile.
                match self.accumulator.add_proof(event, proof, signature) {
                    Ok(()) | Err(InsertError::AlreadyComplete) => {
                        // Proof added or event already completed.
                    }
                    Err(InsertError::ReplacedAlreadyInserted) => {
                        // TODO: If detecting duplicate vote from peer, penalise.
                        log_or_panic!(log::Level::Warn, "Duplicate proof in the accumulator");
                    }
                }

                let event = self
                    .accumulator
                    .unaccumulated_events()
                    .find(|(event, proofs)| {
                        self.is_accumulated(event, proofs.parsec_proof_set(), our_elders)
                    })
                    .map(|(event, _)| event.clone())?;

                self.accumulator
                    .poll_event(event, our_elders.elder_ids().cloned().collect())
            }
            Observation::DkgResult {
                participants,
                dkg_result,
            } => {
                trace!(
                    "Parsec DkgResult v{}: {:?}",
                    self.parsec_map.last_version(),
                    participants
                );
                Some((
                    AccumulatingEvent::DkgResult {
                        participants: participants.clone(),
                        dkg_result: dkg_result.clone(),
                    },
                    AccumulatingProof::default(),
                ))
            }
            Observation::Add { .. }
            | Observation::Remove { .. }
            | Observation::Accusation { .. }
            | Observation::StartDkg(_)
            | Observation::DkgMessage(_) => {
                log_or_panic!(
                    log::Level::Error,
                    "unexpected Parsec observation v{}: {:?}",
                    self.parsec_map.last_version(),
                    block.payload()
                );
                None
            }
        }
    }

    fn is_accumulated(
        &self,
        event: &AccumulatingEvent,
        proofs: &ProofSet,
        our_elders: &EldersInfo,
    ) -> bool {
        match event {
            AccumulatingEvent::SectionInfo(..)
            | AccumulatingEvent::Online(_)
            | AccumulatingEvent::Offline(_)
            | AccumulatingEvent::NeighbourInfo(_)
            | AccumulatingEvent::SendNeighbourInfo(_)
            | AccumulatingEvent::TheirKeyInfo { .. }
            | AccumulatingEvent::TheirKnowledge { .. }
            | AccumulatingEvent::ParsecPrune
            | AccumulatingEvent::Relocate(_)
            | AccumulatingEvent::RelocatePrepare(_, _)
            | AccumulatingEvent::User(_) => our_elders.is_quorum(proofs),

            AccumulatingEvent::Genesis { .. }
            | AccumulatingEvent::StartDkg(_)
            | AccumulatingEvent::DkgResult { .. } => unreachable!(
                "unexpected event present in the event accumulator: {:?}",
                event
            ),
        }
    }

    // Prepares for reset of the consensus engine. Returns all events voted by us that have not
    // accumulated yet, so they can be voted for again. Should be followed by `finalise_reset`.
    pub fn prepare_reset(&mut self, our_id: &PublicId) -> Vec<NetworkEvent> {
        let RemainingEvents {
            unaccumulated_events,
            accumulated_events,
        } = self.accumulator.reset_accumulator(our_id);

        unaccumulated_events
            .into_iter()
            .chain(
                self.parsec_map
                    .our_unpolled_observations()
                    .filter_map(|obs| match obs {
                        parsec::Observation::OpaquePayload(event) => Some(event),

                        parsec::Observation::Genesis { .. }
                        | parsec::Observation::Add { .. }
                        | parsec::Observation::Remove { .. }
                        | parsec::Observation::Accusation { .. }
                        | parsec::Observation::StartDkg(_)
                        | parsec::Observation::DkgResult { .. }
                        | parsec::Observation::DkgMessage(_) => None,
                    })
                    .cloned(),
            )
            .filter(|event| !accumulated_events.contains(&event.payload))
            .collect()
    }

    // Finalises the reset of the consensus engine.
    pub fn finalise_reset(
        &mut self,
        rng: &mut MainRng,
        full_id: FullId,
        elders_info: &EldersInfo,
        serialised_state: Vec<u8>,
        parsec_version: u64,
    ) {
        self.parsec_map
            .init(rng, full_id, elders_info, serialised_state, parsec_version)
    }

    pub fn detect_unresponsive<'a>(
        &self,
        members: impl IntoIterator<Item = &'a PublicId>,
    ) -> BTreeSet<PublicId> {
        self.accumulator.detect_unresponsive(members)
    }

    pub fn vote_for(&mut self, event: NetworkEvent) {
        self.parsec_map.vote_for(event)
    }

    pub fn create_gossip(
        &mut self,
        version: u64,
        target: &PublicId,
    ) -> Result<Variant, CreateGossipError> {
        self.parsec_map.create_gossip(version, target)
    }

    pub fn handle_parsec_request(
        &mut self,
        msg_version: u64,
        request: ParsecRequest,
        pub_id: PublicId,
    ) -> Option<Variant> {
        self.parsec_map.handle_request(msg_version, request, pub_id)
    }

    pub fn handle_parsec_response(
        &mut self,
        msg_version: u64,
        response: ParsecResponse,
        pub_id: PublicId,
    ) {
        self.parsec_map
            .handle_response(msg_version, response, pub_id)
    }

    pub fn prune_if_needed(&mut self) {
        self.parsec_map.prune_if_needed()
    }

    pub fn parsec_version(&self) -> u64 {
        self.parsec_map.last_version()
    }

    pub fn gossip_period(&self) -> Duration {
        self.parsec_map.gossip_period()
    }

    pub fn reset_gossip_period(&mut self) {
        self.parsec_map.reset_gossip_period()
    }

    pub fn should_send_gossip(&mut self) -> bool {
        self.parsec_map.should_send_gossip()
    }

    pub fn gossip_recipients(&self) -> Vec<&PublicId> {
        self.parsec_map.gossip_recipients()
    }

    #[cfg(feature = "mock_base")]
    pub fn parsec_map(&self) -> &ParsecMap {
        &self.parsec_map
    }

    #[cfg(all(test, feature = "mock"))]
    pub fn parsec_map_mut(&mut self) -> &mut ParsecMap {
        &mut self.parsec_map
    }
}
