// Copyright 2019 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

pub use parsec::{ConsensusMode, Observation};

use super::{Block, NetworkEvent, Proof, PublicId, SecretId};
use crate::unwrap;
use bincode::serialize;
use std::{
    collections::{BTreeSet, HashSet},
    ops::Deref,
    rc::Rc,
};

/// Wrapper for `Observation` and optionally its creator, depending on the consensus mode.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub(super) enum ObservationHolder<T: NetworkEvent, P: PublicId> {
    Single {
        observation: Rc<Observation<T, P>>,
        creator: P,
    },
    Supermajority(Rc<Observation<T, P>>),
}

impl<T: NetworkEvent, P: PublicId> ObservationHolder<T, P> {
    pub fn new(observation: Observation<T, P>, creator: &P, consensus_mode: ConsensusMode) -> Self {
        match (&observation, consensus_mode) {
            (&Observation::OpaquePayload(_), ConsensusMode::Single) => ObservationHolder::Single {
                observation: Rc::new(observation),
                creator: creator.clone(),
            },
            _ => ObservationHolder::Supermajority(Rc::new(observation)),
        }
    }

    pub fn is_genesis(&self) -> bool {
        if let Observation::Genesis { .. } = ***self {
            true
        } else {
            false
        }
    }
}

impl<T: NetworkEvent, P: PublicId> Deref for ObservationHolder<T, P> {
    type Target = Rc<Observation<T, P>>;

    fn deref(&self) -> &Self::Target {
        match self {
            ObservationHolder::Single { observation, .. }
            | ObservationHolder::Supermajority(observation) => observation,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct ObservationState<P: PublicId> {
    votes: HashSet<Proof<P>>,
    // Flag flag indicating whether this observation has been seen by at least one peer other than
    // the creator. Only seen votes are considered when computing consensus.
    // This makes mock parsec behave closer to the real one in the sense that an observation cannot
    // reach consensus until it has been seen by other peers.
    seen: bool,
    consensused: bool,
}

impl<P: PublicId> ObservationState<P> {
    pub fn new() -> Self {
        Self {
            votes: Default::default(),
            seen: false,
            consensused: false,
        }
    }

    pub fn see(&mut self, our_id: &P) {
        if self.votes.iter().any(|proof| proof.public_id() != our_id) {
            self.seen = true;
        }
    }

    pub fn vote<T: NetworkEvent, S: SecretId<PublicId = P>>(
        &mut self,
        our_secret_id: &S,
        observation: &Rc<Observation<T, P>>,
    ) {
        let proof = our_secret_id.create_proof(&unwrap!(serialize(&**observation)));
        let _ = self.votes.insert(proof);
    }

    pub fn compute_consensus<T: NetworkEvent>(
        &mut self,
        peers: &BTreeSet<P>,
        consensus_mode: ConsensusMode,
        observation: &Rc<Observation<T, P>>,
    ) -> Option<Block<T, P>> {
        if peers.len() > 1 && !self.seen {
            return None;
        }

        if self.consensused {
            return None;
        }

        let num_valid_voters = self
            .votes
            .iter()
            .map(Proof::public_id)
            .filter(|peer_id| peers.contains(peer_id))
            .count();

        let consensused = match (&**observation, consensus_mode) {
            (&Observation::OpaquePayload(_), ConsensusMode::Single) => num_valid_voters > 0,
            _ => is_more_than_two_thirds(num_valid_voters, peers.len()),
        };

        if consensused {
            self.consensused = true;
            Some(Block::new(observation.clone(), &self.votes))
        } else {
            None
        }
    }
}

// Returns whether `small` is more than two thirds of `large`.
fn is_more_than_two_thirds(small: usize, large: usize) -> bool {
    3 * small > 2 * large
}
