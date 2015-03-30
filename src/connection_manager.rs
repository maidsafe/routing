// Copyright 2015 MaidSafe.net limited
// This MaidSafe Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
// By contributing code to the MaidSafe Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.0, found in the root
// directory of this project at LICENSE, COPYING and CONTRIBUTOR respectively and also
// available at: http://www.maidsafe.net/licenses
// Unless required by applicable law or agreed to in writing, the MaidSafe Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS
// OF ANY KIND, either express or implied.
// See the Licences for the specific language governing permissions and limitations relating to
// use of the MaidSafe
// Software.                                                                 


use std::net::{SocketAddr};
// use std::io::{stdout, stderr, Write, BufReader};
// use std::io::Result as IoResult;
use std::io::Error as IoError;
// use std::io::{ ErrorKind };
// use cbor::{ Encoder, CborError, Decoder }; 
// use std::thread::spawn;
// use std::marker::PhantomData;
// use rustc_serialize::{ Decodable, Encodable };
// use bchannel::channel;
use routing_table;
use bootstrap;
pub use bchannel::Receiver;

type Address = Vec<u8>;

// use net::ip::SocketAddr;

/// Will hold tcp udt sentinel routing_table beacon boostrap_file
struct Connections {
  routing_table: routing_table::RoutingTable,
  boostrap_list: bootstrap::BootStrapHandler,
}

impl Connections {
/// must be called prior to any other method 
/// this function will spawn a thread and listen for messages 
/// and either handle pass to handle_message() 
/// or send via channel to the receiver
/// USe ```match msg.decode(cbor_tag)``` to get message type
/// Any new endpoints are checked for NAT traversal and bootstrap file inclusion
pub fn start() -> Receiver<Vec<u8>, IoError> {
  unimplemented!();
  }

/// we will send to this address, by getting targets from routing table.
pub fn send(message: Vec<u8>, address : Address) {}


/// We add connections the routing table tells us are nodes
fn maintain_connection(socket: SocketAddr) {}
/// will send a message to another node with our interested node included in message
/// the other node will try and connect to the interested node and report back to 
/// us if it can connect. If so its a good bootstrap node
fn send_nat_traverse_message() {}

/// Acting as a rquest form another node, we will try and connect to the node they suggest
/// and send back a response IF we can connect, drop otherwise
fn handle_nat_traverse_message() {}

/// A node we have asked about is actually able to be connected to as though direct
fn handle_nat_traverse_response() {}




/// this is a routing message may be 
/// connect connect response get_key etc. as well as JOIN LEAVE 
/// Only nodes from connect response / connect will be added to 
/// routing table
fn handle_message() {}

}

