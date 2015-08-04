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

use sodiumoxide::crypto::sign::Signature;
use sodiumoxide::crypto::sign;
use crust::Endpoint;
use authority::Authority;
use data::{Data, DataRequest};
use types;
use public_id::PublicId;
use types::{DestinationAddress, SourceAddress};
use error::{ResponseError};
use NameType;
use utils;
use cbor::{CborError};
use std::collections::BTreeMap;

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

#[derive(PartialEq, Eq, Clone, PartialOrd, Ord, Debug, RustcEncodable, RustcDecodable)]
pub struct GetDataResponse {
    pub data           : Data,
    pub orig_request   : SignedMessage,
    // If this is a group response, we carry the
    // (name, pub_key) pairs with it for sentinel.
    // In a similar fassion as GetGroupKeyResponse
    // message does.
    pub group_pub_keys : BTreeMap<NameType, sign::PublicKey>,
}

impl GetDataResponse {
    pub fn verify_request_came_from(&self, requester_pub_key: &sign::PublicKey) -> bool {
        self.orig_request.verify_signature(requester_pub_key)
    }
}

/// Response error which can be verified that originated from our request.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug, RustcEncodable, RustcDecodable)]
pub struct ErrorReturn {
    pub error: ResponseError,
    pub orig_request: SignedMessage
}

impl ErrorReturn {
    #[allow(dead_code)]
    pub fn new(error: ResponseError, orig_request: SignedMessage) -> ErrorReturn {
        ErrorReturn {
            error        : error,
            orig_request : orig_request,
        }
    }

    pub fn verify_request_came_from(&self, requester_pub_key: &sign::PublicKey) -> bool {
        self.orig_request.verify_signature(requester_pub_key)
    }
}

/// These are the messageTypes routing provides
/// many are internal to routing and woudl not be useful
/// to users.
#[derive(PartialEq, Eq, Clone, Debug, RustcEncodable, RustcDecodable)]
pub enum MessageType {
    ConnectRequest(ConnectRequest),
    ConnectResponse(ConnectResponse),
    FindGroup,
    FindGroupResponse(Vec<PublicId>),
    GetData(DataRequest),
    GetDataResponse(GetDataResponse),
    DeleteData(DataRequest),
    DeleteDataResponse(ErrorReturn),
    GetGroupKey,
    GetGroupKeyResponse(BTreeMap<NameType, sign::PublicKey>),
    Post(Data),
    PostResponse(ErrorReturn, BTreeMap<NameType, sign::PublicKey>),
    PutData(Data),
    PutDataResponse(ErrorReturn, BTreeMap<NameType, sign::PublicKey>),
    PutKey,
    PutPublicId(PublicId),
    PutPublicIdResponse(PublicId, SignedMessage),
    Refresh(u64, Vec<u8>),
    Unknown,
}

/// the bare (unsigned) routing message
#[derive(PartialEq, Eq, Clone, Debug, RustcEncodable, RustcDecodable)]
pub struct RoutingMessage {
    pub from_authority : Authority,
    pub to_authority   : Authority,
    pub message_type   : MessageType,
    pub message_id     : types::MessageId,
}

impl RoutingMessage {

    #[allow(dead_code)]
    pub fn message_id(&self) -> types::MessageId {
        self.message_id.clone()
    }

    #[allow(dead_code)]
    pub fn source(&self) -> Authority {
        self.from_authority.clone()
    }

    pub fn destination(&self) -> Authority {
        self.to_authority.clone()
    }

    //pub fn non_relayed_source(&self) -> NameType {
    //    self.source.non_relayed_source()
    //}

    //#[allow(dead_code)]
    //pub fn actual_source(&self) -> types::Address {
    //    self.source.actual_source()
    //}

    //pub fn non_relayed_destination(&self) -> NameType {
    //    self.destination.non_relayed_destination()
    //}

    //// FIXME: add from_authority to filter value
    //pub fn get_filter(&self) -> types::FilterType {
    //    (self.source.clone(), self.message_id, self.destination.clone())
    //}

    //pub fn from_authority(&self) -> Authority {
    //    self.authority.clone()
    //}

    pub fn client_key(&self) -> Option<sign::PublicKey> {
        match self.from_authority {
            Authority::ClientManager(_) => None,
            Authority::NaeManager(_)    => None,
            Authority::NodeManager(_)   => None,
            Authority::ManagedNode(_)   => None,
            Authority::Client(_, key)   => Some(key),
        }
    }

    pub fn client_key_as_name(&self) -> Option<NameType> {
        self.client_key().map(|n|utils::public_key_to_client_name(&n))
    }

    pub fn from_group(&self) -> Option<NameType /* Group name */> {
        match self.from_authority {
            Authority::ClientManager(name) => Some(name),
            Authority::NaeManager(name)    => Some(name),
            Authority::NodeManager(name)   => Some(name),
            Authority::ManagedNode(_)      => None,
            Authority::Client(_, _)        => None,
        }
    }

    ///// This creates a new message for Action::Forward. It clones all the fields,
    ///// and then mutates the destination and source accordingly.
    ///// Authority is changed at this point as this method is called after
    ///// the interface has processed the message.
    ///// Note: this is not for XOR-forwarding; then the header is preserved!
    //#[allow(dead_code)]
    //pub fn create_forward(&self,
    //                      our_name      : NameType,
    //                      our_authority : Authority,
    //                      destination   : NameType,
    //                      orig_signed_message  : SignedMessage) -> RoutingMessage {

    //    // implicitly preserve all non-mutated fields.
    //    let mut forward_message = self.clone();
    //    // if we are sending on and the original message is not stored
    //    // then store it and preserve along the route
    //    // it will contain the address to reply to as well as proof the request was made
    //    // FIXME(dirvine) We need the original encoded signed message here  :13/07/2015
    //    // FIXME(ben) only attach when from client or node 15/07/2015
    //    if self.orig_message.is_none() {
    //        forward_message.orig_message = Some(orig_signed_message);
    //    }

    //    forward_message.source      = SourceAddress::Direct(our_name);
    //    forward_message.destination = DestinationAddress::Direct(destination);
    //    forward_message.authority   = our_authority;
    //    forward_message
    //}

    ///// This creates a new message for Action::Reply. It clones all the fields,
    ///// and then mutates the destination and source accordingly.
    ///// Authority is changed at this point as this method is called after
    ///// the interface has processed the message.
    ///// Note: this is not for XOR-forwarding; then the header is preserved!
    //pub fn create_reply(&self, our_name : &NameType, our_authority : &Authority)
    //    -> Result<RoutingMessage, CborError> {
    //    // Commented the below code as it doesn't compile.
    //    let mut reply_message = self.clone();

    //    // Check if the message was forwarded, if so, reply directly to the
    //    // original poster (not the one who forwarded the message).
    //    reply_message.destination = match self.orig_message {
    //        Some(ref orig_message) => {
    //            try!(orig_message.get_routing_message()).reply_destination()
    //        },
    //        None => {
    //            self.reply_destination()
    //        }
    //    };

    //    reply_message.orig_message = None;
    //    reply_message.source       = SourceAddress::Direct(our_name.clone());
    //    reply_message.authority    = our_authority.clone();

    //    Ok(reply_message)
    //}

    //pub fn reply_destination(&self) -> DestinationAddress {
    //    match self.source {
    //        SourceAddress::RelayedForClient(a, b) => DestinationAddress::RelayToClient(a, b),
    //        SourceAddress::RelayedForNode(a, b)   => DestinationAddress::RelayToNode(a, b),
    //        SourceAddress::Direct(a)              => DestinationAddress::Direct(a),
    //    }
    //}

}

/// All messages sent / received are constructed as signed message.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug, RustcEncodable, RustcDecodable)]
pub struct SignedMessage {
    encoded_body : Vec<u8>,
    claimant     : types::Address,
    //             when signed by Client(sign::PublicKey) the data needs to contain it as an owner
    //             when signed by a Node(NameType), Sentinel needs to validate the signature
    signature    : Signature,
}

impl SignedMessage {
    pub fn new(claimant: types::Address, message: &RoutingMessage, private_sign_key: &sign::SecretKey)
        -> Result<SignedMessage, CborError> {

        let encoded_body = try!(utils::encode(&message));
        let signature    = sign::sign_detached(&encoded_body, private_sign_key);

        Ok(SignedMessage {
            encoded_body : encoded_body,
            claimant     : claimant,
            signature    : signature
        })
    }

    pub fn with_signature(claimant: types::Address, message: &RoutingMessage, signature: Signature)
        -> Result<SignedMessage, CborError> {

          let encoded_body = try!(utils::encode(&message));

          Ok(SignedMessage {
              encoded_body : encoded_body,
              claimant     : claimant,
              signature    : signature
          })
    }

    pub fn verify_signature(&self, public_sign_key: &sign::PublicKey) -> bool {
        sign::verify_detached(&self.signature,
                              &self.encoded_body,
                              &public_sign_key)
    }

    pub fn get_routing_message(&self) -> Result<RoutingMessage, CborError> {
        utils::decode::<RoutingMessage>(&self.encoded_body)
    }

    #[allow(dead_code)]
    pub fn encoded_body(&self) -> &Vec<u8> {
        &self.encoded_body
    }

    pub fn signature(&self) -> &Signature { &self.signature }
}
