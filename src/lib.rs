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
//! The network will report **from authority your authority** and validate cryptographically any message via group consensus.
//! This means any facade you implement will set out what you deem
//! to be a valid operation. Routing will provide a valid message sender and authority that will
//! allow you to set up many decentralised services
//!
//! The data types are encoded with Concise Binary Object Representation (CBOR).
//!
//! We use Iana tag representations http://www.iana.org/assignments/cbor-tags/cbor-tags.xhtml
//!

#![doc(html_logo_url = "http://maidsafe.net/img/Resources/branding/maidsafe_logo.fab2.png",
       html_favicon_url = "http://maidsafe.net/img/favicon.ico",
       html_root_url = "http://maidsafe.github.io/routing")]
// #![warn(missing_docs)]
#![deny(dead_code, unused_variables, unused_features, unused_attributes)]
#![deny(unused_features, unused_attributes)]
#![allow(dead_code, unused_variables, unused_imports)]
#![forbid(bad_style, warnings)]

#[macro_use]
extern crate log;

extern crate cbor;
extern crate rand;
extern crate rustc_serialize;
extern crate sodiumoxide;
extern crate time;
extern crate itertools;

extern crate crust;
extern crate accumulator;
extern crate lru_time_cache;
extern crate message_filter;

mod common_bits;
mod action;
mod filter;
mod messages;
mod direct_messages;
mod name_type;
mod routing_table;
mod routing_node;
mod routing_core;
mod relay;
mod hello;
mod churn;
mod wake_up;
mod peer;
mod refresh_accumulator;
mod message_accumulator;

/// Routing provides an actionable interface to routing.
pub mod routing;
pub mod routing_client;
/// Event provides the events the user can expect to receive from routing
pub mod event;
pub mod test_utils;
pub mod types;
pub mod id;
pub mod utils;
pub mod public_id;
pub mod error;
pub mod authority;
pub mod structured_data;
pub mod immutable_data;
pub mod plain_data;
pub mod data;

/// NameType is a 512bit name to address elements on the DHT network.
pub use name_type::{NameType, closer_to_target};
pub use messages::{SignedToken, ExternalRequest, ExternalResponse};
