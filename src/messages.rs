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

use rustc_serialize::{Decodable, Decoder, Encodable, Encoder};
use sodiumoxide::crypto;
use sodiumoxide::crypto::sign::Signature;
use crust::Endpoint;
use authority::Authority;
use data::{Data, DataRequest};
use types;
use id::Id;
use public_id::PublicId;
use types::{DestinationAddress, SourceAddress, FromAddress, ToAddress, NodeAddress};
use error::{RoutingError, ResponseError};
use NameType;
use cbor;
use utils;
use cbor::{CborError};

#[derive(PartialEq, Eq, Clone, Debug, RustcEncodable, RustcDecodable)]
pub struct ConnectRequest {
    pub local_endpoints: Vec<Endpoint>,
    pub external_endpoints: Vec<Endpoint>,
    // TODO: redundant, already in fob
    pub requester_id: NameType,
    // TODO: make optional, for now simply ignore if requester_fob is not relocated
    pub receiver_id: NameType,
    pub requester_fob: PublicId
}

#[derive(PartialEq, Eq, Clone, Debug, RustcEncodable, RustcDecodable)]
pub struct ConnectResponse {
    pub requester_local_endpoints: Vec<Endpoint>,
    pub requester_external_endpoints: Vec<Endpoint>,
    pub receiver_local_endpoints: Vec<Endpoint>,
    pub receiver_external_endpoints: Vec<Endpoint>,
    pub requester_id: NameType,
    pub receiver_id: NameType,
    pub receiver_fob: PublicId,
    pub serialised_connect_request: Vec<u8>,
    pub connect_request_signature: Signature
}

/// Respond wiht Data or Error 
#[derive(PartialEq, Eq, Clone, Debug, RustcEncodable, RustcDecodable)]
pub enum DataOrError {
Data(Data),
Err(ResponseError),    
} 

/// These are the messageTypes routing provides
/// many are internal to routing and woudl not be useful
/// to users.
#[derive(PartialEq, Eq, Clone, Debug, RustcEncodable, RustcDecodable)]
pub enum MessageType {
    BootstrapIdRequest,
    BootstrapIdResponse,
    ConnectRequest(ConnectRequest),
    ConnectResponse(ConnectResponse),
    FindGroup(NameType),
    FindGroupResponse(Vec<PublicId>),
    GetData(DataRequest),
    GetDataResponse(DataOrError),
    DeleteData(DataRequest),
    DeleteDataResponse(DataOrError),
    GetKey,
    GetKeyResponse(NameType, crypto::sign::PublicKey),
    GetGroupKey,
    GetGroupKeyResponse(Vec<(NameType, crypto::sign::PublicKey)>),
    Post(Data),
    PostResponse(DataOrError),
    PutData(Data),
    PutDataResponse(DataOrError),
    PutKey,
    AccountTransfer(NameType),
    PutPublicId(PublicId),
    PutPublicIdResponse(PublicId),
    Refresh(u64, Vec<u8>),
    Unknown,
}

/// the bare (unsigned) routing message
#[derive(PartialEq, Eq, Clone, Debug, RustcEncodable, RustcDecodable)]
pub struct RoutingMessage {
    pub destination     : DestinationAddress,
    pub source          : SourceAddress,
    pub message_type    : MessageType,
    pub message_id      : types::MessageId,
    pub authority       : Authority
}

impl RoutingMessage {

    pub fn message_id(&self) -> types::MessageId {
        self.message_id
    }

    pub fn send_to(&self) -> types::DestinationAddress {
        types::DestinationAddress {
            dest: match self.source.reply_to.clone() {
                       Some(reply_to) => reply_to,
                       None => match self.source.from_group.clone() {
                           Some(group_name) => group_name,
                           None => self.source.from_node.clone()
                       }
            },
            relay_to: self.source.relayed_for.clone()
        }
    }

    pub fn non_relayed_source(&self) -> NameType {
        match self.source {
            SourceAddress::RelayedForClient(addr, _) => addr,
            SourceAddress::RelayedForNode(addr, _)   => addr,
            SourceAddress::Direct(addr)              => addr,
        }
    }
    
    pub fn actual_source(&self) -> NameType {
        match self.source {
            SourceAddress::RelayedForClient(_, addr) => addr,
            SourceAddress::RelayedForNode(_, addr)   => addr,
            SourceAddress::Direct(addr)              => addr,
        }
    }

    pub fn non_relayed_destination(&self) -> NameType {
        match self.destination {
            DestinationAddress::RelayToClient(to_address, _) => to_address,
            DestinationAddress::RelayToNode(to_address, _)   => to_address,
            DestinationAddress::Direct(to_address)           => to_address,
        }
    }

    // FIXME: add from_authority to filter value
    pub fn get_filter(&self) -> types::FilterType {
        (self.source.clone(), self.message_id, self.destination.clone())
    }

    pub fn from_authority(&self) -> Authority {
        self.authority.clone()
    }

    pub fn set_relay_name(&mut self, reply_to: &NameType, relay_for: &NameType) {
        self.source.reply_to = Some(reply_to.clone());
        self.source.relayed_for = Some(relay_for.clone());
    }

    pub fn client_key(&self) -> Option<crypto::sign::PublicKey> {
        match self.source {
            SourceAddress::RelayedForClient(_, client_key) => Some(client_key),
            SourceAddress::RelayedForNode(_, _)            => None,
            SourceAddress::Direct(_)                       => None,
        }
    }

    pub fn client_key_as_name(&self) -> Option<NameType> {
        self.client_key().map(utils::public_key_to_client_name)
    }

    pub fn from_group(&self) -> Option<NameType /* Group name */> {
        match self.source {
            SourceAddress::RelayedForClient(_, _) => None,
            SourceAddress::RelayedForNode(_, _)   => None,
            SourceAddress::Direct(_) => match self.authority {
                Authority::ClientManager(n) => Some(n),
                Authority::NaeManager(n)    => Some(n),
                Authority::OurCloseGroup(n) => Some(n),
                Authority::NodeManager(n)   => Some(n),
                Authority::ManagedNode      => None,
                Authority::ManagedClient(_) => None,
                Authority::Client(_)        => None,
                Authority::Unknown          => None,
            },
        }
    }

    /// This creates a new message for Action::SendOn. It clones all the fields,
    /// and then mutates the destination and source accordingly.
    /// Authority is changed at this point as this method is called after
    /// the interface has processed the message.
    /// Note: this is not for XOR-forwarding; then the header is preserved!
    pub fn create_send_on(&self, our_name : &NameType, our_authority : &Authority,
                          destination : &NameType) -> RoutingMessage {
        // implicitly preserve all non-mutated fields.
        let mut send_on_message = self.clone();

        send_on_message.source =  SourceAddress::Direct(our_name);
        send_on_message.destination = DestinationAddress::Direct(destination);
        send_on_message.authority = our_authority.clone();
        send_on_message
    }

    /// This creates a new message for Action::Reply. It clones all the fields,
    /// and then mutates the destination and source accordingly.
    /// Authority is changed at this point as this method is called after
    /// the interface has processed the message.
    /// Note: this is not for XOR-forwarding; then the header is preserved!
    pub fn create_reply(&self, our_name : &NameType, our_authority : &Authority)-> RoutingMessage {
        // implicitly preserve all non-mutated fields.
        // TODO(dirvine) Again why copy here instead of change in place?  :08/07/2015
        let mut reply_message = self.clone();
        reply_message.source  = self.reply_source(our_name);
        reply_message.destination = self.reply_destination(our_name);        
        reply_message.authority = our_authority.clone();
        reply_message
    }

    pub fn reply_source(&self, our_name : &NameType)->SourceAddress {
        match self.destination {
            DestinationAddress::RelayToClient(_, b) => SourceAddress::RelayedForClient(our_name.clone(), b),
            DestinationAddress::RelayToNode(_, b)   => SourceAddress::RelayedForNode(our_name.clone()),
            DestinationAddress::Direct(_)           => SourceAddress::Direct(our_name.clone()),
        }
    }

    pub fn reply_destination(&self, our_name : &NameType)->DestinationAddress {
        match self.source {
            SourceAddress::RelayedForClient(a, b) => DestinationAddress::RelayToClient(a, b),
            SourceAddress::RelayedForNode(a, b)   => DestinationAddress::RelayToNode(a, b),
            SourceAddress::Direct(a)              => DestinationAddress::Direct(a),
        }
    }
         
}

#[derive(PartialEq, Eq, Clone, Debug, RustcEncodable, RustcDecodable)]
pub struct SignedRoutingMessage {
    // FIXME: The field `encoded_routing_message` should be private to
    // avoid setting it with data that represent something other than
    // serialised RoutingMessage. (`signature` should probably be
    // private as well for similar reason).
    pub encoded_routing_message : Vec<u8>,
    pub signature               : Signature
}

impl SignedRoutingMessage {
    pub fn new(message: &RoutingMessage, secret_key: &crypto::sign::SecretKey)
        -> Result<SignedRoutingMessage, CborError>
    {
        let encoded_message = try!(utils::encode(&message));
        let signature = crypto::sign::sign_detached(&encoded_message,
                                                    secret_key);
        let message = SignedRoutingMessage {
            encoded_routing_message : encoded_message,
            signature               : signature,
        };
    }
}

/// All messages sent / received are constructed from this type
#[derive(PartialEq, Eq, Clone, Debug, RustcEncodable, RustcDecodable)]
pub enum Message {
    Signed(SignedRoutingMessage),
    Unsigned(RoutingMessage), // Only Get request is unsigned
}

impl Message {
    pub fn routing_message(&self) -> Result<RoutingMessage, CborError> {
        match self {
            Message::Signed(m)   => try!(utils::decode::<RoutingMessage>(m.encoded_routing_message)),
            Message::Unsigned(m) => Ok(m)
        }
    }
    pub fn check_signed_by(&self, signing_key: crypto::sign::PublicKey)->Result<(), RoutingError> {
        match self {
            Message::Signed(m) | Message::Error(m)  => if crypto::sign::verify_detached(&m.signature, 
                                                                          &m.encoded_routing_message,
                                                                          &signing_key) {
                                                            return ();
                                                       } else { 
                                                           return RoutingError::FailedSignature;
                                                       },
            _ => RoutingError::FailedSignature,
        }
        
    }
    
}
