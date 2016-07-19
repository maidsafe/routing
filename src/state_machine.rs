// Copyright 2016 MaidSafe.net limited.
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

use crust::{CrustEventSender, Service};
use crust::Event as CrustEvent;
use maidsafe_utilities::event_sender::MaidSafeEventCategory;
use std::mem;
use std::sync::mpsc::{self, Receiver};

use action::Action;
use timer::Timer;
use types::RoutingActionSender;

/// Holds the current state and handles state transitions.
///
/// # The bootstrap process
///
///
/// ## Bootstrapping a client
///
/// A newly created `Core`, A, starts in `Disconnected` state and tries to establish a connection to
/// any node B of the network via Crust. When successful, i. e. when receiving an `OnConnect` event,
/// it moves to the `Bootstrapping` state.
///
/// A now sends a `ClientIdentify` message to B, containing A's signed public ID. B verifies the
/// signature and responds with a `BootstrapIdentify`, containing B's public ID and the current
/// quorum size. Once it receives that, A goes into the `Client` state and uses B as its proxy to
/// the network.
///
/// A can now exchange messages with any `Authority`. This completes the bootstrap process for
/// clients.
///
///
/// ## Becoming a node
///
/// If A wants to become a full routing node (`client_restriction == false`), it needs to relocate,
/// i. e. change its name to a value chosen by the network, and then add its peers to its routing
/// table and get added to their routing tables.
///
///
/// ### Getting a new network name from the `NaeManager`
///
/// Once in `Client` state, A sends a `GetNodeName` request to the `NaeManager` group authority X
/// of A's current name. X computes a new name and sends it in an `ExpectCloseNode` request to  the
/// `NaeManager` Y of A's new name. Each member of Y caches A's public ID, and Y sends a
/// `GetNodeName` response back to A, which includes the public IDs of the members of Y.
///
///
/// ### Connecting to the close group
///
/// To the `ManagedNode` for each public ID it receives from members of Y, A sends its
/// `ConnectionInfo`. It also caches the ID.
///
/// For each `ConnectionInfo` that a node Z receives from A, it decides whether it wants A in its
/// routing table. If yes, and if A's ID is in its ID cache, Z sends its own `ConnectionInfo` back
/// to A and also attempts to connect to A via Crust. A does the same, once it receives the
/// `ConnectionInfo`.
///
/// Once the connection between A and Z is established and a Crust `OnConnect` event is raised,
/// they exchange `NodeIdentify` messages and add each other to their routing tables. When A
/// receives its first `NodeIdentify`, it finally moves to the `Node` state.
pub struct StateMachine<T> {
    state: T,
    category_rx: Receiver<MaidSafeEventCategory>,
    crust_rx: Receiver<CrustEvent>,
    action_rx: Receiver<Action>,
    is_running: bool,
}

pub trait State {
    fn terminated() -> Self;
    fn transitioning() -> Self;
    fn is_terminated(&self) -> bool;
    fn handle_action(&mut self, action: Action) -> Transition;
    fn handle_crust_event(&mut self, event: CrustEvent) -> Transition;
    fn into_next(self) -> Self;
}

pub enum Transition {
    // Stay in the current state
    Stay,
    // Go to the next state
    Next,
    // Terminate
    Terminate,
}

impl<T: State> StateMachine<T> {
    pub fn new<F>(f: F) -> (RoutingActionSender, Self)
        where F: FnOnce(Service, Timer) -> T
    {
        let (category_tx, category_rx) = mpsc::channel();
        let (crust_tx, crust_rx) = mpsc::channel();
        let (action_tx, action_rx) = mpsc::channel();

        let action_sender = RoutingActionSender::new(action_tx,
                                                     MaidSafeEventCategory::Routing,
                                                     category_tx.clone());

        let crust_sender =
            CrustEventSender::new(crust_tx, MaidSafeEventCategory::Crust, category_tx);

        let mut crust_service = match Service::new(crust_sender) {
            Ok(service) => service,
            Err(error) => panic!("Unable to start crust::Service {:?}", error),
        };
        crust_service.start_service_discovery();

        let timer = Timer::new(action_sender.clone());

        let state = f(crust_service, timer);
        let is_running = !state.is_terminated();

        let machine = StateMachine {
            category_rx: category_rx,
            crust_rx: crust_rx,
            action_rx: action_rx,
            state: state,
            is_running: is_running,
        };

        (action_sender, machine)
    }

    /// If there is an event in the queue, processes it and returns true.
    /// otherwise returns false. Never blocks.
    #[cfg(feature = "use-mock-crust")]
    pub fn poll(&mut self) -> bool {
        if !self.is_running {
            return false;
        }

        match self.category_rx.try_recv() {
            Ok(category) => {
                self.handle_event(category);
                true
            }
            _ => false,
        }
    }

    /// Run the event loop for sending and receiving messages. Blocks until
    /// the core is terminated, so it must be called in a separate thread.
    #[cfg(not(feature = "use-mock-crust"))]
    pub fn run(&mut self) {
        while self.is_running {
            if let Ok(category) = self.category_rx.recv() {
                self.handle_event(category);
            } else {
                break;
            }
        }
    }

    /// Get reference to the current state.
    #[cfg(feature = "use-mock-crust")]
    pub fn current(&self) -> &T {
        &self.state
    }

    /// Get mutable reference to the current state.
    #[cfg(feature = "use-mock-crust")]
    pub fn current_mut(&mut self) -> &mut T {
        &mut self.state
    }

    fn handle_event(&mut self, category: MaidSafeEventCategory) {
        let transition = match category {
            MaidSafeEventCategory::Routing => {
                if let Ok(action) = self.action_rx.try_recv() {
                    self.state.handle_action(action)
                } else {
                    Transition::Terminate
                }
            }
            MaidSafeEventCategory::Crust => {
                if let Ok(crust_event) = self.crust_rx.try_recv() {
                    self.state.handle_crust_event(crust_event)
                } else {
                    Transition::Terminate
                }
            }
        };

        match transition {
            Transition::Stay => (),
            Transition::Terminate => self.terminate(),
            Transition::Next => self.next(),
        }
    }

    fn next(&mut self) {
        let prev_state = mem::replace(&mut self.state, State::transitioning());
        self.state = prev_state.into_next()
    }

    fn terminate(&mut self) {
        self.is_running = false;
    }
}
