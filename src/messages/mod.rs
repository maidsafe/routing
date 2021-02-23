// Copyright 2020 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

mod hash;
mod plain_message;
mod src_authority;
mod variant;

pub use self::{hash::MessageHash, src_authority::SrcAuthority};
pub(crate) use self::{
    plain_message::PlainMessage,
    variant::{JoinRequest, ResourceProofResponse, Variant},
};
use crate::{
    crypto::{self, name, Verifier},
    error::{Error, Result},
    node::Node,
    section::{ExtendError, SectionKeyShare, SectionProofChain, TrustStatus},
};
use bls_signature_aggregator::ProofShare;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use sn_messaging::DstLocation;
use std::fmt::{self, Debug, Formatter};
use thiserror::Error;
use xor_name::Prefix;

/// Message sent over the network.
#[derive(Clone, Eq, Serialize, Deserialize)]
pub(crate) struct Message {
    /// Source authority.
    /// Messages do not need to sign this field as it is all verifiable (i.e. if the sig validates
    /// agains the public key and we know the pub key then we are good. If the proof is not recognised we
    /// ask for a longer chain that can be recognised). Therefor we don't need to sign this field.
    src: SrcAuthority,
    /// Destination location.
    dst: DstLocation,
    /// The body of the message.
    variant: Variant,
    /// Proof chain to verify the message trust. Does not need to be signed.
    proof_chain: Option<SectionProofChain>,
    /// Source's knowledge of the destination section key. If present, the destination can use it
    /// to determine the length of the proof of messages sent to the source so the source would
    /// trust it (the proof needs to start at this key).
    dst_key: Option<bls::PublicKey>,
    /// Serialised message, this is a signed and fully serialised message ready to send.
    #[serde(skip)]
    serialized: Bytes,
    #[serde(skip)]
    hash: MessageHash,
}

impl Message {
    /// Deserialize the message. Only called on message receipt.
    pub(crate) fn from_bytes(msg_bytes: Bytes) -> Result<Self, CreateError> {
        let mut msg: Message = bincode::deserialize(&msg_bytes)?;

        let signed_bytes = bincode::serialize(&SignableView {
            dst: &msg.dst,
            dst_key: msg.dst_key.as_ref(),
            variant: &msg.variant,
        })?;

        match &msg.src {
            SrcAuthority::Node {
                public_key,
                signature,
                ..
            } => {
                if public_key.verify(&signed_bytes, signature).is_err() {
                    error!("Failed signature: {:?}", msg);
                    return Err(CreateError::FailedSignature);
                }
            }
            SrcAuthority::BlsShare { proof_share, .. } => {
                if !proof_share
                    .public_key_set
                    .public_key_share(proof_share.index)
                    .verify(&proof_share.signature_share, &signed_bytes)
                {
                    error!("Failed signature: {:?}", msg);
                    return Err(CreateError::FailedSignature);
                }
            }
            SrcAuthority::Section { signature, .. } => {
                if let Some(proof_chain) = msg.proof_chain.as_ref() {
                    // FIXME Assumes the nodes proof last key is the one signing this message
                    if !proof_chain.last_key().verify(signature, &signed_bytes) {
                        error!("Failed signature: {:?}", msg);
                        return Err(CreateError::FailedSignature);
                    }
                }
            }
        }

        msg.serialized = msg_bytes.clone();
        msg.hash = MessageHash::from_bytes(&msg_bytes);

        Ok(msg)
    }

    /// send across wire
    pub(crate) fn to_bytes(&self) -> Bytes {
        self.serialized.clone()
    }

    /// Creates a signed message where signature is assumed valid.
    fn new_signed(
        src: SrcAuthority,
        dst: DstLocation,
        variant: Variant,
        proof_chain: Option<SectionProofChain>,
        dst_key: Option<bls::PublicKey>,
    ) -> Result<Message, CreateError> {
        let mut msg = Message {
            dst,
            src,
            proof_chain,
            variant,
            dst_key,
            serialized: Default::default(),
            hash: Default::default(),
        };

        msg.serialized = bincode::serialize(&msg)?.into();
        msg.hash = MessageHash::from_bytes(&msg.serialized);

        Ok(msg)
    }

    /// Creates a message signed using a BLS KeyShare for
    /// destination accumulation
    pub(crate) fn for_dst_accumulation(
        node: &Node,
        key_share: &SectionKeyShare,
        dst: DstLocation,
        variant: Variant,
        proof_chain: Option<SectionProofChain>,
        dst_key: Option<bls::PublicKey>,
    ) -> Result<Self, CreateError> {
        let serialized = bincode::serialize(&SignableView {
            dst: &dst,
            dst_key: dst_key.as_ref(),
            variant: &variant,
        })?;
        let signature_share = key_share.secret_key_share.sign(&serialized);
        let proof_share = ProofShare {
            public_key_set: key_share.public_key_set.clone(),
            index: key_share.index,
            signature_share,
        };
        let src = SrcAuthority::BlsShare {
            proof_share,
            public_key: node.keypair.public,
            age: node.age,
        };

        Self::new_signed(src, dst, variant, proof_chain, dst_key)
    }

    pub(crate) fn signable_view(&self) -> SignableView {
        SignableView {
            dst: &self.dst,
            dst_key: self.dst_key.as_ref(),
            variant: &self.variant,
        }
    }

    /// Creates a signed message from single node.
    pub(crate) fn single_src(
        node: &Node,
        dst: DstLocation,
        variant: Variant,
        proof_chain: Option<SectionProofChain>,
        dst_key: Option<bls::PublicKey>,
    ) -> Result<Self, CreateError> {
        let serialized = bincode::serialize(&SignableView {
            dst: &dst,
            dst_key: dst_key.as_ref(),
            variant: &variant,
        })?;
        let signature = crypto::sign(&serialized, &node.keypair);
        let src = SrcAuthority::Node {
            public_key: node.keypair.public,
            age: node.age,
            signature,
        };

        Self::new_signed(src, dst, variant, proof_chain, dst_key)
    }

    /// Creates a signed message from a section.
    /// Note: `signature` isn't verified and is assumed valid.
    pub(crate) fn section_src(
        plain: PlainMessage,
        signature: bls::Signature,
        proof_chain: SectionProofChain,
    ) -> Result<Self, CreateError> {
        Self::new_signed(
            SrcAuthority::Section {
                prefix: plain.src,
                signature,
            },
            plain.dst,
            plain.variant,
            Some(proof_chain),
            Some(plain.dst_key),
        )
    }

    /// Verify this message is properly signed and trusted.
    pub(crate) fn verify<'a, I>(&'a self, trusted_keys: I) -> Result<VerifyStatus>
    where
        I: IntoIterator<Item = (&'a Prefix, &'a bls::PublicKey)>,
    {
        let bytes = bincode::serialize(&SignableView {
            dst: &self.dst,
            dst_key: self.dst_key.as_ref(),
            variant: &self.variant,
        })?;

        match &self.src {
            SrcAuthority::Node {
                public_key,
                signature,
                ..
            } => {
                if public_key.verify(&bytes, signature).is_err() {
                    return Err(Error::FailedSignature);
                }

                // Variant-specific verification.
                let trusted_keys = trusted_keys
                    .into_iter()
                    .filter(|(known_prefix, _)| known_prefix.matches(&name(public_key)))
                    .map(|(_, key)| key);
                self.variant.verify(self.proof_chain.as_ref(), trusted_keys)
            }
            SrcAuthority::BlsShare { proof_share, .. } => {
                if proof_share
                    .public_key_set
                    .public_key_share(proof_share.index)
                    .verify(&proof_share.signature_share, &bytes)
                {
                    Ok(VerifyStatus::Full)
                } else {
                    Err(Error::FailedSignature)
                }
            }
            SrcAuthority::Section { prefix, signature } => {
                // Proof chain is required for section-src messages.
                let proof_chain = if let Some(proof_chain) = self.proof_chain.as_ref() {
                    proof_chain
                } else {
                    return Err(Error::InvalidMessage);
                };

                if !proof_chain.last_key().verify(signature, &bytes) {
                    return Err(Error::FailedSignature);
                }

                let trusted_keys = trusted_keys
                    .into_iter()
                    .filter(|(known_prefix, _)| prefix.is_compatible(known_prefix))
                    .map(|(_, key)| key);

                proof_chain.check_trust(trusted_keys).into()
            }
        }
    }

    /// Getter
    pub fn dst(&self) -> &DstLocation {
        &self.dst
    }

    /// Getter
    pub fn variant(&self) -> &Variant {
        &self.variant
    }

    /// Getter
    pub fn src(&self) -> &SrcAuthority {
        &self.src
    }

    /// Getter
    pub fn dst_key(&self) -> &Option<bls::PublicKey> {
        &self.dst_key
    }
    /// Getter
    pub fn hash(&self) -> &MessageHash {
        &self.hash
    }

    /// Returns the attached proof chain, if any.
    pub(crate) fn proof_chain(&self) -> Result<&SectionProofChain> {
        self.proof_chain.as_ref().ok_or(Error::InvalidMessage)
    }

    /// Returns the last key of the attached the proof chain, if any.
    pub(crate) fn proof_chain_last_key(&self) -> Result<&bls::PublicKey> {
        self.proof_chain().map(|proof_chain| proof_chain.last_key())
    }

    // Extend the current message proof chain so it starts at `new_first_key` while keeping the
    // last key (and therefore the signature) intact.
    pub(crate) fn extend_proof_chain(
        mut self,
        new_first_key: &bls::PublicKey,
        full_chain: &SectionProofChain,
    ) -> Result<Self, ExtendProofChainError> {
        if let Variant::Sync { section, .. } = &mut self.variant {
            section.extend_chain(new_first_key, full_chain)?
        } else if let Some(proof_chain) = &mut self.proof_chain {
            proof_chain.extend(new_first_key, full_chain)?
        } else {
            return Err(ExtendProofChainError::NoProofChain);
        }

        Ok(Self::new_signed(
            self.src,
            self.dst,
            self.variant,
            self.proof_chain,
            self.dst_key,
        )?)
    }
}

// Ignore `serialized` and `hash` fields because they are only computed from the other fields and
// in some cases might be even absent.
impl PartialEq for Message {
    fn eq(&self, other: &Self) -> bool {
        self.src == other.src
            && self.dst == other.dst
            && self.variant == other.variant
            && self.proof_chain == other.proof_chain
            && self.dst_key == other.dst_key
    }
}

impl Debug for Message {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter
            .debug_struct("Message")
            .field("src", &self.src.src_location())
            .field("dst", &self.dst)
            .field("variant", &self.variant)
            .finish()
    }
}

#[derive(Eq, PartialEq, Debug)]
pub enum VerifyStatus {
    // The message has been fully verified.
    Full,
    // The message trust and integrity cannot be verified because it's proof is not trusted by us,
    // even though it is valid. The message should be relayed to other nodes who might be able to
    // verify it.
    Unknown,
}

impl Into<Result<VerifyStatus>> for TrustStatus {
    fn into(self) -> Result<VerifyStatus> {
        match self {
            Self::Trusted => Ok(VerifyStatus::Full),
            Self::Unknown => Ok(VerifyStatus::Unknown),
            Self::Invalid => Err(Error::InvalidMessage),
        }
    }
}

/// Status of an incomming message.
#[derive(Eq, PartialEq)]
pub enum MessageStatus {
    /// Message is useful and should be handled.
    Useful,
    /// Message is useless and should be discarded.
    Useless,
    /// Message trust can't be established.
    Untrusted,
    /// We don't know how to handle the message because we are not in the right state (e.g. it
    /// needs elder but we are not)
    Unknown,
}

#[derive(Debug, Error)]
pub enum CreateError {
    #[error("bincode error: {}", .0)]
    Bincode(#[from] bincode::Error),
    #[error("signature check failed")]
    FailedSignature,
}

impl From<CreateError> for Error {
    fn from(src: CreateError) -> Self {
        match src {
            CreateError::Bincode(inner) => Self::Bincode(inner),
            CreateError::FailedSignature => Self::FailedSignature,
        }
    }
}

/// Error returned from `Message::extend_proof_chain`.
#[derive(Debug, Error)]
pub enum ExtendProofChainError {
    #[error("message has no proof chain")]
    NoProofChain,
    #[error("failed to extend proof chain: {}", .0)]
    Extend(#[from] ExtendError),
    #[error("failed to re-create message: {}", .0)]
    Create(#[from] CreateError),
}

// View of a message that can be serialized for the purpose of signing.
#[derive(Serialize)]
pub(crate) struct SignableView<'a> {
    // TODO: why don't we include also `src`?
    pub dst: &'a DstLocation,
    pub dst_key: Option<&'a bls::PublicKey>,
    pub variant: &'a Variant,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        consensus, crypto,
        peer::Peer,
        section::{self, test_utils::gen_addr, MemberInfo},
        MIN_AGE,
    };
    use anyhow::Result;
    use std::iter;

    #[test]
    fn extend_proof_chain() -> Result<()> {
        let node = Node::new(crypto::gen_keypair(), gen_addr());

        let sk0 = bls::SecretKey::random();
        let pk0 = sk0.public_key();

        let sk1 = bls::SecretKey::random();
        let pk1 = sk1.public_key();

        let mut full_proof_chain = SectionProofChain::new(sk0.public_key());
        let pk1_sig = sk0.sign(&bincode::serialize(&pk1)?);
        let _ = full_proof_chain.push(pk1, pk1_sig);

        let (elders_info, _) = section::test_utils::gen_elders_info(Default::default(), 3);
        let elders_info = consensus::test_utils::proven(&sk1, elders_info)?;

        let peer = Peer::new(rand::random(), gen_addr(), MIN_AGE);
        let member_info = MemberInfo::joined(peer);
        let member_info = consensus::test_utils::proven(&sk1, member_info)?;

        let variant = Variant::NodeApproval {
            elders_info,
            member_info,
        };
        let message = Message::single_src(
            &node,
            DstLocation::Direct,
            variant,
            Some(full_proof_chain.slice(1..)),
            Some(pk1),
        )?;

        assert_eq!(
            message.verify(iter::once((&Prefix::default(), &pk1)))?,
            VerifyStatus::Full
        );
        assert_eq!(
            message.verify(iter::once((&Prefix::default(), &pk0)))?,
            VerifyStatus::Unknown
        );

        let message = message.extend_proof_chain(&pk0, &full_proof_chain)?;

        assert_eq!(
            message.verify(iter::once((&Prefix::default(), &pk0)))?,
            VerifyStatus::Full
        );

        Ok(())
    }
}
