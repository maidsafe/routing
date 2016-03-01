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
use maidsafe_utilities::thread::RaiiThreadJoiner;
use sodiumoxide;
use std::sync::mpsc::{Receiver, Sender, channel};

use id::FullId;
use action::Action;
use event::Event;
use core::Core;
use data::{Data, DataRequest};
use error::{InterfaceError, RoutingError};
use authority::Authority;
use messages::RequestContent;
use types::MessageId;

type RoutingResult = Result<(), RoutingError>;

/// Interface for sending and receiving messages to and from a network of nodes in the role of a
/// client.
///
/// A client is connected to the network via one or more nodes. Messages are never routed via a
/// client, and a client cannot be part of a group authority.
pub struct Client {
    interface_result_tx: Sender<Result<(), InterfaceError>>,
    interface_result_rx: Receiver<Result<(), InterfaceError>>,
    action_sender: ::types::RoutingActionSender,

    #[cfg(feature = "use-mock-crust")]
    core: Core,

    #[cfg(not(feature = "use-mock-crust"))]
    _raii_joiner: ::maidsafe_utilities::thread::RaiiThreadJoiner,
}

impl Client {
    /// Create a new `Client`.
    ///
    /// It will automatically connect to the network, but not attempt to achieve full routing node
    /// status. The name of the client will be the name of the `PublicId` of the `keys` and must
    /// equal the SHA512 hash of its public signing key, otherwise the client will be instantly
    /// terminated.
    ///
    /// Keys will be exchanged with the `ClientAuthority` so that communication with the network is
    /// cryptographically secure and uses group consensus. The restriction for the client name
    /// exists to ensure that the client cannot choose its `ClientAuthority`.
    #[cfg(not(feature = "use-mock-crust"))]
    pub fn new(event_sender: Sender<Event>, keys: Option<FullId>) -> Result<Client, RoutingError> {
        sodiumoxide::init();  // enable shared global (i.e. safe to multithread now)

        // start the handler for routing with a restriction to become a full node
        let (action_sender, mut core) = Core::new(event_sender, true, keys);
        let (tx, rx) = channel();

        let raii_joiner = RaiiThreadJoiner::new(thread!("Client thread", move || {
            core.run();
        }));

        Ok(Client {
            interface_result_tx: tx,
            interface_result_rx: rx,
            action_sender: action_sender,
            _raii_joiner: raii_joiner,
        })
    }

    /// Create a new `Client` for unit testing.
    #[cfg(feature = "use-mock-crust")]
    pub fn new(event_sender: Sender<Event>, keys: Option<FullId>) -> Result<Client, RoutingError> {
        sodiumoxide::init();  // enable shared global (i.e. safe to multithread now)

        // start the handler for routing with a restriction to become a full node
        let (action_sender, core) = Core::new(event_sender, true, keys);
        let (tx, rx) = channel();

        Ok(Client {
            interface_result_tx: tx,
            interface_result_rx: rx,
            action_sender: action_sender,
            core: core,
        })
    }

    #[cfg(feature = "use-mock-crust")]
    #[allow(missing_docs)]
    pub fn poll(&mut self) -> bool {
        self.core.poll()
    }

    /// Send a Get message with a DataRequest to an Authority, signed with given keys.
    pub fn send_get_request(&mut self,
                            dst: Authority,
                            data_request: DataRequest)
                            -> Result<(), InterfaceError> {
        self.send_action(RequestContent::Get(data_request, MessageId::new()), dst)
    }

    /// Add something to the network
    pub fn send_put_request(&self, dst: Authority, data: Data) -> Result<(), InterfaceError> {
        self.send_action(RequestContent::Put(data, MessageId::new()), dst)
    }

    /// Change something already on the network
    pub fn send_post_request(&self, dst: Authority, data: Data) -> Result<(), InterfaceError> {
        self.send_action(RequestContent::Post(data, MessageId::new()), dst)
    }

    /// Remove something from the network
    pub fn send_delete_request(&self, dst: Authority, data: Data) -> Result<(), InterfaceError> {
        self.send_action(RequestContent::Delete(data, MessageId::new()), dst)
    }

    fn send_action(&self, content: RequestContent, dst: Authority) -> Result<(), InterfaceError> {
        let action = Action::ClientSendRequest {
            content: content,
            dst: dst,
            result_tx: self.interface_result_tx.clone(),
        };

        try!(self.action_sender.send(action));

        try!(self.interface_result_rx.recv())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Err(err) = self.action_sender.send(Action::Terminate) {
            error!("Error {:?} sending event to Core", err);
        }
    }
}
