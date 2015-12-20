// Copyright 2015 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.0.  This, along with the
// Licenses can be found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

//! The API for routing nodes (this is where you give the network its rules)
//!
//! The network will report **from authority your authority** and validate cryptographically any
//! message via group consensus or direct (clients). This means any facade you implement
//! will set out what you deem to be a valid operation.

//! Routing will provide
//!
//! 1.  Valid message sender
//!
//! 2.  Confirmed from authority
//!
//! 3.  Confirmed your authhority
//!
//! 4.  Exaclty 1 copy of each message
//!
//! This should allow relatively easy set up many decentralised services. Setting rules for
//! data types and what can be done wiht such types at differnt personas will allow a fairly complex
//! network to be configured wiht relative ease.
//!
//! The data types are encoded with Concise Binary Object Representation (CBOR).
//!

#![doc(html_logo_url =
           "https://raw.githubusercontent.com/maidsafe/QA/master/Images/maidsafe_logo.png",
       html_favicon_url = "http://maidsafe.net/img/favicon.ico",
       html_root_url = "http://maidsafe.github.io/routing")]

// For explanation of lint checks, run `rustc -W help` or see
// https://github.com/maidsafe/QA/blob/master/Documentation/Rust%20Lint%20Checks.md
#![forbid(bad_style, exceeding_bitshifts, mutable_transmutes, no_mangle_const_items,
          unknown_crate_types, warnings)]
#![deny(deprecated, drop_with_repr_extern, improper_ctypes, missing_docs,
        non_shorthand_field_patterns, overflowing_literals, plugin_as_library,
        private_no_mangle_fns, private_no_mangle_statics, stable_features, unconditional_recursion,
        unknown_lints, unsafe_code, unused, unused_allocation, unused_attributes,
        unused_comparisons, unused_features, unused_parens, while_true)]
#![warn(trivial_casts, trivial_numeric_casts, unused_extern_crates, unused_import_braces,
        unused_qualifications, unused_results)]
#![allow(box_pointers, fat_ptr_transmutes, missing_copy_implementations,
         missing_debug_implementations, variant_size_differences)]

#[macro_use]
extern crate log;
extern crate cbor;
extern crate rand;
extern crate rustc_serialize;
extern crate sodiumoxide;
extern crate time;
extern crate itertools;
extern crate ip;
extern crate accumulator;
extern crate xor_name;
extern crate crust;
extern crate lru_time_cache;
#[macro_use]
extern crate maidsafe_utilities;
extern crate message_filter;
extern crate kademlia_routing_table;

mod id;
mod utils;
mod event;
mod error;
mod action;
mod routing;
mod messages;
mod authority;
mod acceptors;
mod routing_node;
mod immutable_data;
mod routing_client;
mod structured_data;
mod connection_management;

/// TODO Remove this from public visibility
pub mod test_utils;
/// Types and functions used throughout the library.
pub mod types;
/// Data types.
pub mod data;
/// PlainData
pub mod plain_data;

pub use event::Event;
pub use routing::Routing;
pub use authority::Authority;
pub use plain_data::PlainData;
pub use id::{FullId, PublicId};
pub use data::{Data, DataRequest};
pub use routing_client::RoutingClient;
pub use immutable_data::{ImmutableData, ImmutableDataType};
pub use error::{RoutingError, InterfaceError};
pub use structured_data::{StructuredData, MAX_STRUCTURED_DATA_SIZE_IN_BYTES};
pub use messages::{SignedMessage, RoutingMessage, RequestMessage, ResponseMessage, RequestContent,
                   ResponseContent};
