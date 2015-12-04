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

//! The main API for routing nodes (this is where you give the network its rules)
//!
//! The network will report **from authority your authority** and validate cryptographically any
//! message via group consensus. This means any facade you implement will set out what you deem to
//! be a valid operation.  Routing will provide a valid message sender and authority that will allow
//! you to set up many decentralised services.
//!
//! The data types are encoded with Concise Binary Object Representation (CBOR).
//!
//! We use Iana tag representations http://www.iana.org/assignments/cbor-tags/cbor-tags.xhtml

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
        unused_qualifications, unused_results, variant_size_differences)]
#![allow(box_pointers, fat_ptr_transmutes, missing_copy_implementations,
         missing_debug_implementations)]

// TODO TODO TODO TODO TODO TODO TODO TODO TODO TODO TODO TODO TODO TODO
#![allow(unused)]

#[macro_use]
extern crate log;
extern crate cbor;
extern crate rand;
extern crate rustc_serialize;
extern crate sodiumoxide;
extern crate time;

extern crate accumulator;
extern crate crust;
extern crate lru_time_cache;
#[macro_use]
extern crate maidsafe_utilities;
extern crate message_filter;

mod action;
mod messages;
mod direct_messages;
mod name_type;
mod routing_table;
mod routing_node;
mod refresh_accumulator;
mod connection_management;

/// Routing provides an actionable interface to routing.
pub mod routing;
/// Client interface to routing.
pub mod routing_client;
/// Event provides the events the user can expect to receive from routing.
pub mod event;
/// Utility structs and functions used during testing.
pub mod test_utils;
/// Types and functions used throught the library.
pub mod types;
/// Network identity component containing public and private IDs.
pub mod id;
/// Commonly required functions.
pub mod utils;
/// Errors reported for failed conditions/operations.
pub mod error;
// FIXME (ben 8/09/2015) make the module authority private
/// Persona types recognised by network.
pub mod authority;
/// StructuredData type.
pub mod structured_data;
/// ImmutableData type.
pub mod immutable_data;
/// PlainData type.
pub mod plain_data;
/// Data types used in messages.
pub mod data;

/// Data cache for all Data types
pub mod data_cache;
/// Data cache options, may be set at runtime
pub mod data_cache_options;
/// NameType is a 512bit name to address elements on the DHT network.
pub use name_type::{NameType, closer_to_target, NAME_TYPE_LEN};
/// Message types defined by the library.
pub use messages::{SignedRequest, ExternalRequest, ExternalResponse};
/// Persona types recognised by the network.
pub use authority::Authority;
pub use id::{FullId, PublicId};
