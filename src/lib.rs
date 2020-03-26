// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

//! P2PNode implementation for a resilient decentralised network infrastructure.
//!
//! This is the "engine room" of a hybrid p2p network, where the p2p nodes are built on
//! top of this library. The features this library gives us is:
//!
//!  * Sybil resistant p2p nodes
//!  * Sharded network with up to approx 200 p2p nodes per shard
//!  * All data encrypted at network level with TLS 1.3
//!  * Network level `quic` compatibility, satisfying industry standards and further
//!    obfuscating the p2p network data.
//!  * Upgrade capable nodes.
//!  * All network messages signed via ED25519 and/or BLS
//!  * Section consensus via an ABFT algorithm (PARSEC)
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maidsafe/QA/master/Images/maidsafe_logo.png",
    html_favicon_url = "https://maidsafe.net/img/favicon.ico",
    test(attr(forbid(warnings)))
)]
// For explanation of lint checks, run `rustc -W help` or see
// https://github.com/maidsafe/QA/blob/master/Documentation/Rust%20Lint%20Checks.md
#![forbid(
    exceeding_bitshifts,
    mutable_transmutes,
    no_mangle_const_items,
    unknown_crate_types,
    warnings
)]
#![deny(
    bad_style,
    improper_ctypes,
    missing_docs,
    non_shorthand_field_patterns,
    overflowing_literals,
    stable_features,
    unconditional_recursion,
    unknown_lints,
    unsafe_code,
    unused,
    unused_allocation,
    unused_attributes,
    unused_comparisons,
    unused_features,
    unused_parens,
    while_true,
    clippy::unicode_not_nfc,
    clippy::wrong_pub_self_convention,
    clippy::option_unwrap_used,
    deprecated
)]
#![warn(
    trivial_casts,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    unused_results,
    clippy::needless_borrow
)]

#[macro_use]
extern crate serde_derive;

// Needs to be before all other modules to make the macros available to them.
#[macro_use]
mod log_utils;
#[macro_use]
mod macros;

// ############################################################################
// Public API
// ############################################################################
pub use self::{
    error::RoutingError,
    id::{FullId, P2pNode, PublicId},
    location::{DstLocation, SrcLocation},
    node::{Node, NodeConfig},
    pause::PausedState,
    quic_p2p::Config as NetworkConfig,
    quic_p2p::Event as NetworkEvent,
    xor_space::{Prefix, XorName, XOR_NAME_LEN},
};
/// Routing events.
pub mod event;

// ############################################################################
// Mock and test API
// ############################################################################

/// Mocking utilities.
#[cfg(feature = "mock_base")]
pub mod mock;
/// Random number generation
#[cfg(feature = "mock_base")]
pub mod rng;

/// Mock network
#[cfg(feature = "mock_base")]
pub use self::{
    chain::{
        delivery_group_size, elders_info_for_test, quorum_count, section_proof_slice_for_test,
        NetworkParams, SectionKeyShare, MIN_AGE,
    },
    messages::{AccumulatingMessage, Message, PlainMessage, Variant},
    parsec::generate_bls_threshold_secret_key,
    relocation::Overrides as RelocationOverrides,
    xor_space::Xorable,
};

#[cfg(feature = "mock_base")]
#[doc(hidden)]
pub mod test_consts {
    pub use crate::{
        chain::{UNRESPONSIVE_THRESHOLD, UNRESPONSIVE_WINDOW},
        node::{BOOTSTRAP_TIMEOUT, JOIN_TIMEOUT},
        parsec::GOSSIP_PERIOD,
        transport::{RESEND_DELAY, RESEND_MAX_ATTEMPTS},
    };
}

#[cfg(feature = "mock")]
pub use self::mock::parsec::init_mock;

// ############################################################################
// Private
// ############################################################################

mod action;
mod chain;
mod core;
mod error;
mod id;
mod location;
mod message_filter;
mod messages;
mod node;
mod outbox;
mod parsec;
mod pause;
mod relocation;
#[cfg(not(feature = "mock_base"))]
mod rng;
mod signature_accumulator;
mod time;
mod timer;
mod transport;
mod utils;
mod xor_space;

// Cryptography
#[cfg(not(feature = "mock_base"))]
mod crypto;
#[cfg(feature = "mock_base")]
use self::mock::crypto;

/// Quorum is defined as having strictly greater than `QUORUM_NUMERATOR / QUORUM_DENOMINATOR`
/// agreement; using only integer arithmetic a quorum can be checked with
/// `votes * QUORUM_DENOMINATOR > voters * QUORUM_NUMERATOR`.
const QUORUM_NUMERATOR: usize = 2;
/// See `QUORUM_NUMERATOR`.
const QUORUM_DENOMINATOR: usize = 3;

/// Minimal safe section size. Routing will keep adding nodes until the section reaches this size.
/// More nodes might be added if requested by the upper layers.
/// This number also detemines when split happens - if both post-split sections would have at least
/// this number of nodes.
const SAFE_SECTION_SIZE: usize = 100;

/// Number of elders per section.
const ELDER_SIZE: usize = 7;

#[cfg(any(test, feature = "mock_base"))]
use unwrap::unwrap;

// Quic-p2p
#[cfg(feature = "mock_base")]
use mock_quic_p2p as quic_p2p;
#[cfg(not(feature = "mock_base"))]
use quic_p2p;

#[cfg(test)]
mod tests {
    use super::{QUORUM_DENOMINATOR, QUORUM_NUMERATOR};

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn quorum_check() {
        assert!(
            QUORUM_NUMERATOR < QUORUM_DENOMINATOR,
            "Quorum impossible to achieve"
        );
        assert!(
            QUORUM_NUMERATOR * 2 >= QUORUM_DENOMINATOR,
            "Quorum does not guarantee agreement"
        );
    }
}
