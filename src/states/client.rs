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

use ack_manager::{Ack, AckManager};
use action::Action;
use authority::Authority;
use crust::{PeerId, Service};
use crust::Event as CrustEvent;
use error::{InterfaceError, RoutingError};
use event::Event;
use id::{FullId, PublicId};
use maidsafe_utilities::serialisation;
use messages::{HopMessage, Message, MessageContent, RoutingMessage, SignedMessage, UserMessage,
               UserMessageCache};
use peer_manager::MIN_GROUP_SIZE;
use routing_message_filter::RoutingMessageFilter;
use state_machine::Transition;
use stats::Stats;
use std::fmt::{self, Debug, Formatter};
use std::sync::mpsc::Sender;
use std::time::Duration;
use super::common::{Base, Bootstrapped, USER_MSG_CACHE_EXPIRY_DURATION_SECS};
use timer::Timer;

/// A node connecting a user to the network, as opposed to a routing / data storage node.
///
/// Each client has a _proxy_: a node through which all requests are routed.
pub struct Client {
    ack_mgr: AckManager,
    crust_service: Service,
    event_sender: Sender<Event>,
    full_id: FullId,
    proxy_peer_id: PeerId,
    proxy_public_id: PublicId,
    routing_msg_filter: RoutingMessageFilter,
    stats: Stats,
    timer: Timer,
    user_msg_cache: UserMessageCache,
}

impl Client {
    #[cfg_attr(feature = "clippy", allow(too_many_arguments))]
    pub fn from_bootstrapping(crust_service: Service,
                              event_sender: Sender<Event>,
                              full_id: FullId,
                              proxy_peer_id: PeerId,
                              proxy_public_id: PublicId,
                              stats: Stats,
                              timer: Timer)
                              -> Self {
        let client = Client {
            ack_mgr: AckManager::new(),
            crust_service: crust_service,
            event_sender: event_sender,
            full_id: full_id,
            proxy_peer_id: proxy_peer_id,
            proxy_public_id: proxy_public_id,
            routing_msg_filter: RoutingMessageFilter::new(),
            stats: stats,
            timer: timer,
            user_msg_cache: UserMessageCache::with_expiry_duration(
                Duration::from_secs(USER_MSG_CACHE_EXPIRY_DURATION_SECS)),
        };

        client.send_event(Event::Connected);

        debug!("{:?} - State changed to client.", client);

        client
    }

    pub fn handle_action(&mut self, action: Action) -> Transition {
        match action {
            Action::ClientSendRequest { content, dst, priority, result_tx } => {
                let src = Authority::Client {
                    client_key: *self.full_id.public_id().signing_public_key(),
                    proxy_node_name: *self.proxy_public_id.name(),
                    peer_id: self.crust_service.id(),
                };

                let user_msg = UserMessage::Request(content);
                let result = match self.send_user_message(src, dst, user_msg, priority) {
                    Err(RoutingError::Interface(err)) => Err(err),
                    Err(_) | Ok(_) => Ok(()),
                };

                let _ = result_tx.send(result);
            }
            Action::NodeSendMessage { result_tx, .. } => {
                let _ = result_tx.send(Err(InterfaceError::InvalidState));
            }
            Action::CloseGroup { result_tx, .. } => {
                let _ = result_tx.send(None);
            }
            Action::Name { result_tx } => {
                let _ = result_tx.send(*self.name());
            }
            Action::Timeout(token) => self.handle_timeout(token),
            Action::Terminate => {
                return Transition::Terminate;
            }
        }

        Transition::Stay
    }

    pub fn handle_crust_event(&mut self, crust_event: CrustEvent) -> Transition {
        match crust_event {
            CrustEvent::LostPeer(peer_id) => self.handle_lost_peer(peer_id),
            CrustEvent::NewMessage(peer_id, bytes) => self.handle_new_message(peer_id, bytes),
            _ => {
                debug!("{:?} Unhandled crust event {:?}", self, crust_event);
                Transition::Stay
            }
        }
    }

    fn handle_ack_response(&mut self, ack: Ack) -> Transition {
        self.ack_mgr.receive(ack);
        Transition::Stay
    }

    fn handle_timeout(&mut self, token: u64) {
        self.resend_unacknowledged_timed_out_msgs(token);
    }

    fn handle_new_message(&mut self, peer_id: PeerId, bytes: Vec<u8>) -> Transition {
        let result = match serialisation::deserialise(&bytes) {
            Ok(Message::Hop(hop_msg)) => self.handle_hop_message(hop_msg, peer_id),
            Ok(message) => {
                debug!("{:?} - Unhandled new message: {:?}", self, message);
                Ok(Transition::Stay)
            }
            Err(error) => Err(RoutingError::SerialisationError(error)),
        };

        match result {
            Ok(transition) => transition,
            Err(RoutingError::FilterCheckFailed) => Transition::Stay,
            Err(error) => {
                debug!("{:?} - {:?}", self, error);
                Transition::Stay
            }
        }
    }

    fn handle_hop_message(&mut self,
                          hop_msg: HopMessage,
                          peer_id: PeerId)
                          -> Result<Transition, RoutingError> {

        if self.proxy_peer_id == peer_id {
            try!(hop_msg.verify(self.proxy_public_id.signing_public_key()));
        } else {
            return Err(RoutingError::UnknownConnection(peer_id));
        }

        let signed_msg = hop_msg.content();
        try!(signed_msg.check_integrity());

        let routing_msg = signed_msg.routing_message();
        let in_authority = self.in_authority(&routing_msg.dst);
        if in_authority {
            self.send_ack(routing_msg, 0);
        }

        // Prevents us repeatedly handling identical messages sent by a malicious peer.
        if self.routing_msg_filter.filter_incoming(routing_msg, hop_msg.route) != 1 {
            return Err(RoutingError::FilterCheckFailed);
        }

        if !in_authority {
            return Ok(Transition::Stay);
        }

        self.dispatch_routing_message(routing_msg.clone())
    }

    fn dispatch_routing_message(&mut self,
                                routing_msg: RoutingMessage)
                                -> Result<Transition, RoutingError> {
        match routing_msg.content {
            MessageContent::Ack(ack, _) => Ok(self.handle_ack_response(ack)),
            MessageContent::UserMessagePart { hash, part_count, part_index, payload, .. } => {
                trace!("{:?} Got UserMessagePart {:x}, {}/{} from {:?} to {:?}.",
                       self,
                       hash,
                       part_count,
                       part_index,
                       routing_msg.src,
                       routing_msg.dst);
                if let Some(msg) = self.user_msg_cache.add(hash, part_count, part_index, payload) {
                    self.stats().count_user_message(&msg);
                    self.send_event(msg.into_event(routing_msg.src, routing_msg.dst));
                }
                Ok(Transition::Stay)
            }
            content => {
                debug!("{:?} - Unhandled routing message: {:?} from {:?} to {:?}",
                       self,
                       content,
                       routing_msg.src,
                       routing_msg.dst);
                Ok(Transition::Stay)
            }
        }
    }

    /// Sends the given message, possibly splitting it up into smaller parts.
    fn send_user_message(&mut self,
                         src: Authority,
                         dst: Authority,
                         user_msg: UserMessage,
                         priority: u8)
                         -> Result<(), RoutingError> {
        self.stats.count_user_message(&user_msg);
        for part in try!(user_msg.to_parts(priority)) {
            try!(self.send_routing_message(RoutingMessage {
                src: src,
                dst: dst,
                content: part,
            }));
        }
        Ok(())
    }

    /// Does the given authority represent us?
    fn in_authority(&self, auth: &Authority) -> bool {
        if let Authority::Client { ref client_key, .. } = *auth {
            client_key == self.full_id.public_id().signing_public_key()
        } else {
            false
        }
    }
}

impl Base for Client {
    fn crust_service(&self) -> &Service {
        &self.crust_service
    }

    fn full_id(&self) -> &FullId {
        &self.full_id
    }

    fn handle_lost_peer(&mut self, peer_id: PeerId) -> Transition {
        if peer_id == self.crust_service().id() {
            error!("{:?} LostPeer fired with our crust peer id", self);
            return Transition::Stay;
        }

        debug!("{:?} Received LostPeer - {:?}", self, peer_id);

        if self.proxy_peer_id == peer_id {
            debug!("{:?} Lost bootstrap connection to {:?} ({:?}).",
                   self,
                   self.proxy_public_id.name(),
                   peer_id);
            self.send_event(Event::Terminate);
            Transition::Terminate
        } else {
            Transition::Stay
        }
    }

    fn stats(&mut self) -> &mut Stats {
        &mut self.stats
    }

    fn send_event(&self, event: Event) {
        let _ = self.event_sender.send(event);
    }
}

impl Bootstrapped for Client {
    fn ack_mgr(&self) -> &AckManager {
        &self.ack_mgr
    }

    fn ack_mgr_mut(&mut self) -> &mut AckManager {
        &mut self.ack_mgr
    }

    fn resend_unacknowledged_timed_out_msgs(&mut self, token: u64) {
        if let Some((unacked_msg, ack)) = self.ack_mgr.find_timed_out(token) {
            trace!("{:?} - Timed out waiting for ack({}) {:?}",
                   self,
                   ack,
                   unacked_msg);

            if unacked_msg.route as usize == MIN_GROUP_SIZE {
                debug!("{:?} - Message unable to be acknowledged - giving up. {:?}",
                       self,
                       unacked_msg);
                self.stats.count_unacked();
            } else if let Err(error) =
                   self.send_routing_message_via_route(unacked_msg.routing_msg, unacked_msg.route) {
                debug!("{:?} Failed to send message: {:?}", self, error);
            }
        }
    }

    fn send_routing_message_via_route(&mut self,
                                      routing_msg: RoutingMessage,
                                      route: u8)
                                      -> Result<(), RoutingError> {
        self.stats.count_route(route);

        if routing_msg.dst.is_client() && self.in_authority(&routing_msg.dst) {
            return Ok(()); // Message is for us.
        }

        // Get PeerId of the proxy node
        let proxy_peer_id = if let Authority::Client { ref proxy_node_name, .. } =
                                   routing_msg.src {
            if *self.proxy_public_id.name() == *proxy_node_name {
                self.proxy_peer_id
            } else {
                error!("{:?} - Unable to find connection to proxy node in proxy map",
                       self);
                return Err(RoutingError::ProxyConnectionNotFound);
            }
        } else {
            error!("{:?} - Source should be client if our state is a Client",
                   self);
            return Err(RoutingError::InvalidSource);
        };

        let signed_msg = try!(SignedMessage::new(routing_msg, &self.full_id()));

        if !self.add_to_pending_acks(&signed_msg, route) {
            return Ok(());
        }

        if !self.filter_outgoing_routing_msg(signed_msg.routing_message(), &proxy_peer_id, route) {
            let bytes = try!(self.to_hop_bytes(signed_msg.clone(), route, Vec::new()));

            if let Err(error) = self.send_or_drop(&proxy_peer_id, bytes, signed_msg.priority()) {
                info!("{:?} - Error sending message to {:?}: {:?}.",
                      self,
                      proxy_peer_id,
                      error);
            }
        }

        Ok(())
    }

    fn routing_msg_filter(&mut self) -> &mut RoutingMessageFilter {
        &mut self.routing_msg_filter
    }

    fn timer(&mut self) -> &mut Timer {
        &mut self.timer
    }
}

#[cfg(feature = "use-mock-crust")]
impl Client {
    /// Resends all unacknowledged messages.
    pub fn resend_unacknowledged(&mut self) -> bool {
        let timer_tokens = self.ack_mgr.timer_tokens();
        for timer_token in &timer_tokens {
            self.resend_unacknowledged_timed_out_msgs(*timer_token);
        }
        !timer_tokens.is_empty()
    }

    /// Are there any unacknowledged messages?
    pub fn has_unacknowledged(&self) -> bool {
        self.ack_mgr.has_pending()
    }
}

impl Debug for Client {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "Client({})", self.name())
    }
}
