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

#[cfg(not(feature = "use-mock-crust"))]
use crust::PeerId;
#[cfg(feature = "use-mock-crust")]
use mock_crust::crust::PeerId;
use maidsafe_utilities::serialisation::serialise;
use rustc_serialize::{Decoder, Encoder};
use sodiumoxide::crypto::{box_, sign};
use sodiumoxide::crypto::hash::sha512;
use std::fmt::{self, Debug, Formatter};
use xor_name::XorName;

use authority::Authority;
use data::{Data, DataRequest};
use error::RoutingError;
use id::{FullId, PublicId};
use types::MessageId;
use utils;

/// Wrapper of all messages.
///
/// This is the only type allowed to be sent / received on the network.
#[derive(Debug, RustcEncodable, RustcDecodable)]
pub enum Message {
    /// A message sent between two nodes directly
    Direct(DirectMessage),
    /// A message sent across the network (in transit)
    Hop(HopMessage),
    /// A direct message sent via a tunnel because the nodes could not connect directly
    TunnelDirect {
        /// The wrapped message
        content: DirectMessage,
        /// The sender
        src: PeerId,
        /// The receiver
        dst: PeerId,
    },
    /// A hop message sent via a tunnel because the nodes could not connect directly
    TunnelHop {
        /// The wrapped message
        content: HopMessage,
        /// The sender
        src: PeerId,
        /// The receiver
        dst: PeerId,
    },
}

/// Messages sent via a direct connection.
///
/// Allows routing to directly send specific messages between nodes.
#[derive(RustcEncodable, RustcDecodable)]
pub enum DirectMessage {
    /// Sent from the bootstrap node to a client in response to `ClientIdentify`.
    BootstrapIdentify {
        /// The bootstrape node's keys and name.
        public_id: ::id::PublicId,
        /// The dynamically calculated quorum size the client's accumulator should use.
        current_quorum_size: usize,
    },
    /// Sent to the client to indicate that this node is not available as a bootstrap node.
    BootstrapDeny,
    /// Sent from a newly connected client to the bootstrap node to inform it about the client's
    /// public ID.
    ClientIdentify {
        /// Serialised keys and claimed name.
        serialised_public_id: Vec<u8>,
        /// Signature of the client.
        signature: sign::Signature,
        /// Indicate whether we intend to remain a client, as opposed to becoming a routing node.
        client_restriction: bool,
    },
    /// Sent from a node to a node, to allow the latter to add the former to its routing table.
    NodeIdentify {
        /// Keys and claimed name, serialised outside routing.
        serialised_public_id: Vec<u8>,
        /// Signature of the originator of this message.
        signature: sign::Signature,
    },
    /// Sent from a client that became a full routing node. The recipient can remove it from its
    /// client map.
    ClientToNode,
    /// Sent from a node that found a new node in the network to all its contacts who might need to
    /// add the new node to their routing table.
    NewNode(PublicId),
    /// Sent from a node that needs a tunnel to be able to connect to the given peer.
    TunnelRequest(PeerId),
    /// Sent as a response to `TunnelRequest` if the node cannot act as a tunnel.
    TunnelFail(PeerId),
    /// Sent as a response to `TunnelRequest` if the node can act as a tunnel.
    TunnelSuccess(PeerId),
    /// Sent from a tunnel node to indicate that the given peer has disconnected.
    TunnelClosed(PeerId),
    /// Sent to a tunnel node to indicate the tunnel is not needed anymore.
    TunnelDisconnect(PeerId),
}

/// And individual hop message that represents a part of the route of a message in transit.
///
/// To relay a `SignedMessage` via another node, the `SignedMessage` is wrapped in a `HopMessage`.
/// The `signature` is from the node that sends this directly to a node in its routing table. To
/// prevent Man-in-the-middle attacks, the `content` is signed by the original sender.
#[derive(RustcEncodable, RustcDecodable)]
pub struct HopMessage {
    /// Wrapped signed message.
    content: SignedMessage,
    /// Name of the previous node in the `content`'s route.
    name: XorName,
    /// Signature to be validated against `name`'s public key.
    signature: sign::Signature,
}

impl HopMessage {
    /// Wrap `content` for transmission to the next hop and sign it.
    pub fn new(content: SignedMessage,
               name: XorName,
               sign_key: &sign::SecretKey)
               -> Result<HopMessage, RoutingError> {
        let bytes_to_sign = try!(serialise(&(&content, &name)));
        Ok(HopMessage {
            content: content,
            name: name,
            signature: sign::sign_detached(&bytes_to_sign, sign_key),
        })
    }

    /// Validate that the message is signed by `verification_key` contained in message.
    ///
    /// This does not imply that the message came from a known node. That requires a check against
    /// the routing table to identify the name associated with the `verification_key`.
    pub fn verify(&self, verification_key: &sign::PublicKey) -> Result<(), RoutingError> {
        let signed_bytes = try!(serialise(&(&self.content, &self.name)));
        if sign::verify_detached(&self.signature, &signed_bytes, verification_key) {
            Ok(())
        } else {
            Err(RoutingError::FailedSignature)
        }
    }

    /// Returns the `SignedMessage` and the `name` of the previous routing node.
    ///
    /// Does not validate the message! [#verify] must be called to ensure that the sender is valid
    /// and signed the message.
    pub fn content(&self) -> &SignedMessage {
        &self.content
    }

    /// The name of the previous node in the signed message's route.
    pub fn name(&self) -> &XorName {
        &self.name
    }
}

/// Wrapper around a routing message, signed by the originator of the message.
#[derive(Ord, PartialOrd, Eq, PartialEq, Clone, Hash, RustcEncodable, RustcDecodable)]
pub struct SignedMessage {
    /// A request or response type message.
    content: RoutingMessage,
    /// Claimed public ID of a node or client.
    ///
    /// For clients this is easily verifiable since their name is computed from the ID. For nodes it
    /// needs to be confirmed by their `NodeManager`.
    public_id: PublicId,
    signature: sign::Signature,
}

impl SignedMessage {
    /// Creates a `SignedMessage` with the given `content` and signed by the given `full_id`.
    pub fn new(content: RoutingMessage, full_id: &FullId) -> Result<SignedMessage, RoutingError> {
        let bytes_to_sign = try!(serialise(&(&content, full_id.public_id())));
        Ok(SignedMessage {
            content: content,
            public_id: *full_id.public_id(),
            signature: sign::sign_detached(&bytes_to_sign, full_id.signing_private_key()),
        })
    }

    /// Confirms the signature against the claimed public ID.
    pub fn check_integrity(&self) -> Result<(), RoutingError> {
        let signed_bytes = try!(serialise(&(&self.content, &self.public_id)));
        if sign::verify_detached(&self.signature,
                                 &signed_bytes,
                                 self.public_id().signing_public_key()) {
            Ok(())
        } else {
            Err(RoutingError::FailedSignature)
        }
    }

    /// The routing message that was signed.
    pub fn content(&self) -> &RoutingMessage {
        &self.content
    }

    /// The `PublicId` associated with the signed message
    pub fn public_id(&self) -> &PublicId {
        &self.public_id
    }
}

/// Variant type to hold `either` a request or response.
#[derive(Ord, PartialOrd, Eq, PartialEq, Clone, Hash, Debug, RustcEncodable, RustcDecodable)]
pub enum RoutingMessage {
    /// Outgoing RPC type message.
    Request(RequestMessage),
    /// Incoming answer to request RPC.
    Response(ResponseMessage),
}

impl RoutingMessage {
    /// Returns the sender, i. e. the source authority of the routing message.
    pub fn src(&self) -> &Authority {
        match *self {
            RoutingMessage::Request(ref msg) => &msg.src,
            RoutingMessage::Response(ref msg) => &msg.src,
        }
    }

    /// Returns the recipient, i. e. the destination authority of routing message.
    pub fn dst(&self) -> &Authority {
        match *self {
            RoutingMessage::Request(ref msg) => &msg.dst,
            RoutingMessage::Response(ref msg) => &msg.dst,
        }
    }
}

/// A request message wrapper
#[derive(Ord, PartialOrd, Eq, PartialEq, Clone, Hash, Debug, RustcEncodable, RustcDecodable)]
pub struct RequestMessage {
    /// Source authority
    pub src: Authority,
    /// Destination authority
    pub dst: Authority,
    /// The request content
    pub content: RequestContent,
}

/// A response message wrapper
#[derive(Ord, PartialOrd, Eq, PartialEq, Clone, Hash, Debug, RustcEncodable, RustcDecodable)]
pub struct ResponseMessage {
    /// Source authority
    pub src: Authority,
    /// Destination authority
    pub dst: Authority,
    /// The response content
    pub content: ResponseContent,
}

/// The request types
#[derive(Ord, PartialOrd, Eq, PartialEq, Clone, Hash, RustcEncodable, RustcDecodable)]
pub enum RequestContent {
    // ---------- Internal ------------
    /// Ask the network to alter your `PublicId` name.
    ///
    /// This is sent by a `Client` to its `NaeManager` with the intent to become a routing node with
    /// a new name chosen by the `NaeManager`.
    GetNetworkName {
        /// The client's `PublicId` (public keys and name)
        current_id: PublicId,
    },
    /// Notify a joining node's `NodeManager` so that it expects a `GetCloseGroup` request from it.
    ExpectCloseNode {
        /// The joining node's `PublicId` (public keys and name)
        expect_id: PublicId,
    },
    /// Request the `PublicId`s of the recipient's close group.
    ///
    /// This is sent from a joining node to its `NodeManager` to request the `PublicId`s of the
    /// `NodeManager`'s members.
    GetCloseGroup,
    /// Request a direct connection to the recipient.
    Connect,
    /// Send our connection_info encrypted to a node we wish to connect to and have the keys for.
    ConnectionInfo {
        /// Encrypted Crust connection info.
        encrypted_connection_info: Vec<u8>,
        /// Nonce used to provide a salt in the encrytped message.
        nonce_bytes: [u8; box_::NONCEBYTES],
    },
    /// Ask each member of a group near a node address for the `PublicId`.
    GetPublicId,
    /// Ask for a `PublicId` but provide our connection_info encrytped.
    GetPublicIdWithConnectionInfo {
        /// Encrypted crust connection_info (socket address and protocol).
        encrypted_connection_info: Vec<u8>,
        /// Nonce used to provide a salt in the encrytped message.
        nonce_bytes: [u8; box_::NONCEBYTES],
    },
    /// Message from upper layers sending network state on any network churn event.
    Refresh(Vec<u8>),
    // ---------- External ------------
    /// Ask for data from network, passed from API with data name as parameter
    Get(DataRequest, MessageId),
    /// Put data to network. Provide actual data as parameter
    Put(Data, MessageId),
    /// Post data to network. Provide actual data as parameter
    Post(Data, MessageId),
    /// Delete data from network. Provide actual data as parameter
    Delete(Data, MessageId),
}

/// The response types
///
/// All responses map to a specific request, and where the request was from a single node
/// or client, the response will contain the signed request to prevent forgery.
#[derive(Ord, PartialOrd, Eq, PartialEq, Clone, Hash, RustcEncodable, RustcDecodable)]
pub enum ResponseContent {
    // ---------- Internal ------------
    /// Reply with the new `PublicId` for the joining node.
    ///
    /// Sent from the `NaeManager` to the `Client`.
    GetNetworkName {
        /// Supplied `PublicId`, but with the new name
        relocated_id: PublicId,
    },
    /// Reply with the requested `PublicId`.
    ///
    /// Sent from the `NodeManager` to the joining node.
    GetPublicId {
        /// The requested `PublicId`
        public_id: PublicId,
    },
    /// Reply with the `PublicId` along with the sender's encrypted connection_info
    ///
    /// Sent from a `ManagedNode` to another node or client.
    GetPublicIdWithConnectionInfo {
        /// Our `PublicId`
        public_id: PublicId,
        /// Their connection_info
        encrypted_connection_info: Vec<u8>,
        /// Message salt
        nonce_bytes: [u8; box_::NONCEBYTES],
    },
    /// Return the close `PublicId`s back to the requestor.
    ///
    /// Sent from a `NodeManager` to a node or client.
    GetCloseGroup {
        /// Our close group `PublicId`s.
        close_group_ids: Vec<PublicId>,
    },
    // ---------- External ------------
    /// Reply with the requested data (may not be ignored)
    ///
    /// Sent from a `ManagedNode` to an `NaeManager`, and from there to a `Client`, although this
    /// may be shortcut if the data is in a node's cache.
    GetSuccess(Data, MessageId),
    /// Success token for Put (may be ignored)
    PutSuccess(sha512::Digest, MessageId),
    /// Success token for Post  (may be ignored)
    PostSuccess(sha512::Digest, MessageId),
    /// Success token for delete  (may be ignored)
    DeleteSuccess(sha512::Digest, MessageId),
    /// Error for `Get`, includes signed request to prevent injection attacks
    GetFailure {
        /// Unique message identifier
        id: MessageId,
        /// Originator's signed request
        request: RequestMessage,
        /// Error type sent back, may be injected from upper layers
        external_error_indicator: Vec<u8>,
    },
    /// Error for Put, includes signed request to prevent injection attacks
    PutFailure {
        /// Unique message identifier
        id: MessageId,
        /// Originator's signed request
        request: RequestMessage,
        /// Error type sent back, may be injected from upper layers
        external_error_indicator: Vec<u8>,
    },
    /// Error for Post, includes signed request to prevent injection attacks
    PostFailure {
        /// Unique message identifier
        id: MessageId,
        /// Originator's signed request
        request: RequestMessage,
        /// Error type sent back, may be injected from upper layers
        external_error_indicator: Vec<u8>,
    },
    /// Error for delete, includes signed request to prevent injection attacks
    DeleteFailure {
        /// Unique message identifier
        id: MessageId,
        /// Originator's signed request
        request: RequestMessage,
        /// Error type sent back, may be injected from upper layers
        external_error_indicator: Vec<u8>,
    },
}

impl Debug for DirectMessage {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        match *self {
            DirectMessage::BootstrapIdentify { ref public_id, ref current_quorum_size } => {
                write!(formatter,
                       "BootstrapIdentify {{ {:?}, {:?} }}",
                       public_id,
                       current_quorum_size)
            }
            DirectMessage::BootstrapDeny => write!(formatter, "BootstrapDeny"),
            DirectMessage::ClientToNode => write!(formatter, "ClientToNode"),
            DirectMessage::ClientIdentify { client_restriction: true, .. } => {
                write!(formatter, "ClientIdentify (client only)")
            }
            DirectMessage::ClientIdentify { client_restriction: false, .. } => {
                write!(formatter, "ClientIdentify (joining node)")
            }
            DirectMessage::NodeIdentify { .. } => write!(formatter, "NodeIdentify {{ .. }}"),
            DirectMessage::NewNode(ref public_id) => write!(formatter, "NewNode({:?})", public_id),
            DirectMessage::TunnelRequest(peer_id) => {
                write!(formatter, "TunnelRequest({:?})", peer_id)
            }
            DirectMessage::TunnelFail(peer_id) => write!(formatter, "TunnelFail({:?})", peer_id),
            DirectMessage::TunnelSuccess(peer_id) => {
                write!(formatter, "TunnelSuccess({:?})", peer_id)
            }
            DirectMessage::TunnelClosed(peer_id) => {
                write!(formatter, "TunnelClosed({:?})", peer_id)
            }
            DirectMessage::TunnelDisconnect(peer_id) => {
                write!(formatter, "TunnelDisconnect({:?})", peer_id)
            }
        }
    }
}

impl Debug for HopMessage {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter,
               "HopMessage {{ content: {:?}, name: {:?}, signature: .. }}",
               self.content,
               self.name)
    }
}

impl Debug for SignedMessage {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter,
               "SignedMessage {{ content: {:?}, public_id: {:?}, signature: .. }}",
               self.content,
               self.public_id)
    }
}

impl Debug for RequestContent {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        match *self {
            RequestContent::GetNetworkName { ref current_id } => {
                write!(formatter, "GetNetworkName {{ {:?} }}", current_id)
            }
            RequestContent::ExpectCloseNode { ref expect_id } => {
                write!(formatter, "ExpectCloseNode {{ {:?} }}", expect_id)
            }
            RequestContent::GetCloseGroup => write!(formatter, "GetCloseGroup"),
            RequestContent::Connect => write!(formatter, "Connect"),
            RequestContent::ConnectionInfo { .. } => write!(formatter, "ConnectionInfo {{ .. }}"),
            RequestContent::GetPublicId => write!(formatter, "GetPublicId"),
            RequestContent::GetPublicIdWithConnectionInfo { .. } => {
                write!(formatter, "GetPublicIdWithConnectionInfo {{ .. }}")
            }
            RequestContent::Refresh(ref data) => {
                write!(formatter, "Refresh({})", utils::format_binary_array(data))
            }
            RequestContent::Get(ref data_request, ref message_id) => {
                write!(formatter, "Get({:?}, {:?})", data_request, message_id)
            }
            RequestContent::Put(ref data, ref message_id) => {
                write!(formatter, "Put({:?}, {:?})", data, message_id)
            }
            RequestContent::Post(ref data, ref message_id) => {
                write!(formatter, "Post({:?}, {:?})", data, message_id)
            }
            RequestContent::Delete(ref data, ref message_id) => {
                write!(formatter, "Delete({:?}, {:?})", data, message_id)
            }
        }
    }
}

impl Debug for ResponseContent {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        match *self {
            ResponseContent::GetNetworkName { ref relocated_id } => {
                write!(formatter, "GetNetworkName {{ {:?} }}", relocated_id)
            }
            ResponseContent::GetPublicId { ref public_id } => {
                write!(formatter, "GetPublicId {{ {:?} }}", public_id)
            }
            ResponseContent::GetPublicIdWithConnectionInfo { ref public_id, .. } => {
                write!(formatter,
                       "GetPublicIdWithConnectionInfo {{ {:?}, .. }}",
                       public_id)
            }
            ResponseContent::GetCloseGroup { ref close_group_ids } => {
                write!(formatter, "GetCloseGroup {{ {:?} }}", close_group_ids)
            }
            ResponseContent::GetSuccess(ref data, ref message_id) => {
                write!(formatter, "GetSuccess {{ {:?}, {:?} }}", data, message_id)
            }
            ResponseContent::PutSuccess(ref digest, ref message_id) => {
                write!(formatter,
                       "PutSuccess {{ Digest({}), {:?} }}",
                       utils::format_binary_array(digest),
                       message_id)
            }
            ResponseContent::PostSuccess(ref digest, ref message_id) => {
                write!(formatter,
                       "PostSuccess {{ Digest({}), {:?} }}",
                       utils::format_binary_array(digest),
                       message_id)
            }
            ResponseContent::DeleteSuccess(ref digest, ref message_id) => {
                write!(formatter,
                       "DeleteSuccess {{ Digest({}), {:?} }}",
                       utils::format_binary_array(digest),
                       message_id)
            }
            ResponseContent::GetFailure { ref id, ref request, .. } => {
                write!(formatter, "GetFailure {{ {:?}, {:?}, .. }}", id, request)
            }
            ResponseContent::PutFailure { ref id, ref request, .. } => {
                write!(formatter, "PutFailure {{ {:?}, {:?}, .. }}", id, request)
            }
            ResponseContent::PostFailure { ref id, ref request, .. } => {
                write!(formatter, "PostFailure {{ {:?}, {:?}, .. }}", id, request)
            }
            ResponseContent::DeleteFailure { ref id, ref request, .. } => {
                write!(formatter, "DeleteFailure {{ {:?}, {:?}, .. }}", id, request)
            }
        }
    }
}


#[cfg(test)]
mod test {
    extern crate rand;

    use super::{HopMessage, SignedMessage, RoutingMessage, RequestMessage, RequestContent};
    use id::FullId;
    use authority::Authority;
    use xor_name::XorName;
    use sodiumoxide::crypto::sign;
    use maidsafe_utilities::serialisation::serialise;

    #[test]
    fn signed_message_check_integrity() {
        let name: XorName = rand::random();
        let routing_message = RoutingMessage::Request(RequestMessage {
            src: Authority::ClientManager(name),
            dst: Authority::ClientManager(name),
            content: RequestContent::Connect,
        });
        let full_id = FullId::new();
        let signed_message_result = SignedMessage::new(routing_message.clone(), &full_id);

        assert!(signed_message_result.is_ok());

        let mut signed_message = unwrap_result!(signed_message_result);

        assert_eq!(routing_message, *signed_message.content());
        assert_eq!(full_id.public_id(), signed_message.public_id());

        let check_integrity_result = signed_message.check_integrity();

        assert!(check_integrity_result.is_ok());

        let full_id = FullId::new();
        let bytes_to_sign = unwrap_result!(serialise(&(&routing_message, full_id.public_id())));
        let signature = sign::sign_detached(&bytes_to_sign, full_id.signing_private_key());

        signed_message.signature = signature;

        let check_integrity_result = signed_message.check_integrity();

        assert!(check_integrity_result.is_err());
    }

    #[test]
    fn hop_message_verify() {
        let name: XorName = rand::random();
        let routing_message = RoutingMessage::Request(RequestMessage {
            src: Authority::ClientManager(name),
            dst: Authority::ClientManager(name),
            content: RequestContent::Connect,
        });
        let full_id = FullId::new();
        let signed_message_result = SignedMessage::new(routing_message.clone(), &full_id);

        assert!(signed_message_result.is_ok());

        let signed_message = unwrap_result!(signed_message_result);
        let hop_name: XorName = rand::random();
        let (public_signing_key, secret_signing_key) = sign::gen_keypair();
        let hop_message_result = HopMessage::new(signed_message.clone(),
                                                 hop_name,
                                                 &secret_signing_key);

        assert!(hop_message_result.is_ok());

        let hop_message = unwrap_result!(hop_message_result);

        assert_eq!(signed_message, *hop_message.content());
        assert_eq!(hop_name, *hop_message.name());

        let verify_result = hop_message.verify(&public_signing_key);

        assert!(verify_result.is_ok());

        let (public_signing_key, _) = sign::gen_keypair();
        let verify_result = hop_message.verify(&public_signing_key);

        assert!(verify_result.is_err());
    }
}
