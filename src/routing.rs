// Copyright 2015 MaidSafe.net limited.
//
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

use sodiumoxide;
use std::sync::mpsc;

use id::Id;
use action::Action;
use event::Event;
use messages::SignedToken;
use routing_node::RoutingNode;
use NameType;
use data::{Data, DataRequest};
use types::Bytes;
use error::{RoutingError, ResponseError};
use authority::Authority;
use sodiumoxide::crypto;

//use types::{MessageId, Address};
//use utils::{encode, decode};
//use authority::{Authority};
//use messages::{RoutingMessage, SignedMessage, MessageType};
//use error::{RoutingError};
//use std::thread::spawn;
//use std::collections::BTreeMap;

type RoutingResult = Result<(), RoutingError>;

/// Routing provides an actionable interface to RoutingNode.
/// On constructing a new Routing object a RoutingNode will also be started.
/// Routing objects are clonable for multithreading, or a Routing object can be
/// cloned with a new set of keys while preserving a single RoutingNode.
pub struct Routing {
    keys          : Id,
    action_sender : mpsc::Sender<Action>,
}

impl Routing {
    /// Starts a new RoutingIdentity, which will also start a new RoutingNode.
    /// The RoutingNode will attempt to achieve full routing node status.
    /// The intial Routing object will have newly generated keys
    // TODO(dirvine) Always start a node if possible  :09/08/2015
    pub fn new(event_sender : mpsc::Sender<Event>) -> Result<Routing, RoutingError> {
        sodiumoxide::init();  // enable shared global (i.e. safe to multithread now)

        let keys = Id::new();
        let (action_sender, action_receiver) = mpsc::channel::<Action>();

        // TODO (ben 5/08/2015) Errors on starting RoutingNode should more aggressively
        //      be handled internally
        // start the handler for routing
        let routing_node = match RoutingNode::new(action_sender.clone(), action_receiver,
            event_sender) {
                Ok(routing_node) => routing_node,
                Err(e) => return Err(e),
        };
        Ok(Routing {
            keys          : keys,
            action_sender : action_sender,
        })
    }

    /// Starts a new RoutingIdentity, which will also start a new RoutingNode.
    /// The RoutingNode will only bootstrap to the network and not attempt to
    /// achieve full routing node status.
    // TODO(dirvine) take an Id as a param to sign messages ???? (or amend put etc. for a client put_request to take reference to a particular ID for sign/encryt, we should be already bootstrapped anyway with the new() call :09/08/2015
    // FIXME(dirvine) discussion required :09/08/2015
    pub fn new_client(event_receiver : mpsc::Sender<Event>)
        -> Result<Routing, RoutingError> {
        unimplemented!()
    }

    /// Clone the interface while maintaining the same RoutingNode, with a given set of keys.
    pub fn clone_with_keys(&self, keys : Id) -> Routing {
        unimplemented!()
    }

    /// Send a Get message with a DataRequest to an Authority, signed with given keys.
    pub fn get_request(&self, location : Authority, data_request : DataRequest) {
        unimplemented!()
    }

    /// Add something to the network
    pub fn put_request(&self, location : Authority, data : Data) {
        unimplemented!()
    }

    /// Change something already on the network
    pub fn post_request(&self, location : Authority, data : Data) {
        unimplemented!()
    }

    /// Remove something from the network
    pub fn delete_request(&self, location : Authority, data_request : DataRequest) {
        unimplemented!()
    }
    /// Respond to a get_request (no error can be sent)
    /// If we received the request from a group, we'll not get the signed_token.
    pub fn get_response(&self, location : Authority, data: Data, signed_token : Option<SignedToken>) {
        unimplemented!()
    }
    // FIXME(dirvine) perhaps all responses here shoudl be a single respond_error fn instead 
    // Also these shoudl return an error so if not yet a node they fail (if clients try and call for instance) :09/08/2015
    /// response error to a put request
    pub fn put_response(&self, location : Authority, response_error : ResponseError,
        signed_token : SignedToken) {
        unimplemented!()
    }
    /// Response error to a post request
    pub fn post_response(&self, location : Authority, response_error : ResponseError,
        signed_token : SignedToken) {
        unimplemented!()
    }
    /// response error to a delete respons
    pub fn delete_response(&self, location : Authority, response_error : ResponseError,
        signed_token : SignedToken) {
        unimplemented!()
    }

    /// Refresh the content in the close group nodes of group address content::name.
    /// This method needs to be called when churn is triggered.
    /// all the group members need to call this, otherwise it will not be resolved as a valid
    /// content.
    pub fn refresh_request(&self, type_tag: u64, from_group: NameType, content: Bytes) {
        unimplemented!()
    }

    /// Signal to RoutingNode that it needs to refuse new messages and handle all outstanding
    /// messages.  After handling all messages it will send an Event::Terminated to the user.
    // TODO(dirvine) This maybe should be implementing  aDrop trait  :09/08/2015
    pub fn stop(&mut self) {
        unimplemented!()
    }

    pub fn signing_public_key(&self) -> crypto::sign::PublicKey {
        self.keys.signing_public_key()
    }
}
