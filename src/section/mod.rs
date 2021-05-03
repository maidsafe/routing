// Copyright 2020 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

mod member_info;
mod section_authority_provider;
mod section_chain;
mod section_keys;
mod section_peers;

#[cfg(test)]
pub(crate) use self::section_authority_provider::test_utils;
pub(crate) use self::section_peers::SectionPeers;
pub use self::{
    member_info::{
        MemberInfo, PeerState, FIRST_SECTION_MAX_AGE, FIRST_SECTION_MIN_AGE, MIN_ADULT_AGE, MIN_AGE,
    },
    section_authority_provider::SectionAuthorityProvider,
    section_chain::{Error as SectionChainError, SectionChain},
    section_keys::{SectionKeyShare, SectionKeysProvider},
};

use crate::{
    agreement::Proven,
    error::{Error, Result},
    peer::Peer,
    ELDER_SIZE, RECOMMENDED_SECTION_SIZE,
};
use bls_signature_aggregator::Proof;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, convert::TryInto, iter, net::SocketAddr};
use xor_name::{Prefix, XorName};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct Section {
    genesis_key: bls::PublicKey,
    chain: SectionChain,
    section_auth: Proven<SectionAuthorityProvider>,
    members: SectionPeers,
}

impl Section {
    /// Creates a minimal `Section` initially containing only info about our elders
    /// (`section_auth`).
    ///
    /// Returns error if `section_auth` is not signed with the last key of `chain`.
    pub fn new(
        genesis_key: bls::PublicKey,
        chain: SectionChain,
        section_auth: Proven<SectionAuthorityProvider>,
    ) -> Result<Self, Error> {
        if section_auth.proof.public_key != *chain.last_key() {
            error!("can't create section: section_auth signed with incorrect key");
            // TODO: consider more specific error here.
            return Err(Error::InvalidMessage);
        }

        Ok(Self {
            genesis_key,
            chain,
            section_auth,
            members: SectionPeers::default(),
        })
    }

    /// Creates `Section` for the first node in the network
    pub fn first_node(peer: Peer) -> Result<(Self, SectionKeyShare)> {
        let secret_key_set = bls::SecretKeySet::random(0, &mut rand::thread_rng());
        let public_key_set = secret_key_set.public_keys();
        let secret_key_share = secret_key_set.secret_key_share(0);

        let section_auth =
            create_first_section_authority_provider(&public_key_set, &secret_key_share, peer)?;

        let mut section = Self::new(
            section_auth.proof.public_key,
            SectionChain::new(section_auth.proof.public_key),
            section_auth,
        )?;

        for peer in section.section_auth.value.peers() {
            let member_info = MemberInfo::joined(peer);
            let proof = create_first_proof(&public_key_set, &secret_key_share, &member_info)?;
            let _ = section.members.update(Proven {
                value: member_info,
                proof,
            });
        }

        let section_key_share = SectionKeyShare {
            public_key_set,
            index: 0,
            secret_key_share,
        };

        Ok((section, section_key_share))
    }

    pub fn genesis_key(&self) -> &bls::PublicKey {
        &self.genesis_key
    }

    /// Try to merge this `Section` with `other`. Returns `InvalidMessage` if `other` is invalid or
    /// its chain is not compatible with the chain of `self`.
    pub fn merge(&mut self, other: Self) -> Result<()> {
        if !other.section_auth.self_verify() {
            error!("can't merge sections: other section_auth failed self-verification");
            return Err(Error::InvalidMessage);
        }
        if &other.section_auth.proof.public_key != other.chain.last_key() {
            // TODO: use more specific error variant.
            error!("can't merge sections: other section_auth signed with incorrect key");
            return Err(Error::InvalidMessage);
        }

        self.chain.merge(other.chain.clone())?;

        if &other.section_auth.proof.public_key == self.chain.last_key() {
            self.section_auth = other.section_auth;
        }

        for info in other.members {
            let _ = self.update_member(info);
        }

        self.members
            .prune_not_matching(&self.section_auth.value.prefix);

        Ok(())
    }

    /// Update the `SectionAuthorityProvider` of our section.
    pub fn update_elders(
        &mut self,
        new_section_auth: Proven<SectionAuthorityProvider>,
        new_key_proof: Proof,
    ) -> bool {
        if new_section_auth.value.prefix != *self.prefix()
            && !new_section_auth.value.prefix.is_extension_of(self.prefix())
        {
            return false;
        }

        if !new_section_auth.self_verify() {
            return false;
        }

        if let Err(error) = self.chain.insert(
            &new_key_proof.public_key,
            new_section_auth.proof.public_key,
            new_key_proof.signature,
        ) {
            error!(
                "failed to insert key {:?} (signed with {:?}) into the section chain: {}",
                new_section_auth.proof.public_key, new_key_proof.public_key, error,
            );
            return false;
        }

        if &new_section_auth.proof.public_key == self.chain.last_key() {
            self.section_auth = new_section_auth;
        }

        self.members
            .prune_not_matching(&self.section_auth.value.prefix);

        true
    }

    /// Update the member. Returns whether it actually changed anything.
    pub fn update_member(&mut self, member_info: Proven<MemberInfo>) -> bool {
        if !member_info.verify(&self.chain) {
            error!("can't merge member {:?}", member_info.value);
            return false;
        }

        self.members.update(member_info)
    }

    pub fn chain(&self) -> &SectionChain {
        &self.chain
    }

    // Extend the section chain so it starts at `trusted_key` while keeping the last key intact.
    pub(crate) fn extend_chain(
        &self,
        trusted_key: &bls::PublicKey,
        full_chain: &SectionChain,
    ) -> Result<Self, SectionChainError> {
        let chain = match self.chain.extend(trusted_key, full_chain) {
            Ok(chain) => chain,
            Err(SectionChainError::InvalidOperation) => {
                // This means the tip of the chain is not reachable from `trusted_key`.
                // Use the full chain instead as it is always trusted.
                self.chain.clone()
            }
            Err(error) => return Err(error),
        };

        Ok(Self {
            genesis_key: self.genesis_key,
            section_auth: self.section_auth.clone(),
            chain,
            members: self.members.clone(),
        })
    }

    pub fn authority_provider(&self) -> &SectionAuthorityProvider {
        &self.section_auth.value
    }

    pub fn proven_authority_provider(&self) -> &Proven<SectionAuthorityProvider> {
        &self.section_auth
    }

    pub fn is_elder(&self, name: &XorName) -> bool {
        self.authority_provider().elders.contains_key(name)
    }

    /// Generate a new section info(s) based on the current set of members.
    /// Returns a set of candidate SectionAuthorityProviders.
    pub fn promote_and_demote_elders(&self, our_name: &XorName) -> Vec<SectionAuthorityProvider> {
        if let Some((our_info, other_info)) = self.try_split(our_name) {
            return vec![our_info, other_info];
        }

        let expected_peers = self.elder_candidates(ELDER_SIZE);
        let expected_names: BTreeSet<_> = expected_peers.iter().map(Peer::name).collect();
        let current_names: BTreeSet<_> = self.authority_provider().elders.keys().collect();

        if expected_names == current_names {
            vec![]
        } else if expected_names.len() < crate::supermajority(current_names.len()) {
            warn!("ignore attempt to reduce the number of elders too much");
            vec![]
        } else {
            let new_info =
                SectionAuthorityProvider::new(expected_peers, self.authority_provider().prefix);
            vec![new_info]
        }
    }

    // Prefix of our section.
    pub fn prefix(&self) -> &Prefix {
        &self.authority_provider().prefix
    }

    pub fn members(&self) -> &SectionPeers {
        &self.members
    }

    /// Returns members that are either joined or are left but still elders.
    pub fn active_members(&self) -> impl Iterator<Item = &Peer> {
        self.members
            .all()
            .filter(move |info| {
                self.members.is_joined(info.peer.name()) || self.is_elder(info.peer.name())
            })
            .map(|info| &info.peer)
    }

    /// Returns adults from our section.
    pub fn adults(&self) -> impl Iterator<Item = &Peer> {
        self.members
            .mature()
            .filter(move |peer| !self.is_elder(peer.name()))
    }

    /// Returns live adults from our section.
    pub fn live_adults(&self) -> impl Iterator<Item = &Peer> {
        self.members.joined().filter_map(move |info| {
            if !self.is_elder(info.peer.name()) {
                Some(&info.peer)
            } else {
                None
            }
        })
    }

    pub fn find_joined_member_by_addr(&self, addr: &SocketAddr) -> Option<&Peer> {
        self.members
            .joined()
            .find(|info| info.peer.addr() == addr)
            .map(|info| &info.peer)
    }

    // Tries to split our section.
    // If we have enough mature nodes for both subsections, returns the SectionAuthorityProviders
    // of the two subsections. Otherwise returns `None`.
    fn try_split(
        &self,
        our_name: &XorName,
    ) -> Option<(SectionAuthorityProvider, SectionAuthorityProvider)> {
        let next_bit_index = if let Ok(index) = self.prefix().bit_count().try_into() {
            index
        } else {
            // Already at the longest prefix, can't split further.
            return None;
        };

        let next_bit = our_name.bit(next_bit_index);

        let (our_new_size, sibling_new_size) = self
            .members
            .mature()
            .map(|peer| peer.name().bit(next_bit_index) == next_bit)
            .fold((0, 0), |(ours, siblings), is_our_prefix| {
                if is_our_prefix {
                    (ours + 1, siblings)
                } else {
                    (ours, siblings + 1)
                }
            });

        // If none of the two new sections would contain enough entries, return `None`.
        if our_new_size < RECOMMENDED_SECTION_SIZE || sibling_new_size < RECOMMENDED_SECTION_SIZE {
            return None;
        }

        let our_prefix = self.prefix().pushed(next_bit);
        let other_prefix = self.prefix().pushed(!next_bit);

        let our_elders = self.members.elder_candidates_matching_prefix(
            &our_prefix,
            ELDER_SIZE,
            self.authority_provider(),
        );
        let other_elders = self.members.elder_candidates_matching_prefix(
            &other_prefix,
            ELDER_SIZE,
            self.authority_provider(),
        );

        let our_info = SectionAuthorityProvider::new(our_elders, our_prefix);
        let other_info = SectionAuthorityProvider::new(other_elders, other_prefix);

        Some((our_info, other_info))
    }

    // Returns the candidates for elders out of all the nodes in the section, even out of the
    // relocating nodes if there would not be enough instead.
    fn elder_candidates(&self, elder_size: usize) -> Vec<Peer> {
        self.members
            .elder_candidates(elder_size, self.authority_provider())
    }
}

// Create `SectionAuthorityProvider` for the first node.
fn create_first_section_authority_provider(
    pk_set: &bls::PublicKeySet,
    sk_share: &bls::SecretKeyShare,
    mut peer: Peer,
) -> Result<Proven<SectionAuthorityProvider>> {
    peer.set_reachable(true);
    let section_auth = SectionAuthorityProvider::new(iter::once(peer), Prefix::default());
    let proof = create_first_proof(pk_set, sk_share, &section_auth)?;
    Ok(Proven::new(section_auth, proof))
}

fn create_first_proof<T: Serialize>(
    pk_set: &bls::PublicKeySet,
    sk_share: &bls::SecretKeyShare,
    payload: &T,
) -> Result<Proof> {
    let bytes = bincode::serialize(payload).map_err(|_| Error::InvalidPayload)?;
    let signature_share = sk_share.sign(&bytes);
    let signature = pk_set
        .combine_signatures(iter::once((0, &signature_share)))
        .map_err(|_| Error::InvalidSignatureShare)?;

    Ok(Proof {
        public_key: pk_set.public_key(),
        signature,
    })
}
