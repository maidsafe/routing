// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

//! Peer implementation for a resilient decentralised network infrastructure.
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
    arithmetic_overflow,
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
// HACK: workaround for the "reached the type-length limit..." errors.
// TODO: find out whether there is any downside to doing this.
#![type_length_limit = "14621964"]

#[macro_use]
extern crate log;
#[macro_use]
extern crate serde;

// ############################################################################
// Public API
// ############################################################################
pub use self::{
    error::{Error, Result},
    location::{DstLocation, SrcLocation},
    network_params::NetworkParams,
    node::{EventStream, Node, NodeConfig},
    section::{SectionProofChain, MIN_AGE},
};
pub use qp2p::Config as TransportConfig;

pub use xor_name::{Prefix, XorName, XOR_NAME_LEN}; // TODO remove pub on API update
/// sn_routing events.
pub mod event;
pub mod log_ident;
/// Random number generation
pub mod rng;

// ############################################################################
// Private
// ############################################################################

mod cancellation;
mod consensus;
mod delivery_group;
mod error;
mod location;
mod message_filter;
mod messages;
mod network_params;
mod node;
mod peer;
mod relocation;
mod section;

// Cryptography
mod crypto;

/// Majority is defined as having strictly greater than `MAJORITY_NUMERATOR / MAJORITY_DENOMINATOR`
/// agreement; using only integer arithmetic a quorum can be checked with
/// `votes * MAJORITY_DENOMINATOR > voters * MAJORITY_NUMERATOR`.
const MAJORITY_NUMERATOR: usize = 2;
/// See `QUORUM_NUMERATOR`.
const MAJORITY_DENOMINATOR: usize = 3;

/// Recommended section size. sn_routing will keep adding nodes until the section reaches this size.
/// More nodes might be added if requested by the upper layers.
/// This number also detemines when split happens - if both post-split sections would have at least
/// this number of nodes.
const RECOMMENDED_SECTION_SIZE: usize = 60;

/// Number of elders per section.
const ELDER_SIZE: usize = 7;

#[cfg(test)]
mod tests {
    use super::{MAJORITY_DENOMINATOR, MAJORITY_NUMERATOR};

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn quorum_check() {
        assert!(
            MAJORITY_NUMERATOR < MAJORITY_DENOMINATOR,
            "Majority impossible to achieve"
        );
        assert!(
            MAJORITY_NUMERATOR * 2 >= MAJORITY_DENOMINATOR,
            "Majority does not guarantee agreement"
        );
    }
}
