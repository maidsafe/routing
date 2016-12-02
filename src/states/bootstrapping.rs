// Copyright 2016 MaidSafe.net limited.
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
use cache::Cache;
use crust::{PeerId, Service};
use crust::Event as CrustEvent;
use error::RoutingError;
use event::Event;
use id::{FullId, PublicId};
use maidsafe_utilities::serialisation;
use messages::{DirectMessage, Message};
use rust_sodium::crypto::hash::sha256;
use rust_sodium::crypto::sign;
use state_machine::Transition;
use stats::Stats;
use std::collections::HashSet;
use std::fmt::{self, Debug, Formatter};
use std::net::SocketAddr;
use std::sync::mpsc::Sender;
use std::time::Duration;
use super::{Client, Node};
use super::common::Base;
use timer::Timer;
use xor_name::XorName;

// Time (in seconds) after which bootstrap is cancelled (and possibly retried).
const BOOTSTRAP_TIMEOUT_SECS: u64 = 20;

// State of Client or Node while bootstrapping.
pub struct Bootstrapping {
    bootstrap_blacklist: HashSet<SocketAddr>,
    bootstrap_connection: Option<(PeerId, u64)>,
    cache: Box<Cache>,
    client_restriction: bool,
    crust_service: Service,
    event_sender: Sender<Event>,
    full_id: FullId,
    min_group_size: usize,
    stats: Stats,
    timer: Timer,
}

impl Bootstrapping {
    pub fn new(cache: Box<Cache>,
               client_restriction: bool,
               mut crust_service: Service,
               event_sender: Sender<Event>,
               full_id: FullId,
               min_group_size: usize,
               timer: Timer)
               -> Self {
        let _ = crust_service.start_bootstrap(HashSet::new());

        Bootstrapping {
            bootstrap_blacklist: HashSet::new(),
            bootstrap_connection: None,
            cache: cache,
            client_restriction: client_restriction,
            crust_service: crust_service,
            event_sender: event_sender,
            full_id: full_id,
            min_group_size: min_group_size,
            stats: Stats::new(),
            timer: timer,
        }
    }

    pub fn handle_action(&mut self, action: Action) -> Transition {
        match action {
            Action::ClientSendRequest { ref result_tx, .. } |
            Action::NodeSendMessage { ref result_tx, .. } => {
                warn!("{:?} - Cannot handle {:?} - not bootstrapped", self, action);
                // TODO: return Err here eventually. Returning Ok for now to
                // preserve the pre-refactor behaviour.
                let _ = result_tx.send(Ok(()));
            }
            Action::Name { result_tx } => {
                let _ = result_tx.send(*self.name());
            }
            Action::Timeout(token) => self.handle_timeout(token),
            Action::Terminate => {
                return Transition::Terminate;
            }

            // TODO: these actions make no sense in this state, but we handle
            // them for now, to preserve the pre-refactor behaviour.
            Action::CloseGroup { result_tx, .. } => {
                let _ = result_tx.send(None);
            }
        }

        Transition::Stay
    }

    pub fn handle_crust_event(&mut self, crust_event: CrustEvent) -> Transition {
        match crust_event {
            CrustEvent::BootstrapConnect(peer_id, socket_addr) => {
                self.handle_bootstrap_connect(peer_id, socket_addr)
            }
            CrustEvent::BootstrapFailed => self.handle_bootstrap_failed(),
            CrustEvent::NewMessage(peer_id, bytes) => {
                match self.handle_new_message(peer_id, bytes) {
                    Ok(transition) => transition,
                    Err(error) => {
                        debug!("{:?} - {:?}", self, error);
                        Transition::Stay
                    }
                }
            }
            _ => {
                debug!("{:?} Unhandled crust event {:?}", self, crust_event);
                Transition::Stay
            }
        }
    }

    pub fn into_client(self, proxy_peer_id: PeerId, proxy_public_id: PublicId) -> Client {
        Client::from_bootstrapping(self.crust_service,
                                   self.event_sender,
                                   self.full_id,
                                   self.min_group_size,
                                   proxy_peer_id,
                                   proxy_public_id,
                                   self.stats,
                                   self.timer)
    }

    pub fn into_node(self, proxy_peer_id: PeerId, proxy_public_id: PublicId) -> Option<Node> {
        Node::from_bootstrapping(self.cache,
                                 self.crust_service,
                                 self.event_sender,
                                 self.full_id,
                                 self.min_group_size,
                                 proxy_peer_id,
                                 proxy_public_id,
                                 self.stats,
                                 self.timer)
    }

    pub fn client_restriction(&self) -> bool {
        self.client_restriction
    }

    fn handle_timeout(&mut self, token: u64) {
        if let Some((bootstrap_id, bootstrap_token)) = self.bootstrap_connection {
            if bootstrap_token == token {
                debug!("{:?} Timeout when trying to bootstrap against {:?}.",
                       self,
                       bootstrap_id);

                self.rebootstrap();
            }
        }
    }

    fn handle_bootstrap_connect(&mut self, peer_id: PeerId, socket_addr: SocketAddr) -> Transition {
        match self.bootstrap_connection {
            None => {
                debug!("{:?} Received BootstrapConnect from {:?}.", self, peer_id);
                // Established connection. Pending Validity checks
                let _ = self.send_client_identify(peer_id);
                let _ = self.bootstrap_blacklist.insert(socket_addr);
            }
            Some((bootstrap_id, _)) if bootstrap_id == peer_id => {
                warn!("{:?} Got more than one BootstrapConnect for peer {:?}.",
                      self,
                      peer_id);
            }
            _ => {
                self.disconnect_peer(&peer_id);
            }
        }

        Transition::Stay
    }

    fn handle_bootstrap_failed(&mut self) -> Transition {
        debug!("{:?} Failed to bootstrap.", self);
        self.send_event(Event::Terminate);
        Transition::Terminate
    }

    fn handle_new_message(&mut self,
                          peer_id: PeerId,
                          bytes: Vec<u8>)
                          -> Result<Transition, RoutingError> {
        match serialisation::deserialise(&bytes) {
            Ok(Message::Direct(direct_msg)) => Ok(self.handle_direct_message(direct_msg, peer_id)),
            Ok(message) => {
                debug!("{:?} - Unhandled new message: {:?}", self, message);
                Ok(Transition::Stay)
            }
            Err(error) => Err(From::from(error)),
        }
    }

    fn handle_direct_message(&mut self,
                             direct_message: DirectMessage,
                             peer_id: PeerId)
                             -> Transition {
        match direct_message {
            DirectMessage::BootstrapIdentify { public_id } => {
                self.handle_bootstrap_identify(public_id, peer_id)
            }
            DirectMessage::BootstrapDeny => self.handle_bootstrap_deny(),
            _ => {
                debug!("{:?} - Unhandled direct message: {:?}",
                       self,
                       direct_message);
                Transition::Stay
            }
        }
    }

    fn handle_bootstrap_identify(&mut self, public_id: PublicId, peer_id: PeerId) -> Transition {
        if *public_id.name() == XorName(sha256::hash(&public_id.signing_public_key().0).0) {
            warn!("{:?} Incoming Connection not validated as a proper node - dropping",
                  self);
            self.rebootstrap();
            return Transition::Stay;
        }

        Transition::IntoBootstrapped {
            proxy_peer_id: peer_id,
            proxy_public_id: public_id,
        }
    }

    fn handle_bootstrap_deny(&mut self) -> Transition {
        info!("{:?} Connection failed: Proxy node needs a larger routing table to accept \
               clients.",
              self);
        self.rebootstrap();
        Transition::Stay
    }

    fn send_client_identify(&mut self, peer_id: PeerId) -> Result<(), RoutingError> {
        debug!("{:?} - Sending ClientIdentify to {:?}.", self, peer_id);

        let token = self.timer.schedule(Duration::from_secs(BOOTSTRAP_TIMEOUT_SECS));
        self.bootstrap_connection = Some((peer_id, token));

        let serialised_public_id = serialisation::serialise(self.full_id.public_id())?;
        let signature = sign::sign_detached(&serialised_public_id,
                                            self.full_id.signing_private_key());

        let direct_message = DirectMessage::ClientIdentify {
            serialised_public_id: serialised_public_id,
            signature: signature,
            client_restriction: self.client_restriction,
        };

        self.stats().count_direct_message(&direct_message);
        self.send_message(&peer_id, Message::Direct(direct_message))
    }

    fn disconnect_peer(&mut self, peer_id: &PeerId) {
        debug!("{:?} Disconnecting {:?}. Calling crust::Service::disconnect.",
               self,
               peer_id);
        let _ = self.crust_service.disconnect(*peer_id);
    }

    fn rebootstrap(&mut self) {
        if let Some((bootstrap_id, _)) = self.bootstrap_connection.take() {
            debug!("{:?} Dropping bootstrap node {:?} and retrying.",
                   self,
                   bootstrap_id);
            self.crust_service.disconnect(bootstrap_id);
            let _ = self.crust_service.start_bootstrap(self.bootstrap_blacklist.clone());
        }
    }
}

impl Base for Bootstrapping {
    fn crust_service(&self) -> &Service {
        &self.crust_service
    }

    fn full_id(&self) -> &FullId {
        &self.full_id
    }

    fn send_event(&self, event: Event) {
        let _ = self.event_sender.send(event);
    }

    fn stats(&mut self) -> &mut Stats {
        &mut self.stats
    }
}

impl Debug for Bootstrapping {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "Bootstrapping({})", self.name())
    }
}
