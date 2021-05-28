// Copyright 2020 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

mod plain_message;
mod src_authority;
mod variant;

pub use self::src_authority::SrcAuthority;
pub(crate) use self::{
    plain_message::PlainMessage,
    variant::{JoinRequest, ResourceProofResponse, Variant},
};
use crate::{
    crypto::{self, Verifier},
    error::{Error, Result},
    node::Node,
    section::SectionKeyShare,
};
use bls_signature_aggregator::{Proof, ProofShare};
use bytes::Bytes;
use secured_linked_list::{error::Error as SecuredLinkedListError, SecuredLinkedList};
use serde::{Deserialize, Serialize};
use sn_messaging::{Aggregation, DstLocation, MessageId};
use std::fmt::{self, Debug, Formatter};
use thiserror::Error;
use xor_name::XorName;

/// Message sent over the network.
#[derive(Clone, Eq, Serialize, Deserialize)]
pub(crate) struct Message {
    /// Source authority.
    /// Messages do not need to sign this field as it is all verifiable (i.e. if the sig validates
    /// agains the public key and we know the pub key then we are good. If the proof is not recognised we
    /// ask for a longer chain that can be recognised). Therefore we don't need to sign this field.
    src: SrcAuthority,
    /// Destination location.
    dst: DstLocation,
    /// The aggregation scheme to be used.
    aggregation: Aggregation,
    /// The body of the message.
    variant: Variant,
    /// Proof chain to verify the message trust. Does not need to be signed.
    proof_chain: Option<SecuredLinkedList>,
    /// Serialised message, this is a signed and fully serialised message ready to send.
    #[serde(skip)]
    serialized: Bytes,
    /// message id
    id: MessageId,
}

impl Message {
    /// Deserialize the message. Only called on message receipt.
    pub(crate) fn from_bytes(msg_bytes: Bytes) -> Result<Self> {
        let mut msg: Message =
            bincode::deserialize(&msg_bytes).map_err(|_| Error::InvalidMessage)?;

        let signed_bytes = bincode::serialize(&SignableView {
            dst: &msg.dst,
            variant: &msg.variant,
        })
        .map_err(|_| Error::InvalidMessage)?;

        match &msg.src {
            SrcAuthority::Node {
                public_key,
                signature,
                ..
            } => {
                if public_key.verify(&signed_bytes, signature).is_err() {
                    error!("Failed signature: {:?}", msg);
                    return Err(Error::CreateError(CreateError::FailedSignature));
                }
            }
            SrcAuthority::BlsShare { proof_share, .. } => {
                if !proof_share.verify(&signed_bytes) {
                    error!("Failed signature: {:?}", msg);
                    return Err(Error::CreateError(CreateError::FailedSignature));
                }

                if Some(&proof_share.public_key_set.public_key()) != msg.proof_chain_last_key().ok()
                {
                    error!(
                        "Proof share public key doesn't match proof chain last key: {:?}",
                        msg
                    );
                    return Err(Error::CreateError(CreateError::FailedSignature));
                }
            }
            SrcAuthority::Section { proof, .. } => {
                if let Some(proof_chain) = msg.proof_chain.as_ref() {
                    if !proof_chain
                        .last_key()
                        .verify(&proof.signature, &signed_bytes)
                    {
                        error!(
                            "Failed signature: {:?} (proof chain: {:?})",
                            msg, proof_chain
                        );
                        return Err(Error::CreateError(CreateError::FailedSignature));
                    }
                }
            }
        }

        msg.serialized = msg_bytes;

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
        proof_chain: Option<SecuredLinkedList>,
    ) -> Result<Message, Error> {
        let id = src.id();
        let mut msg = Message {
            dst,
            src,
            aggregation: Aggregation::None,
            proof_chain,
            variant,
            serialized: Default::default(),
            id,
        };

        msg.serialized = bincode::serialize(&msg)
            .map_err(|_| Error::InvalidMessage)?
            .into();

        Ok(msg)
    }

    /// Creates a message signed using a BLS KeyShare for destination accumulation
    pub(crate) fn for_dst_accumulation(
        key_share: &SectionKeyShare,
        src_name: XorName,
        dst: DstLocation,
        variant: Variant,
        proof_chain: SecuredLinkedList,
    ) -> Result<Self, Error> {
        let serialized = bincode::serialize(&SignableView {
            dst: &dst,
            variant: &variant,
        })
        .map_err(|_| Error::InvalidMessage)?;
        let signature_share = key_share.secret_key_share.sign(&serialized);
        let proof_share = ProofShare {
            public_key_set: key_share.public_key_set.clone(),
            index: key_share.index,
            signature_share,
        };
        let src = SrcAuthority::BlsShare {
            src_name,
            proof_share,
        };

        Self::new_signed(src, dst, variant, Some(proof_chain))
    }

    /// Converts the message src authority from `BlsShare` to `Section` on successful accumulation.
    /// Returns errors if src is not `BlsShare` or if the proof is invalid.
    pub(crate) fn into_dst_accumulated(mut self, proof: Proof) -> Result<Self> {
        let (proof_share, src_name) = if let SrcAuthority::BlsShare {
            proof_share,
            src_name,
        } = &self.src
        {
            (proof_share.clone(), *src_name)
        } else {
            error!("not a message for dst accumulation");
            return Err(Error::InvalidMessage);
        };

        if proof_share.public_key_set.public_key() != proof.public_key {
            error!("proof public key doesn't match proof share public key");
            return Err(Error::InvalidMessage);
        }

        if Some(&proof.public_key) != self.proof_chain_last_key().ok() {
            error!("proof public key doesn't match proof chain last key");
            return Err(Error::InvalidMessage);
        }

        let bytes = bincode::serialize(&self.signable_view()).map_err(|_| Error::InvalidMessage)?;

        if !proof.verify(&bytes) {
            return Err(Error::FailedSignature);
        }

        self.src = SrcAuthority::Section { proof, src_name };

        Ok(self)
    }

    pub(crate) fn signable_view(&self) -> SignableView {
        SignableView {
            dst: &self.dst,
            variant: &self.variant,
        }
    }

    /// Creates a signed message from single node.
    pub(crate) fn single_src(
        node: &Node,
        dst: DstLocation,
        variant: Variant,
        proof_chain: Option<SecuredLinkedList>,
    ) -> Result<Self> {
        let serialized = bincode::serialize(&SignableView {
            dst: &dst,
            variant: &variant,
        })
        .map_err(|_| Error::InvalidMessage)?;
        let signature = crypto::sign(&serialized, &node.keypair);
        let src = SrcAuthority::Node {
            public_key: node.keypair.public,
            signature,
        };

        Self::new_signed(src, dst, variant, proof_chain)
    }

    /// Creates a signed message from a section.
    /// Note: `proof` isn't verified and is assumed valid.
    pub(crate) fn section_src(
        plain: PlainMessage,
        proof: Proof,
        proof_chain: SecuredLinkedList,
    ) -> Result<Self> {
        Self::new_signed(
            SrcAuthority::Section {
                src_name: plain.src,
                proof,
            },
            plain.dst,
            plain.variant,
            Some(proof_chain),
        )
    }

    /// Verify this message is properly signed and trusted.
    pub(crate) fn verify<'a, I>(&self, trusted_keys: I) -> Result<VerifyStatus>
    where
        I: IntoIterator<Item = &'a bls::PublicKey>,
    {
        let bytes = bincode::serialize(&SignableView {
            dst: &self.dst,
            variant: &self.variant,
        })
        .map_err(|_| Error::InvalidMessage)?;

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
                self.variant.verify(self.proof_chain.as_ref(), trusted_keys)
            }
            SrcAuthority::BlsShare { proof_share, .. } => {
                // Proof chain is required for accumulation at destination.
                let proof_chain = if let Some(proof_chain) = self.proof_chain.as_ref() {
                    proof_chain
                } else {
                    return Err(Error::InvalidMessage);
                };

                if proof_share.public_key_set.public_key() != *proof_chain.last_key() {
                    return Err(Error::InvalidMessage);
                }

                if !proof_share.verify(&bytes) {
                    return Err(Error::FailedSignature);
                }

                if proof_chain.check_trust(trusted_keys) {
                    Ok(VerifyStatus::Full)
                } else {
                    Ok(VerifyStatus::Unknown)
                }
            }
            SrcAuthority::Section { proof, .. } => {
                // Proof chain is required for section-src messages.
                let proof_chain = if let Some(proof_chain) = self.proof_chain.as_ref() {
                    proof_chain
                } else {
                    return Err(Error::InvalidMessage);
                };

                if !proof_chain.last_key().verify(&proof.signature, &bytes) {
                    return Err(Error::FailedSignature);
                }

                if proof_chain.check_trust(trusted_keys) {
                    Ok(VerifyStatus::Full)
                } else {
                    Ok(VerifyStatus::Unknown)
                }
            }
        }
    }

    /// Getter
    pub fn proof(&self) -> Option<Proof> {
        if let SrcAuthority::Section { proof, .. } = &self.src {
            Some(proof.clone())
        } else {
            None
        }
    }

    /// Getter
    pub fn dst(&self) -> &DstLocation {
        &self.dst
    }

    /// Get the MessageId
    pub fn id(&self) -> &MessageId {
        &self.id
    }

    /// Getter
    pub fn variant(&self) -> &Variant {
        &self.variant
    }

    /// Getter
    pub fn src(&self) -> &SrcAuthority {
        &self.src
    }

    /// Returns the attached proof chain, if any.
    pub(crate) fn proof_chain(&self) -> Result<&SecuredLinkedList> {
        self.proof_chain.as_ref().ok_or(Error::InvalidMessage)
    }

    /// Returns the last key of the attached the proof chain, if any.
    pub(crate) fn proof_chain_last_key(&self) -> Result<&bls::PublicKey> {
        self.proof_chain().map(|proof_chain| proof_chain.last_key())
    }

    // Extend the current message proof chain so it starts at `new_first_key` while keeping the
    // last key (and therefore the signature) intact.
    // NOTE: This operation doesn't invalidate the signatures because the proof chain is not part of
    // the signed data.
    pub(crate) fn extend_proof_chain(
        mut self,
        new_first_key: &bls::PublicKey,
        full_chain: &SecuredLinkedList,
    ) -> Result<Self, Error> {
        let proof_chain = self
            .proof_chain
            .as_mut()
            .ok_or(ExtendProofChainError::NoProofChain)?;

        *proof_chain = match proof_chain.extend(new_first_key, full_chain) {
            Ok(chain) => chain,
            Err(SecuredLinkedListError::InvalidOperation) => {
                // This means the tip of the proof chain is not reachable from `new_first_key`.
                // Extend it from the root key of the full chain instead as that should be the
                // genesis key which is implicitly trusted.
                proof_chain.extend(full_chain.root_key(), full_chain)?
            }
            Err(error) => return Err(error.into()),
        };

        Self::new_signed(self.src, self.dst, self.variant, self.proof_chain)
    }
}

// Ignore `serialized` and `hash` fields because they are only computed from the other fields and
// in some cases might be even absent.
impl PartialEq for Message {
    fn eq(&self, other: &Self) -> bool {
        self.src == other.src
            && self.dst == other.dst
            && self.id == other.id
            && self.variant == other.variant
            && self.proof_chain == other.proof_chain
    }
}

impl Debug for Message {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter
            .debug_struct("Message")
            .field("id", &self.id)
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

/// Status of an incomming message.
#[derive(Eq, PartialEq)]
pub enum MessageStatus {
    /// Message is useful and should be handled.
    Useful,
    /// Message is useless and should be discarded.
    Useless,
    /// Message trust can't be established.
    Untrusted,
}

#[derive(Debug, Error)]
pub enum CreateError {
    #[error("signature check failed")]
    FailedSignature,
    #[error("public key mismatch")]
    PublicKeyMismatch,
}

/// Error returned from `Message::extend_proof_chain`.
#[derive(Debug, Error)]
pub enum ExtendProofChainError {
    #[error("message has no proof chain")]
    NoProofChain,
    #[error("failed to extend proof chain: {}", .0)]
    Extend(#[from] SecuredLinkedListError),
    #[error("failed to re-create message: {}", .0)]
    Create(#[from] CreateError),
}

// View of a message that can be serialized for the purpose of signing.
#[derive(Serialize)]
pub(crate) struct SignableView<'a> {
    // TODO: why don't we include also `src`?
    pub dst: &'a DstLocation,
    pub variant: &'a Variant,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        agreement, crypto,
        peer::Peer,
        section::{self, test_utils::gen_addr, MemberInfo},
        MIN_ADULT_AGE,
    };
    use anyhow::Result;
    use std::iter;
    use xor_name::Prefix;

    #[test]
    fn extend_proof_chain() -> Result<()> {
        let node = Node::new(
            crypto::gen_keypair(&Prefix::default().range_inclusive(), MIN_ADULT_AGE),
            gen_addr(),
        );

        let sk0 = bls::SecretKey::random();
        let pk0 = sk0.public_key();

        let sk1 = bls::SecretKey::random();
        let pk1 = sk1.public_key();

        let mut full_proof_chain = SecuredLinkedList::new(pk0);
        let pk1_sig = sk0.sign(&bincode::serialize(&pk1)?);
        let _ = full_proof_chain.insert(&pk0, pk1, pk1_sig);

        let (section_auth, _, _) =
            section::test_utils::gen_section_authority_provider(Prefix::default(), 3);
        let section_auth = agreement::test_utils::proven(&sk1, section_auth)?;

        let peer = Peer::new(rand::random(), gen_addr());
        let member_info = MemberInfo::joined(peer);
        let member_info = agreement::test_utils::proven(&sk1, member_info)?;

        let variant = Variant::NodeApproval {
            genesis_key: pk0,
            section_auth,
            member_info,
        };
        let message = Message::single_src(
            &node,
            DstLocation::DirectAndUnrouted,
            variant,
            Some(full_proof_chain.truncate(1)),
        )?;

        assert_eq!(message.verify(iter::once(&pk1))?, VerifyStatus::Full);
        assert_eq!(message.verify(iter::once(&pk0))?, VerifyStatus::Unknown);

        let message = message.extend_proof_chain(&pk0, &full_proof_chain)?;

        assert_eq!(message.verify(iter::once(&pk0))?, VerifyStatus::Full);

        Ok(())
    }
}
