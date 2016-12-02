// Copyright 2015 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.1.  This, along with the
// Licenses can be found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

use action::Action;
use authority::Authority;
use cache::NullCache;
use data::{AppendWrapper, Data, DataIdentifier};
use error::{InterfaceError, RoutingError};
use event::Event;
use id::FullId;
#[cfg(not(feature = "use-mock-crust"))]
use maidsafe_utilities::thread::{self, Joiner};
use messages::{CLIENT_GET_PRIORITY, DEFAULT_PRIORITY, Request};
#[cfg(not(feature = "use-mock-crust"))]
use rust_sodium;
use state_machine::{State, StateMachine};
use states;
#[cfg(feature = "use-mock-crust")]
use std::cell::RefCell;
use std::sync::mpsc::{Receiver, Sender, channel};
use types::MessageId;
use types::RoutingActionSender;
use xor_name::XorName;

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
    machine: RefCell<StateMachine>,

    #[cfg(not(feature = "use-mock-crust"))]
    _raii_joiner: Joiner,
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
    pub fn new(event_sender: Sender<Event>,
               keys: Option<FullId>,
               min_group_size: usize)
               -> Result<Client, RoutingError> {
        rust_sodium::init();  // enable shared global (i.e. safe to multithread now)

        // start the handler for routing with a restriction to become a full node
        let (action_sender, mut machine) =
            Self::make_state_machine(event_sender, keys, min_group_size);
        let (tx, rx) = channel();

        let raii_joiner = thread::named("Client thread", move || machine.run());

        Ok(Client {
            interface_result_tx: tx,
            interface_result_rx: rx,
            action_sender: action_sender,
            _raii_joiner: raii_joiner,
        })
    }

    fn make_state_machine(event_sender: Sender<Event>,
                          keys: Option<FullId>,
                          min_group_size: usize)
                          -> (RoutingActionSender, StateMachine) {
        let cache = Box::new(NullCache);
        let full_id = keys.unwrap_or_else(FullId::new);

        StateMachine::new(move |crust_service, timer| {
            State::Bootstrapping(states::Bootstrapping::new(cache,
                                                            true,
                                                            crust_service,
                                                            event_sender,
                                                            full_id,
                                                            min_group_size,
                                                            timer))
        })
    }

    /// Send a Get message with a `DataIdentifier` to an `Authority`, signed with given keys.
    pub fn send_get_request(&self,
                            dst: Authority,
                            data_id: DataIdentifier,
                            message_id: MessageId)
                            -> Result<(), InterfaceError> {
        self.send_action(Request::Get(data_id, message_id), dst, CLIENT_GET_PRIORITY)
    }

    /// Add something to the network
    pub fn send_put_request(&self,
                            dst: Authority,
                            data: Data,
                            message_id: MessageId)
                            -> Result<(), InterfaceError> {
        self.send_action(Request::Put(data, message_id), dst, DEFAULT_PRIORITY)
    }

    /// Change something already on the network
    pub fn send_post_request(&self,
                             dst: Authority,
                             data: Data,
                             message_id: MessageId)
                             -> Result<(), InterfaceError> {
        self.send_action(Request::Post(data, message_id), dst, DEFAULT_PRIORITY)
    }

    /// Remove something from the network
    pub fn send_delete_request(&self,
                               dst: Authority,
                               data: Data,
                               message_id: MessageId)
                               -> Result<(), InterfaceError> {
        self.send_action(Request::Delete(data, message_id), dst, DEFAULT_PRIORITY)
    }

    /// Append an item to appendable data.
    pub fn send_append_request(&self,
                               dst: Authority,
                               wrapper: AppendWrapper,
                               message_id: MessageId)
                               -> Result<(), InterfaceError> {
        self.send_action(Request::Append(wrapper, message_id), dst, DEFAULT_PRIORITY)
    }


    /// Request account information for the Client calling this function
    pub fn send_get_account_info_request(&self,
                                         dst: Authority,
                                         message_id: MessageId)
                                         -> Result<(), InterfaceError> {
        self.send_action(Request::GetAccountInfo(message_id),
                         dst,
                         CLIENT_GET_PRIORITY)
    }

    /// Returns the name of this node.
    pub fn name(&self) -> Result<XorName, InterfaceError> {
        let (result_tx, result_rx) = channel();
        self.action_sender.send(Action::Name { result_tx: result_tx })?;

        self.receive_action_result(&result_rx)
    }

    fn send_action(&self,
                   content: Request,
                   dst: Authority,
                   priority: u8)
                   -> Result<(), InterfaceError> {
        let action = Action::ClientSendRequest {
            content: content,
            dst: dst,
            priority: priority,
            result_tx: self.interface_result_tx.clone(),
        };

        self.action_sender.send(action)?;
        self.receive_action_result(&self.interface_result_rx)?
    }

    #[cfg(not(feature = "use-mock-crust"))]
    fn receive_action_result<T>(&self, rx: &Receiver<T>) -> Result<T, InterfaceError> {
        Ok(rx.recv()?)
    }
}

#[cfg(feature = "use-mock-crust")]
impl Client {
    /// Create a new `Client` for unit testing.
    pub fn new(event_sender: Sender<Event>,
               keys: Option<FullId>,
               min_group_size: usize)
               -> Result<Client, RoutingError> {
        // start the handler for routing with a restriction to become a full node
        let (action_sender, machine) = Self::make_state_machine(event_sender, keys, min_group_size);
        let (tx, rx) = channel();

        Ok(Client {
            interface_result_tx: tx,
            interface_result_rx: rx,
            action_sender: action_sender,
            machine: RefCell::new(machine),
        })
    }

    /// Poll and process all events in this client's `Core` instance.
    pub fn poll(&self) -> bool {
        self.machine.borrow_mut().poll()
    }

    /// Resend all unacknowledged messages.
    pub fn resend_unacknowledged(&self) -> bool {
        self.machine.borrow_mut().current_mut().resend_unacknowledged()
    }

    /// Are there any unacknowledged messages?
    pub fn has_unacknowledged(&self) -> bool {
        self.machine.borrow().current().has_unacknowledged()
    }

    fn receive_action_result<T>(&self, rx: &Receiver<T>) -> Result<T, InterfaceError> {
        while self.poll() {}
        Ok(rx.recv()?)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Err(err) = self.action_sender.send(Action::Terminate) {
            debug!("Error {:?} sending event to Core", err);
        }
    }
}
