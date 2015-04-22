// Copyright 2015 MaidSafe.net limited
//
// This MaidSafe Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the MaidSafe Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.0, found in the root
// directory of this project at LICENSE, COPYING and CONTRIBUTOR respectively and also
// available at: http://www.maidsafe.net/licenses
//
// Unless required by applicable law or agreed to in writing, the MaidSafe Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS
// OF ANY KIND, either express or implied.
//
// See the Licences for the specific language governing permissions and limitations relating to
// use of the MaidSafe Software.

//! The main API for routing nodes (this is where you give the network it's rules)
//!
//! The network will report **From Authority your Authority** and validate cryptographically and
//! via group consensus any message. This means any facade you implement will set out what you deem
//! to be a valid operation, routing will provide a valid message sender and authority that will
//! allow you to set up many decentralised services
//!
//! See maidsafe.net to see what they are doing as an example
//!
//! The data types are encoded with Concise Binary Object Representation (CBOR).
//!
//! This allows us to demand certain tags are available to routing that allows it to confirm things
//! like data.name() when calculating authority.
//!
//! We use Iana tag representations http://www.iana.org/assignments/cbor-tags/cbor-tags.xhtml
//!
//! Please define your own for this library. These tags are non optional and your data MUST meet
//! the requirements and implement the following tags:
//!
//! ```text
//! tag: 5483_0 -> name [u8; 64] type
//! tag: 5483_1 -> XXXXXXXXXXXXXX
//! ```

#![feature(collections)]
#![doc(html_logo_url = "http://maidsafe.net/img/Resources/branding/maidsafe_logo.fab2.png",
       html_favicon_url = "http://maidsafe.net/img/favicon.ico",
              html_root_url = "http://dirvine.github.io/routing")]
// #![warn(missing_docs)]
#![allow(dead_code, unused_variables, unused_features, unused_attributes)]
#![feature(custom_derive, rand, collection, std_misc, unsafe_destructor, unboxed_closures, io, core,
           thread_sleep, ip_addr, convert, scoped)]
extern crate sodiumoxide;
extern crate lru_time_cache;
extern crate message_filter;
extern crate rustc_serialize;
extern crate cbor;
extern crate rand;
extern crate time;
extern crate crust;

use sodiumoxide::crypto;

pub mod routing_client;
pub mod types;
mod message_header;
mod routing_table;
mod accumulator;
mod common_bits;
mod sentinel;
mod messages;
mod name_type;
pub mod message_interface;
pub mod routing_node;
pub mod interface;
/// NameType is a 512bit name to address elements on the DHT network.
pub use name_type::{NameType};
pub mod test_utils;

//#[derive(RustcEncodable, RustcDecodable)]
struct SignedKey {
  sign_public_key: crypto::sign::PublicKey,
  encrypt_public_key: crypto::asymmetricbox::PublicKey,
  signature: crypto::sign::Signature, // detached signature
}

pub enum Action {
  Reply(Vec<u8>),
  SendOn(Vec<NameType>),
}

pub enum RoutingError {
  Success,  // vault will also return a Success to indicate a dead end
  NoData,
  InvalidRequest,
  IncorrectData(Vec<u8>),
}

#[test]
fn facade_implementation() {

  mod routing_node;
  use interface::Interface;
  use types::{DestinationAddress, Authority};
  use NameType;
  use test_utils::Random;

  struct MyFacade;

  impl Interface for MyFacade {
    fn handle_get(&mut self, type_id: u64, our_authority: Authority, from_authority: Authority,from_address: NameType , data: Vec<u8>)->Result<Action, RoutingError> { unimplemented!(); }
    fn handle_put(&mut self, our_authority: Authority, from_authority: Authority,
                  from_address: NameType, dest_address: DestinationAddress, data: Vec<u8>)->Result<Action, RoutingError> { unimplemented!(); }
    fn handle_post(&mut self, our_authority: Authority, from_authority: Authority, from_address: NameType, data: Vec<u8>)->Result<Action, RoutingError> { unimplemented!(); }
    fn handle_get_response(&mut self, from_address: NameType , response: Result<Vec<u8>, RoutingError>) { unimplemented!() }
    fn handle_put_response(&mut self, from_authority: Authority,from_address: NameType , response: Result<Vec<u8>, RoutingError>) { unimplemented!(); }
    fn handle_post_response(&mut self, from_authority: Authority,from_address: NameType , response: Result<Vec<u8>, RoutingError>) { unimplemented!(); }
    fn add_node(&mut self, node: NameType) { unimplemented!(); }
    fn drop_node(&mut self, node: NameType) { unimplemented!(); }
  }

  let my_facade = MyFacade;

  let my_routing = routing_node::RoutingNode::new(Random::generate_random(), my_facade);
  /* assert_eq!(999, my_routing.get_facade().handle_get_response());  */
}
