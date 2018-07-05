// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

//! Client and node implementations for a resilient decentralised network.
//!
//! The network is based on the [`kademlia_routing_table`][1] and uses the XOR metric to define the
//! "distance" between two [`XorName`][2]s. `XorName`s are used as addresses of nodes, clients as
//! well as data.
//!
//! [1]: ../kademlia_routing_table/index.html
//! [2]: ../xor_name/struct.XorName.html
//!
//! Messages are exchanged between _authorities_, where an `Authority` can be an individual client
//! or node, or a collection of nodes called a "section", or a subset of a section called a "group".
//! In all cases, messages are cryptographically signed by the sender, and in the case of sections
//! and groups, it is verified that a sufficient number of members agree on the message: only if
//! that quorum is reached, the message is delivered. In addition, each message has a unique ID, and
//! is delivered only once.
//!
//! Section and group authorities are also addressed using a single `XorName`. The members are the
//! nodes that are closest to that name. Sections contain a minimum number of nodes with the minimum
//! value specified as a network-wide constant. Groups are of fixed size, defined as the above
//! minimum section size. Since nodes are assigned their name by the network, this provides
//! redundancy and resilience: a node has no control over which section or group authority it will
//! be a member of, and without a majority in the section or group it cannot forge a message from
//! there.
//!
//! The library also provides different types for the messages' data.
//!
//!
//! # Usage
//!
//! A decentralised service based on the `routing` library uses `Client` to send requests to the
//! network of nodes and receive responses.
//!
//! `Node` is used to handle and send requests within that network, and to implement its
//! functionality, e.g. storing and retrieving data, validating permissions, managing metadata, etc.
//!
//!
//! ## Client creation
//!
//! A client's name is a hash of its public keys. Upon creation, the client will attempt to connect
//! to the network through any node, and exchange public keys with it. That node becomes a
//! bootstrap node for the client, and messages to and from the client will be routed over it.
//!
//! ```no_run
//! # #![allow(unused)]
//! use std::sync::mpsc;
//! use routing::{Client, Event, SecretId};
//!
//! let (sender, receiver) = mpsc::channel::<Event>();
//! let full_id = SecretId::new(); // Generate new keys.
//! # #[cfg(not(feature = "use-mock-crust"))]
//! let client = Client::new(sender, Some(full_id), None).unwrap();
//! ```
//!
//! Messages can be sent using the methods of `client`, and received as `Event`s from the
//! `receiver`.
//!
//!
//! ## Node creation
//!
//! Creating a node looks even simpler:
//!
//! ```no_run
//! # #![allow(unused)]
//! use routing::Node;
//!
//! let node = Node::builder().create().unwrap();
//! ```
//!
//! Upon creation, the node will first connect to the network as a client. Once it has client
//! status, it requests a new name from the network, and then integrates itself in the network with
//! that new name, adding close nodes to its routing table.
//!
//! Messages can be sent using the methods of `node`, and received as `Event`s from the `receiver`.
//! The node can act as an individual node or as part of a section or group authority. Sending a
//! message as a section or group authority only has an effect if sufficiently many other nodes in
//! that authority send the same message.
//!
//!
//! # Sequence diagrams
//!
//! - [Bootstrapping](bootstrap.png)
//! - [Churn (`NewNode`)](new-node.png)
//! - [Tunnel](tunnel.png)

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maidsafe/QA/master/Images/maidsafe_logo.png",
    html_favicon_url = "https://maidsafe.net/img/favicon.ico",
    html_root_url = "https://docs.rs/routing"
)]
// For explanation of lint checks, run `rustc -W help` or see
// https://github.com/maidsafe/QA/blob/master/Documentation/Rust%20Lint%20Checks.md
#![forbid(
    exceeding_bitshifts, mutable_transmutes, no_mangle_const_items, unknown_crate_types, warnings
)]
#![deny(
    bad_style, deprecated, improper_ctypes, missing_docs, non_shorthand_field_patterns,
    overflowing_literals, plugin_as_library, private_no_mangle_fns, private_no_mangle_statics,
    stable_features, unconditional_recursion, unknown_lints, unsafe_code, unused, unused_allocation,
    unused_attributes, unused_comparisons, unused_features, unused_parens, while_true
)]
#![warn(
    trivial_casts, trivial_numeric_casts, unused_extern_crates, unused_import_braces,
    unused_qualifications, unused_results
)]
// TODO: Remove `renamed_and_removed_lints` once
// https://github.com/rust-lang-nursery/error-chain/pull/246 has been fixed.
#![allow(
    box_pointers, missing_copy_implementations, missing_debug_implementations,
    variant_size_differences, non_camel_case_types, renamed_and_removed_lints
)]
#![cfg_attr(
    feature = "cargo-clippy", deny(unicode_not_nfc, wrong_pub_self_convention, option_unwrap_used)
)]
// FIXME: allow `needless_pass_by_value` until it's OK to change the public API
#![cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]

extern crate config_file_handler;
extern crate hex;
#[macro_use]
extern crate log;
#[cfg(feature = "use-mock-crust")]
extern crate fake_clock;
extern crate maidsafe_utilities;
#[macro_use]
extern crate quick_error;
#[macro_use]
extern crate unwrap;
#[cfg(not(feature = "use-mock-crust"))]
extern crate crust;
extern crate itertools;
extern crate lru_time_cache;
extern crate num_bigint;
extern crate rand;
extern crate resource_proof;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate safe_crypto;
#[cfg(test)]
extern crate serde_json;
extern crate tiny_keccak;

// Needs to be before all other modules to make the macros available to them.
#[macro_use]
mod macros;

mod ack_manager;
mod action;
mod cache;
mod client;
mod client_error;
mod common_types;
mod config_handler;
mod cumulative_own_section_merge;
mod data;
mod error;
mod event;
mod event_stream;
mod message_filter;
mod messages;
mod node;
mod outbox;
mod peer_manager;
mod rate_limiter;
mod resource_prover;
mod routing_message_filter;
mod routing_table;
mod section_list_cache;
mod signature_accumulator;
mod state_machine;
mod states;
mod stats;
mod timer;
mod tunnels;
mod types;
mod utils;
mod xor_name;

#[cfg(feature = "use-mock-crypto")]
pub mod mock_crypto;

/// Reexports `crust::ConfigFile`
pub type BootstrapConfig = crust::ConfigFile;

/// Mock crust
#[cfg(feature = "use-mock-crust")]
pub mod mock_crust;

/// SHA-3 type alias.
pub mod sha3;

/// Messaging infrastructure
pub mod messaging;
/// Structured Data Tag for Session Packet Type
pub const TYPE_TAG_SESSION_PACKET: u64 = 0;
/// Structured Data Tag for DNS Packet Type
pub const TYPE_TAG_DNS_PACKET: u64 = 5;

/// Quorum is defined as having strictly greater than `QUORUM_NUMERATOR / QUORUM_DENOMINATOR`
/// agreement; using only integer arithmetic a quorum can be checked with
/// `votes * QUORUM_DENOMINATOR > voters * QUORUM_NUMERATOR`.
pub const QUORUM_NUMERATOR: usize = 1;
/// See `QUORUM_NUMERATOR`.
pub const QUORUM_DENOMINATOR: usize = 2;

/// Default minimal section size.
pub const MIN_SECTION_SIZE: usize = 8;
/// Key of an account data in the account packet
pub const ACC_LOGIN_ENTRY_KEY: &[u8] = b"Login";

pub use cache::{Cache, NullCache};
pub use client::Client;
pub use client_error::{ClientError, EntryError};
pub use common_types::AccountPacket;
pub use config_handler::{Config, DevConfig};
pub use data::{
    Action, EntryAction, EntryActions, ImmutableData, MutableData, PermissionSet, User, Value,
    MAX_IMMUTABLE_DATA_SIZE_IN_BYTES, MAX_MUTABLE_DATA_ENTRIES, MAX_MUTABLE_DATA_SIZE_IN_BYTES,
};
pub use error::{InterfaceError, RoutingError};
pub use event::Event;
pub use event_stream::EventStream;
pub use messages::{AccountInfo, Request, Response};
#[cfg(feature = "use-mock-crust")]
pub use mock_crust::crust;
pub use node::{Node, NodeBuilder};
#[cfg(feature = "use-mock-crust")]
pub use peer_manager::test_consts;
#[cfg(feature = "use-mock-crust")]
pub use rate_limiter::rate_limiter_consts;
#[cfg(any(test, feature = "use-mock-crust"))]
pub use routing_table::verify_network_invariant;
pub use routing_table::Error as RoutingTableError;
pub use routing_table::{Authority, Prefix, RoutingTable, Xorable};
pub use safe_crypto::{PublicId, SecretId};
pub use types::MessageId;
pub use xor_name::{PublicIdExt, XorName, XorNameFromHexError, XOR_NAME_BITS, XOR_NAME_LEN};

type Service = crust::compat::Service;
use crust::compat::Event as CrustEvent;
type CrustEventSender = crust::compat::CrustEventSender;
type PubConnectionInfo = crust::PubConnectionInfo;

#[cfg(test)]
mod tests {
    use super::{QUORUM_DENOMINATOR, QUORUM_NUMERATOR};

    #[test]
    #[cfg_attr(feature = "cargo-clippy", allow(eq_op))]
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
