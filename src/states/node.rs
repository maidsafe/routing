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

use crust::{ConnectionInfoResult, CrustError, PeerId, PrivConnectionInfo, PubConnectionInfo,
            Service};
use crust::Event as CrustEvent;
use itertools::Itertools;
#[cfg(feature = "use-mock-crust")]
use kademlia_routing_table::RoutingTable;
use kademlia_routing_table::{AddedNodeDetails, ContactInfo, DroppedNodeDetails};
use maidsafe_utilities::serialisation;
use sodiumoxide::crypto::{box_, sign};
use sodiumoxide::crypto::hash::sha256;
use std::{cmp, fmt, iter};
use std::fmt::{Debug, Formatter};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use ack_manager::{Ack, AckManager};
use action::Action;
use authority::Authority;
use cache::Cache;
use error::{InterfaceError, RoutingError};
use event::Event;
use id::{FullId, PublicId};
use message_accumulator::MessageAccumulator;
use message_filter::MessageFilter;
use messages::{DEFAULT_PRIORITY, DirectMessage, HopMessage, Message, MessageContent,
               RoutingMessage, SignedMessage, UserMessage, UserMessageCache};
use peer_manager::{ConnectionInfoPreparedResult, ConnectionInfoReceivedResult, GROUP_SIZE,
                   PeerManager, QUORUM_SIZE};
use signed_message_filter::SignedMessageFilter;
use state_machine::Transition;
use stats::Stats;
use super::common::{Base, Bootstrapped, USER_MSG_CACHE_EXPIRY_DURATION_SECS};
use timer::Timer;
use tunnels::Tunnels;
use types::MessageId;
use utils;
use xor_name::{XOR_NAME_BITS, XorName};

/// Time (in seconds) after which a `Tick` event is sent.
const TICK_TIMEOUT_SECS: u64 = 60;
/// Time (in seconds) after which a `GetNodeName` request is resent.
const GET_NODE_NAME_TIMEOUT_SECS: u64 = 60;
/// Time (in seconds) the new close group waits for a joining node it sent a network name to.
const SENT_NETWORK_NAME_TIMEOUT_SECS: u64 = 30;
/// Initial period for requesting bucket close groups of all non-full buckets. This is doubled each
/// time.
const REFRESH_BUCKET_GROUPS_SECS: u64 = 120;

pub struct Node {
    ack_mgr: AckManager,
    bucket_filter: MessageFilter<usize>,
    bucket_refresh_token_and_delay: Option<(u64, u64)>,
    cacheable_user_msg_cache: UserMessageCache,
    crust_service: Service,
    event_sender: Sender<Event>,
    full_id: FullId,
    get_node_name_timer_token: Option<u64>,
    is_first_node: bool,
    msg_accumulator: MessageAccumulator,
    peer_mgr: PeerManager,
    response_cache: Box<Cache>,
    /// The last joining node we have sent a `GetNodeName` response to, and when.
    sent_network_name_to: Option<(XorName, Instant)>,
    signed_msg_filter: SignedMessageFilter,
    stats: Stats,
    tick_timer_token: u64,
    timer: Timer,
    tunnels: Tunnels,
    user_msg_cache: UserMessageCache,
}

impl Node {
    pub fn first(cache: Box<Cache>,
                 crust_service: Service,
                 event_sender: Sender<Event>,
                 mut full_id: FullId,
                 timer: Timer)
                 -> Option<Self> {
        let name = XorName(sha256::hash(&full_id.public_id().name().0).0);
        full_id.public_id_mut().set_name(name);

        Self::new(cache,
                  crust_service,
                  event_sender,
                  true,
                  full_id,
                  Default::default(),
                  timer)
    }

    #[cfg_attr(feature = "clippy", allow(too_many_arguments))]
    pub fn from_bootstrapping(cache: Box<Cache>,
                              crust_service: Service,
                              event_sender: Sender<Event>,
                              full_id: FullId,
                              proxy_peer_id: PeerId,
                              proxy_public_id: PublicId,
                              quorum_size: usize,
                              stats: Stats,
                              timer: Timer)
                              -> Option<Self> {
        timer.stop();

        let mut node = Self::new(cache,
                                 crust_service,
                                 event_sender,
                                 false,
                                 full_id,
                                 stats,
                                 timer);

        if let Some(ref mut node) = node {
            node.msg_accumulator.set_quorum_size(quorum_size);
            let _ = node.peer_mgr.set_proxy(proxy_peer_id, proxy_public_id);
        }

        node
    }

    #[cfg_attr(feature = "clippy", allow(too_many_arguments))]
    fn new(cache: Box<Cache>,
           crust_service: Service,
           event_sender: Sender<Event>,
           first_node: bool,
           full_id: FullId,
           stats: Stats,
           mut timer: Timer)
           -> Option<Self> {
        let public_id = *full_id.public_id();
        let tick_period = Duration::from_secs(TICK_TIMEOUT_SECS);
        let tick_timer_token = timer.schedule(tick_period);
        let user_msg_cache_duration = Duration::from_secs(USER_MSG_CACHE_EXPIRY_DURATION_SECS);

        let mut node = Node {
            ack_mgr: AckManager::new(),
            bucket_filter: MessageFilter::with_expiry_duration(Duration::from_secs(60)),
            bucket_refresh_token_and_delay: None,
            cacheable_user_msg_cache:
                UserMessageCache::with_expiry_duration(user_msg_cache_duration),
            crust_service: crust_service,
            event_sender: event_sender.clone(),
            full_id: full_id,
            get_node_name_timer_token: None,
            is_first_node: first_node,
            msg_accumulator: MessageAccumulator::new(),
            peer_mgr: PeerManager::new(public_id),
            response_cache: cache,
            signed_msg_filter: SignedMessageFilter::new(),
            sent_network_name_to: None,
            stats: stats,
            tick_timer_token: tick_timer_token,
            timer: timer,
            tunnels: Default::default(),
            user_msg_cache: UserMessageCache::with_expiry_duration(user_msg_cache_duration),
        };

        if node.start_listening() {
            debug!("{:?} - State changed to node.", node);
            Some(node)
        } else {
            node.send_event(Event::Terminate);
            None
        }
    }

    /// Routing table of this node.
    #[cfg(feature = "use-mock-crust")]
    pub fn routing_table(&self) -> &RoutingTable<XorName> {
        self.peer_mgr.routing_table()
    }

    /// Resends all unacknowledged messages.
    #[cfg(feature = "use-mock-crust")]
    pub fn resend_unacknowledged(&mut self) -> bool {
        self.timer.stop();
        let timer_tokens = self.ack_mgr.timer_tokens();
        for timer_token in &timer_tokens {
            self.resend_unacknowledged_timed_out_msgs(*timer_token);
        }
        !timer_tokens.is_empty()
    }

    /// Are there any unacknowledged messages?
    #[cfg(feature = "use-mock-crust")]
    pub fn has_unacknowledged(&self) -> bool {
        self.ack_mgr.has_pending()
    }

    #[cfg(feature = "use-mock-crust")]
    pub fn clear_state(&mut self) {
        self.ack_mgr.clear();
        self.bucket_filter.clear();
        self.msg_accumulator.clear();
        self.peer_mgr.clear_caches();
        self.signed_msg_filter.clear();
        self.sent_network_name_to = None;
    }

    fn update_stats(&mut self) {
        let old_client_num = self.stats.cur_client_num;
        self.stats.cur_client_num = self.peer_mgr.client_num();
        if self.stats.cur_client_num != old_client_num {
            if self.stats.cur_client_num > old_client_num {
                self.stats.cumulative_client_num += self.stats.cur_client_num - old_client_num;
            }
            info!("{:?} - Connected clients: {}, cumulative: {}",
                  self,
                  self.stats.cur_client_num,
                  self.stats.cumulative_client_num);
        }
        if self.stats.tunnel_connections != self.tunnels.tunnel_count() ||
           self.stats.tunnel_client_pairs != self.tunnels.client_count() {
            self.stats.tunnel_connections = self.tunnels.tunnel_count();
            self.stats.tunnel_client_pairs = self.tunnels.client_count();
            info!("{:?} - Indirect connections: {}, tunneling for: {}",
                  self,
                  self.stats.tunnel_connections,
                  self.stats.tunnel_client_pairs);
        }

        if self.stats.cur_routing_table_size != self.peer_mgr.routing_table().len() {
            self.stats.cur_routing_table_size = self.peer_mgr.routing_table().len();

            let status_str = format!("{:?} {:?} - Routing Table size: {:3}",
                                     self,
                                     self.crust_service.id(),
                                     self.stats.cur_routing_table_size);
            info!(" -{}- ",
                  iter::repeat('-').take(status_str.len()).collect::<String>());
            info!("| {} |", status_str); // Temporarily error for ci_test.
            info!(" -{}- ",
                  iter::repeat('-').take(status_str.len()).collect::<String>());
        }
    }

    pub fn handle_action(&mut self, action: Action) -> Transition {
        match action {
            Action::ClientSendRequest { result_tx, .. } => {
                let _ = result_tx.send(Err(InterfaceError::InvalidState));
            }
            Action::NodeSendMessage { src, dst, content, priority, result_tx } => {
                let result = match self.send_user_message(src, dst, content, priority) {
                    Err(RoutingError::Interface(err)) => Err(err),
                    Err(_) | Ok(_) => Ok(()),
                };

                let _ = result_tx.send(result);
            }
            Action::CloseGroup { name, result_tx } => {
                let _ =
                    result_tx.send(self.peer_mgr.routing_table().close_nodes(&name, GROUP_SIZE));
            }
            Action::Name { result_tx } => {
                let _ = result_tx.send(*self.name());
            }
            Action::QuorumSize { result_tx } => {
                let _ = result_tx.send(self.dynamic_quorum_size());
            }
            Action::Timeout(token) => {
                if !self.handle_timeout(token) {
                    return Transition::Terminate;
                }
            }
            Action::Terminate => {
                return Transition::Terminate;
            }
        }

        self.update_stats();
        Transition::Stay
    }

    pub fn handle_crust_event(&mut self, crust_event: CrustEvent) -> Transition {
        match crust_event {
            CrustEvent::BootstrapAccept(peer_id) => self.handle_bootstrap_accept(peer_id),
            CrustEvent::BootstrapConnect(peer_id, _) => self.handle_bootstrap_connect(peer_id),
            CrustEvent::ConnectSuccess(peer_id) => self.handle_connect_success(peer_id),
            CrustEvent::ConnectFailure(peer_id) => self.handle_connect_failure(peer_id),
            CrustEvent::LostPeer(peer_id) => {
                if let Transition::Terminate = self.handle_lost_peer(peer_id) {
                    return Transition::Terminate;
                }
            }
            CrustEvent::NewMessage(peer_id, bytes) => {
                match self.handle_new_message(peer_id, bytes) {
                    Err(RoutingError::FilterCheckFailed) |
                    Ok(_) => (),
                    Err(err) => debug!("{:?} - {:?}", self, err),
                }
            }
            CrustEvent::ConnectionInfoPrepared(ConnectionInfoResult { result_token, result }) => {
                self.handle_connection_info_prepared(result_token, result)
            }
            CrustEvent::ListenerStarted(port) => {
                if let Transition::Terminate = self.handle_listener_started(port) {
                    return Transition::Terminate;
                }
            }
            CrustEvent::ListenerFailed => {
                error!("{:?} Failed to start listening.", self);
                self.send_event(Event::Terminate);
                return Transition::Terminate;
            }
            CrustEvent::WriteMsgSizeProhibitive(peer_id, msg) => {
                error!("{:?} Failed to send {}-byte message to {:?}. Message too large.",
                       self,
                       msg.len(),
                       peer_id);
            }
            _ => {
                debug!("{:?} - Unhandled crust event: {:?}", self, crust_event);
            }
        }

        self.update_stats();
        Transition::Stay
    }

    fn handle_listener_started(&mut self, port: u16) -> Transition {
        trace!("{:?} Listener started on port {}.", self, port);
        self.crust_service.set_service_discovery_listen(true);

        if self.is_first_node {
            info!("{:?} - Started a new network as a seed node.", self);
            Transition::Stay
        } else if let Err(error) = self.relocate() {
            error!("{:?} Failed to start relocation: {:?}", self, error);
            self.send_event(Event::RestartRequired);
            Transition::Terminate
        } else {
            Transition::Stay
        }
    }

    fn handle_bootstrap_accept(&mut self, peer_id: PeerId) {
        trace!("{:?} Received BootstrapAccept from {:?}.", self, peer_id);
        // TODO: Keep track of that peer to make sure we receive a message from them.
    }

    fn handle_bootstrap_connect(&mut self, peer_id: PeerId) {
        self.disconnect_peer(&peer_id)
    }

    fn handle_connect_success(&mut self, peer_id: PeerId) {
        if peer_id == self.crust_service.id() {
            debug!("{:?} Received ConnectSuccess event with our Crust peer ID.",
                   self);
            return;
        }

        // TODO(afck): Keep track of this connection: Disconnect if we don't receive a
        // NodeIdentify.

        // Remove tunnel connection if we have one for this peer already
        if let Some(tunnel_id) = self.tunnels.remove_tunnel_for(&peer_id) {
            debug!("{:?} Removing unwanted tunnel for {:?}", self, peer_id);
            let message = DirectMessage::TunnelDisconnect(peer_id);
            let _ = self.send_direct_message(&tunnel_id, message);
        } else if let Some(pub_id) = self.peer_mgr.get_routing_peer(&peer_id) {
            warn!("{:?} Received ConnectSuccess from {:?}, but node {:?} is already in our \
                   routing table.",
                  self,
                  peer_id,
                  pub_id.name());
            return;
        }
        // TODO: Keep track of this peer, even if this returns false.
        self.peer_mgr.connected_to(peer_id);
        debug!("{:?} Received ConnectSuccess from {:?}. Sending NodeIdentify.",
               self,
               peer_id);
        if let Err(error) = self.send_node_identify(peer_id) {
            warn!("{:?} Failed to send NodeIdentify to {:?}: {:?}",
                  self,
                  peer_id,
                  error);
            self.disconnect_peer(&peer_id);
        }
    }

    fn handle_connect_failure(&mut self, peer_id: PeerId) {
        if peer_id == self.crust_service.id() {
            debug!("{:?} Received ConnectFailure event with our Crust peer ID.",
                   self);
            return;
        }

        if let Some(&pub_id) = self.peer_mgr.get_connecting_peer(&peer_id) {
            info!("{:?} Failed to connect to peer {:?} with pub_id {:?}.",
                  self,
                  peer_id,
                  pub_id);
            self.find_tunnel_for_peer(peer_id, &pub_id);
        }
    }

    fn find_tunnel_for_peer(&mut self, peer_id: PeerId, pub_id: &PublicId) {
        for (name, dst_peer_id) in self.peer_mgr.set_searching_for_tunnel(peer_id, pub_id) {
            trace!("{:?} Asking {:?} to serve as a tunnel.", self, name);
            let tunnel_request = DirectMessage::TunnelRequest(peer_id);
            let _ = self.send_direct_message(&dst_peer_id, tunnel_request);
        }
    }

    fn handle_new_message(&mut self, peer_id: PeerId, bytes: Vec<u8>) -> Result<(), RoutingError> {
        match serialisation::deserialise(&bytes) {
            Ok(Message::Hop(hop_msg)) => self.handle_hop_message(hop_msg, peer_id),
            Ok(Message::Direct(direct_msg)) => self.handle_direct_message(direct_msg, peer_id),
            Ok(Message::TunnelDirect { content, src, dst }) => {
                if dst == self.crust_service.id() &&
                   self.tunnels.tunnel_for(&src) == Some(&peer_id) {
                    self.handle_direct_message(content, src)
                } else if self.tunnels.has_clients(src, dst) {
                    self.send_or_drop(&dst, bytes, content.priority())
                } else if self.tunnels.accept_clients(src, dst) {
                    try!(self.send_direct_message(&dst, DirectMessage::TunnelSuccess(src)));
                    self.send_or_drop(&dst, bytes, content.priority())
                } else {
                    Err(RoutingError::InvalidDestination)
                }
            }
            Ok(Message::TunnelHop { content, src, dst }) => {
                if dst == self.crust_service.id() &&
                   self.tunnels.tunnel_for(&src) == Some(&peer_id) {
                    self.handle_hop_message(content, src)
                } else if self.tunnels.has_clients(src, dst) {
                    self.send_or_drop(&dst, bytes, content.content().priority())
                } else {
                    Err(RoutingError::InvalidDestination)
                }
            }
            Err(error) => Err(RoutingError::SerialisationError(error)),
        }
    }

    fn handle_direct_message(&mut self,
                             direct_message: DirectMessage,
                             peer_id: PeerId)
                             -> Result<(), RoutingError> {
        match direct_message {
            DirectMessage::ClientIdentify { ref serialised_public_id,
                                            ref signature,
                                            client_restriction } => {
                if let Ok(public_id) = verify_signed_public_id(serialised_public_id, signature) {
                    self.handle_client_identify(public_id, peer_id, client_restriction)
                } else {
                    warn!("{:?} Signature check failed in ClientIdentify - \
                            Dropping connection {:?}",
                          self,
                          peer_id);
                    self.disconnect_peer(&peer_id);
                    Ok(())
                }
            }
            DirectMessage::NodeIdentify { ref serialised_public_id, ref signature } => {
                if let Ok(public_id) = verify_signed_public_id(serialised_public_id, signature) {
                    self.handle_node_identify(public_id, peer_id);
                } else {
                    warn!("{:?} Signature check failed in NodeIdentify - Dropping peer {:?}",
                          self,
                          peer_id);
                    self.disconnect_peer(&peer_id);
                }
                Ok(())
            }
            DirectMessage::NewNode(public_id) => {
                trace!("{:?} Received NewNode({:?}).", self, public_id);
                if self.peer_mgr.routing_table().need_to_add(public_id.name()) {
                    let our_name = *self.name();
                    return self.send_connection_info(public_id,
                                              Authority::ManagedNode(our_name),
                                              Authority::ManagedNode(*public_id.name()))
                        .map(|_| ());
                }
                Ok(())
            }
            DirectMessage::ConnectionUnneeded(ref name) => {
                if self.peer_mgr.remove_if_unneeded(name, &peer_id) {
                    info!("{:?} Dropped {:?} from the routing table.", self, name);
                    self.crust_service.disconnect(peer_id);
                    let _ = self.handle_lost_peer(peer_id);
                }
                Ok(())
            }
            DirectMessage::TunnelRequest(dst_id) => self.handle_tunnel_request(peer_id, dst_id),
            DirectMessage::TunnelSuccess(dst_id) => self.handle_tunnel_success(peer_id, dst_id),
            DirectMessage::TunnelClosed(dst_id) => self.handle_tunnel_closed(peer_id, dst_id),
            DirectMessage::TunnelDisconnect(dst_id) => {
                self.handle_tunnel_disconnect(peer_id, dst_id)
            }
            _ => {
                debug!("{:?} - Unhandled direct message: {:?}",
                       self,
                       direct_message);
                Ok(())
            }
        }
    }

    fn handle_hop_message(&mut self,
                          hop_msg: HopMessage,
                          peer_id: PeerId)
                          -> Result<(), RoutingError> {
        let hop_name;
        if self.is_proper() {
            if let Some(public_id) = self.peer_mgr.get_routing_peer(&peer_id) {
                try!(hop_msg.verify(public_id.signing_public_key()));
                hop_name = *public_id.name();
            } else if let Some(client_info) = self.peer_mgr.get_client(&peer_id) {
                try!(hop_msg.verify(&client_info.public_key));
                if client_info.client_restriction {
                    try!(self.check_valid_client_message(hop_msg.content().routing_message()));
                }
                hop_name = *self.name();
            } else if let Some(pub_id) = self.peer_mgr.get_proxy_public_id(&peer_id) {
                try!(hop_msg.verify(pub_id.signing_public_key()));
                hop_name = *pub_id.name();
            } else {
                // TODO: Drop peer?
                // debug!("Received hop message from unknown name {:?}. Dropping peer {:?}.",
                //        hop_msg.name(),
                //        peer_id);
                // self.disconnect_peer(&peer_id);
                return Err(RoutingError::UnknownConnection(peer_id));
            }
        } else if let Some(pub_id) = self.peer_mgr.get_proxy_public_id(&peer_id) {
            try!(hop_msg.verify(pub_id.signing_public_key()));
            hop_name = *pub_id.name();
        } else {
            return Err(RoutingError::UnknownConnection(peer_id));
        }

        self.handle_signed_message(hop_msg.content(),
                                   hop_msg.route(),
                                   &hop_name,
                                   hop_msg.sent_to())
    }

    fn handle_signed_message(&mut self,
                             signed_msg: &SignedMessage,
                             route: u8,
                             hop_name: &XorName,
                             sent_to: &[XorName])
                             -> Result<(), RoutingError> {
        try!(signed_msg.check_integrity());
        let routing_msg = signed_msg.routing_message();

        // FIXME: This is currently only in place so acks can get delivered if the
        // original ack was lost in transit
        if (self.msg_accumulator.contains(routing_msg) || !routing_msg.src.is_group()) &&
           self.is_recipient(&routing_msg.dst) {
            self.send_ack(routing_msg, route);
        }

        let count = self.signed_msg_filter.filter_incoming(signed_msg);

        // Prevents
        // 1) someone sending messages repeatedly to us
        // 2) swarm messages generated by us reaching us again
        if count > GROUP_SIZE {
            return Err(RoutingError::FilterCheckFailed);
        }

        if self.is_recipient(&routing_msg.dst) {
            // TODO: If group, verify the sender's membership.
            if let Authority::Client { ref client_key, .. } = signed_msg.routing_message().src {
                if client_key != signed_msg.public_id().signing_public_key() {
                    return Err(RoutingError::FailedSignature);
                };
            }
        } else if try!(self.respond_from_cache(&routing_msg, route)) {
            return Ok(());
        }

        if let Err(error) = self.send_signed_message(signed_msg, route, hop_name, sent_to) {
            debug!("{:?} Failed to send {:?}: {:?}", self, signed_msg, error);
        }

        if count == 1 && self.is_recipient(&routing_msg.dst) {
            self.handle_routing_message(routing_msg, signed_msg.public_id())
        } else {
            Ok(())
        }
    }

    fn handle_routing_message(&mut self,
                              routing_msg: &RoutingMessage,
                              public_id: &PublicId)
                              -> Result<(), RoutingError> {
        if self.is_proper() {
            let dynamic_quorum_size = self.dynamic_quorum_size();
            self.msg_accumulator.set_quorum_size(dynamic_quorum_size);
        }

        if let Some(msg) = try!(self.accumulate(routing_msg, public_id)) {
            if msg.src.is_group() {
                self.send_ack(&msg, 0);
            }

            self.dispatch_routing_message(msg)
        } else {
            Ok(())
        }
    }

    fn dispatch_routing_message(&mut self,
                                routing_msg: RoutingMessage)
                                -> Result<(), RoutingError> {
        let msg_content = routing_msg.content.clone();
        let msg_src = routing_msg.src.clone();
        let msg_dst = routing_msg.dst.clone();
        match msg_content {
            MessageContent::Ack(..) => (),
            _ => {
                trace!("{:?} Got routing message {:?} from {:?} to {:?}.",
                       self,
                       msg_content,
                       msg_src,
                       msg_dst)
            }
        }

        match (msg_content, msg_src, msg_dst) {
            (MessageContent::GetNodeName { current_id, message_id },
             Authority::Client { client_key, proxy_node_name, peer_id },
             Authority::NaeManager(dst_name)) => {
                self.handle_get_node_name_request(current_id,
                                                  client_key,
                                                  proxy_node_name,
                                                  dst_name,
                                                  peer_id,
                                                  message_id)
            }
            (MessageContent::GetNodeNameResponse { relocated_id, close_group_ids, .. },
             Authority::NodeManager(_),
             dst) => {
                self.handle_get_node_name_response(relocated_id, close_group_ids, dst);
                Ok(())
            }
            (MessageContent::ExpectCloseNode { expect_id, client_auth, message_id },
             Authority::NaeManager(_),
             Authority::NaeManager(_)) => {
                self.handle_expect_close_node_request(expect_id, client_auth, message_id)
            }
            (MessageContent::GetCloseGroup(message_id), src, Authority::NaeManager(dst_name)) => {
                self.handle_get_close_group_request(src, dst_name, message_id)
            }
            (MessageContent::ConnectionInfo { encrypted_connection_info, nonce_bytes, public_id },
             src @ Authority::Client { .. },
             Authority::ManagedNode(dst_name)) => {
                self.handle_connection_info_from_client(encrypted_connection_info,
                                                        nonce_bytes,
                                                        src,
                                                        dst_name,
                                                        public_id)
            }
            (MessageContent::ConnectionInfo { encrypted_connection_info, nonce_bytes, public_id },
             Authority::ManagedNode(src_name),
             Authority::Client { .. }) |
            (MessageContent::ConnectionInfo { encrypted_connection_info, nonce_bytes, public_id },
             Authority::ManagedNode(src_name),
             Authority::ManagedNode(_)) => {
                self.handle_connection_info_from_node(encrypted_connection_info,
                                                      nonce_bytes,
                                                      src_name,
                                                      routing_msg.dst.clone(),
                                                      public_id)
            }
            (MessageContent::GetCloseGroupResponse { close_group_ids, .. },
             Authority::ManagedNode(_),
             dst) => self.handle_get_close_group_response(close_group_ids, dst),
            (MessageContent::Ack(ack, _), _, _) => self.handle_ack_response(ack),
            (MessageContent::UserMessagePart { hash, part_count, part_index, payload, .. },
             src,
             dst) => {
                self.handle_user_message_part(hash, part_count, part_index, payload, src, dst);
                Ok(())
            }
            _ => {
                debug!("{:?} Unhandled routing message {:?}", self, routing_msg);
                Err(RoutingError::BadAuthority)
            }
        }
    }

    /// Returns `Ok` if a client is allowed to send the given message.
    fn check_valid_client_message(&self, msg: &RoutingMessage) -> Result<(), RoutingError> {
        match msg.content {
            MessageContent::Ack(..) => Ok(()),
            MessageContent::UserMessagePart { priority, .. } if priority >= DEFAULT_PRIORITY => {
                Ok(())
            }
            _ => {
                debug!("{:?} Illegitimate client message {:?}. Refusing to relay.",
                       self,
                       msg);
                Err(RoutingError::RejectedClientMessage)
            }
        }
    }

    fn respond_from_cache(&mut self,
                          routing_msg: &RoutingMessage,
                          route: u8)
                          -> Result<bool, RoutingError> {
        if let MessageContent::UserMessagePart { hash,
                                                 part_count,
                                                 part_index,
                                                 cacheable,
                                                 ref payload,
                                                 .. } = routing_msg.content {
            if !cacheable {
                return Ok(false);
            }

            match self.cacheable_user_msg_cache.add(hash, part_count, part_index, payload.clone()) {
                Some(UserMessage::Request(request)) => {
                    if let Some(response) = self.response_cache.get(&request) {
                        debug!("{:?} Found cached response to {:?}", self, request);

                        let priority = response.priority();
                        let src = Authority::ManagedNode(*self.name());
                        let dst = routing_msg.src.clone();

                        self.send_ack_from(routing_msg, route, src.clone());

                        try!(self.send_user_message(src,
                                                    dst,
                                                    UserMessage::Response(response),
                                                    priority));

                        return Ok(true);
                    }
                }

                Some(UserMessage::Response(response)) => {
                    debug!("{:?} putting {:?} to cache", self, response);
                    self.response_cache.put(response);
                }

                None => (),
            }
        }

        Ok(false)
    }

    fn dynamic_quorum_size(&self) -> usize {
        // Routing table entries plus this node itself.
        let network_size = self.peer_mgr.routing_table().len() + 1;
        if network_size >= GROUP_SIZE {
            QUORUM_SIZE
        } else {
            cmp::max(network_size * QUORUM_SIZE / GROUP_SIZE,
                     network_size / 2 + 1)
        }
    }

    fn start_listening(&mut self) -> bool {
        if let Err(error) = self.crust_service.start_listening_tcp() {
            error!("{:?} Failed to start listening: {:?}", self, error);
            false
        } else {
            info!("{:?} Attempting to start listener.", self);
            true
        }
    }

    fn relocate(&mut self) -> Result<(), RoutingError> {
        let duration = Duration::from_secs(GET_NODE_NAME_TIMEOUT_SECS);
        self.get_node_name_timer_token = Some(self.timer.schedule(duration));

        let request_content = MessageContent::GetNodeName {
            current_id: *self.full_id.public_id(),
            message_id: MessageId::new(),
        };

        let proxy_name = if let Some((_, _, ref proxy_public_id)) = *self.peer_mgr.proxy() {
            *proxy_public_id.name()
        } else {
            return Err(RoutingError::ProxyConnectionNotFound);
        };

        let src = Authority::Client {
            client_key: *self.full_id.public_id().signing_public_key(),
            proxy_node_name: proxy_name,
            peer_id: self.crust_service.id(),
        };

        let request_msg = RoutingMessage {
            src: src,
            dst: Authority::NaeManager(*self.name()),
            content: request_content,
        };

        info!("{:?} Sending GetNodeName request with: {:?}. This can take a while.",
              self,
              self.full_id.public_id());

        self.send_routing_message(request_msg)
    }

    fn send_bootstrap_identify(&mut self, peer_id: PeerId) -> Result<(), RoutingError> {
        let direct_message = DirectMessage::BootstrapIdentify {
            public_id: *self.full_id.public_id(),
            current_quorum_size: self.dynamic_quorum_size(),
        };
        self.send_direct_message(&peer_id, direct_message)
    }

    fn handle_client_identify(&mut self,
                              public_id: PublicId,
                              peer_id: PeerId,
                              client_restriction: bool)
                              -> Result<(), RoutingError> {
        if *public_id.name() != XorName(sha256::hash(&public_id.signing_public_key().0).0) {
            warn!("{:?} Incoming Connection not validated as a proper client - dropping",
                  self);
            self.disconnect_peer(&peer_id);
            return Ok(());
        }

        for peer_id in self.peer_mgr.remove_stale_joining_nodes() {
            debug!("{:?} Removing stale joining node with Crust ID {:?}",
                   self,
                   peer_id);
            self.disconnect_peer(&peer_id);
        }

        if (client_restriction || !self.is_first_node) &&
           self.peer_mgr.routing_table().len() < GROUP_SIZE - 1 {
            debug!("{:?} Client {:?} rejected: Routing table has {} entries. {} required.",
                   self,
                   public_id.name(),
                   self.peer_mgr.routing_table().len(),
                   GROUP_SIZE - 1);
            return self.send_direct_message(&peer_id, DirectMessage::BootstrapDeny);
        }
        if self.peer_mgr.get_client(&peer_id).is_some() {
            debug!("{:?} Received two ClientInfo from the same peer ID {:?}.",
                   self,
                   peer_id);
        }
        self.peer_mgr.insert_client(peer_id, &public_id, client_restriction);

        debug!("{:?} Accepted client {:?}.", self, public_id.name());

        self.send_bootstrap_identify(peer_id)
    }

    fn handle_node_identify(&mut self, public_id: PublicId, peer_id: PeerId) {
        debug!("{:?} Handling NodeIdentify from {:?}.",
               self,
               public_id.name());

        if let Some((name, _)) = self.sent_network_name_to {
            if name == *public_id.name() {
                self.sent_network_name_to = None;
            }
        }

        self.add_to_routing_table(public_id, peer_id);
    }

    fn add_to_routing_table(&mut self, public_id: PublicId, peer_id: PeerId) {
        let name = *public_id.name();
        if self.peer_mgr.routing_table().contains(&name) {
            return; // We already sent a `NodeIdentify` to this peer.
        }

        let bucket_index = self.name().bucket_index(&name);
        let common_groups = self.peer_mgr
            .routing_table()
            .is_in_any_close_group_with(bucket_index, GROUP_SIZE);

        match self.peer_mgr.add_to_routing_table(public_id, peer_id) {
            None => {
                debug!("{:?} Peer was not added to the routing table: {:?}",
                       self,
                       peer_id);
                self.disconnect_peer(&peer_id);
            }
            Some(AddedNodeDetails { must_notify, unneeded }) => {
                info!("{:?} Added {:?} to routing table.", self, name);
                if self.peer_mgr.routing_table().len() == 1 {
                    self.send_event(Event::Connected);
                }
                for peer_id in self.peer_mgr.get_peer_ids(&must_notify) {
                    let message = DirectMessage::NewNode(public_id);
                    let _ = self.send_direct_message(&peer_id, message);
                }
                for peer_id in self.peer_mgr.get_peer_ids(&unneeded) {
                    let message = DirectMessage::ConnectionUnneeded(*self.name());
                    let _ = self.send_direct_message(&peer_id, message);
                }

                self.reset_bucket_refresh_timer();

                if common_groups {
                    let event = Event::NodeAdded(name, self.peer_mgr.routing_table().clone());
                    if let Err(err) = self.event_sender.send(event) {
                        error!("{:?} Error sending event to routing user - {:?}", self, err);
                    }
                }
            }
        }

        if self.peer_mgr.routing_table().len() == 1 {
            self.request_bucket_close_groups();
        }

        for dst_id in self.peer_mgr.peers_needing_tunnel() {
            let tunnel_request = DirectMessage::TunnelRequest(dst_id);
            let _ = self.send_direct_message(&peer_id, tunnel_request);
        }
    }

    fn reset_bucket_refresh_timer(&mut self) {
        if let Some((_, REFRESH_BUCKET_GROUPS_SECS)) = self.bucket_refresh_token_and_delay {
            return; // Timer has already been reset.
        }
        let new_token = self.timer.schedule(Duration::from_secs(REFRESH_BUCKET_GROUPS_SECS));
        self.bucket_refresh_token_and_delay = Some((new_token, REFRESH_BUCKET_GROUPS_SECS));
    }

    /// Sends a `GetCloseGroup` request to the close group with our `bucket_index`-th bucket
    /// address.
    fn request_bucket_ids(&mut self, bucket_index: usize) -> Result<(), RoutingError> {
        if bucket_index >= XOR_NAME_BITS {
            return Ok(());
        }
        trace!("{:?} Send GetCloseGroup to bucket {}.", self, bucket_index);
        let bucket_address = self.name().with_flipped_bit(bucket_index);
        self.request_close_group(bucket_address)
    }

    fn request_close_group(&mut self, name: XorName) -> Result<(), RoutingError> {
        let request_msg = RoutingMessage {
            src: Authority::ManagedNode(*self.name()),
            dst: Authority::NaeManager(name),
            content: MessageContent::GetCloseGroup(MessageId::new()),
        };

        self.send_routing_message(request_msg)
    }

    fn handle_connection_info_prepared(&mut self,
                                       result_token: u32,
                                       result: Result<PrivConnectionInfo, CrustError>) {
        let our_connection_info = match result {
            Err(err) => {
                error!("{:?} Failed to prepare connection info: {:?}", self, err);
                return;
            }
            Ok(connection_info) => connection_info,
        };
        let encoded_connection_info =
            match serialisation::serialise(&our_connection_info.to_pub_connection_info()) {
                Err(err) => {
                    error!("{:?} Failed to serialise connection info: {:?}", self, err);
                    return;
                }
                Ok(encoded_connection_info) => encoded_connection_info,
            };

        let (pub_id, src, dst) = match self.peer_mgr
            .connection_info_prepared(result_token, our_connection_info) {
            Err(error) => {
                // This usually means we have already connected.
                debug!("{:?} Prepared connection info, but no entry found in token map: {:?}",
                       self,
                       error);
                return;
            }
            Ok(ConnectionInfoPreparedResult { pub_id, src, dst, infos }) => {
                match infos {
                    None => {
                        debug!("{:?} Prepared connection info for {:?}.",
                               self,
                               pub_id.name());
                    }
                    Some((our_info, their_info)) => {
                        debug!("{:?} Trying to connect to {:?} as {:?}.",
                               self,
                               their_info.id(),
                               pub_id.name());
                        let _ = self.crust_service.connect(our_info, their_info);
                    }
                }
                (pub_id, src, dst)
            }
        };

        let nonce = box_::gen_nonce();
        let encrypted_connection_info = box_::seal(&encoded_connection_info,
                                                   &nonce,
                                                   pub_id.encrypting_public_key(),
                                                   self.full_id().encrypting_private_key());

        let request_content = MessageContent::ConnectionInfo {
            encrypted_connection_info: encrypted_connection_info,
            nonce_bytes: nonce.0,
            public_id: *self.full_id().public_id(),
        };

        let request_msg = RoutingMessage {
            src: src,
            dst: dst,
            content: request_content,
        };

        if let Err(err) = self.send_routing_message(request_msg) {
            debug!("{:?} Failed to send connection info for {:?}: {:?}.",
                   self,
                   pub_id.name(),
                   err);
        }
    }

    // TODO: check whether these two methods can be merged into one.
    fn handle_connection_info_from_client(&mut self,
                                          encrypted_connection_info: Vec<u8>,
                                          nonce_bytes: [u8; box_::NONCEBYTES],
                                          src: Authority,
                                          dst_name: XorName,
                                          their_public_id: PublicId)
                                          -> Result<(), RoutingError> {
        try!(self.check_address_for_routing_table(their_public_id.name()));
        self.connect(encrypted_connection_info,
                     nonce_bytes,
                     their_public_id,
                     Authority::ManagedNode(dst_name),
                     src)
    }

    fn handle_connection_info_from_node(&mut self,
                                        encrypted_connection_info: Vec<u8>,
                                        nonce_bytes: [u8; box_::NONCEBYTES],
                                        src_name: XorName,
                                        dst: Authority,
                                        their_public_id: PublicId)
                                        -> Result<(), RoutingError> {
        try!(self.check_address_for_routing_table(&src_name));
        self.connect(encrypted_connection_info,
                     nonce_bytes,
                     their_public_id,
                     dst,
                     Authority::ManagedNode(src_name))
    }

    /// Handle a request by `peer_id` to act as a tunnel connecting it with `dst_id`.
    fn handle_tunnel_request(&mut self,
                             peer_id: PeerId,
                             dst_id: PeerId)
                             -> Result<(), RoutingError> {
        if self.peer_mgr.can_tunnel_for(&peer_id, &dst_id) {
            if let Some((id0, id1)) = self.tunnels.consider_clients(peer_id, dst_id) {
                debug!("{:?} Accepted tunnel request from {:?} for {:?}.",
                       self,
                       peer_id,
                       dst_id);
                return self.send_direct_message(&id0, DirectMessage::TunnelSuccess(id1));
            }
        } else {
            debug!("{:?} Rejected tunnel request from {:?} for {:?}.",
                   self,
                   peer_id,
                   dst_id);
        }
        Ok(())
    }

    /// Handle a `TunnelSuccess` response from `peer_id`: It will act as a tunnel to `dst_id`.
    fn handle_tunnel_success(&mut self,
                             peer_id: PeerId,
                             dst_id: PeerId)
                             -> Result<(), RoutingError> {
        if self.peer_mgr.tunnelling_to(dst_id) && self.tunnels.add(dst_id, peer_id) {
            debug!("{:?} Adding {:?} as a tunnel node for {:?}.",
                   self,
                   peer_id,
                   dst_id);
            return self.send_node_identify(dst_id);
        }
        Ok(())
    }

    /// Handle a `TunnelClosed` message from `peer_id`: `dst_id` disconnected.
    fn handle_tunnel_closed(&mut self,
                            peer_id: PeerId,
                            dst_id: PeerId)
                            -> Result<(), RoutingError> {
        if self.tunnels.remove(dst_id, peer_id) {
            debug!("{:?} Tunnel to {:?} via {:?} closed.",
                   self,
                   dst_id,
                   peer_id);
            if !self.crust_service.is_connected(&dst_id) {
                self.dropped_routing_node_connection(&dst_id);
            }
        }
        Ok(())
    }

    /// Handle a `TunnelDisconnect` message from `peer_id` who wants to disconnect `dst_id`.
    fn handle_tunnel_disconnect(&mut self,
                                peer_id: PeerId,
                                dst_id: PeerId)
                                -> Result<(), RoutingError> {
        debug!("{:?} Closing tunnel connecting {:?} and {:?}.",
               self,
               dst_id,
               peer_id);
        if self.tunnels.drop_client_pair(dst_id, peer_id) {
            self.send_direct_message(&dst_id, DirectMessage::TunnelClosed(peer_id))
        } else {
            Ok(())
        }
    }

    /// Disconnects from the given peer, via Crust or by dropping the tunnel node, if the peer is
    /// not a proxy, client or routing table entry.
    fn disconnect_peer(&mut self, peer_id: &PeerId) {
        if let Some(&pub_id) = self.peer_mgr.get_routing_peer(peer_id) {
            debug!("{:?} Not disconnecting routing table entry {:?} ({:?}).",
                   self,
                   pub_id.name(),
                   peer_id);
        } else if let Some(&public_id) = self.peer_mgr.get_proxy_public_id(peer_id) {
            debug!("{:?} Not disconnecting proxy node {:?} ({:?}).",
                   self,
                   public_id.name(),
                   peer_id);
        } else if self.peer_mgr.get_client(peer_id).is_some() {
            debug!("{:?} Not disconnecting client {:?}.", self, peer_id);
        } else if let Some(tunnel_id) = self.tunnels.remove_tunnel_for(peer_id) {
            debug!("{:?} Disconnecting {:?} (indirect).", self, peer_id);
            let message = DirectMessage::TunnelDisconnect(*peer_id);
            let _ = self.send_direct_message(&tunnel_id, message);
        } else {
            debug!("{:?} Disconnecting {:?}. Calling crust::Service::disconnect.",
                   self,
                   peer_id);
            let _ = self.crust_service.disconnect(*peer_id);
            let _ = self.peer_mgr.remove_peer(peer_id);
        }
    }


    // Received by X; From A -> X
    fn handle_get_node_name_request(&mut self,
                                    mut their_public_id: PublicId,
                                    client_key: sign::PublicKey,
                                    proxy_name: XorName,
                                    dst_name: XorName,
                                    peer_id: PeerId,
                                    message_id: MessageId)
                                    -> Result<(), RoutingError> {
        let hashed_key = sha256::hash(&client_key.0);
        let close_group_to_client = XorName(hashed_key.0);

        // Validate Client (relocating node) has contacted the correct Group-X
        if close_group_to_client != dst_name {
            return Err(RoutingError::InvalidDestination);
        }

        let close_group = match self.peer_mgr.routing_table().close_nodes(&dst_name, GROUP_SIZE) {
            Some(close_group) => close_group,
            None => return Err(RoutingError::InvalidDestination),
        };
        let relocated_name = try!(utils::calculate_relocated_name(close_group,
                                                                  &their_public_id.name()));
        their_public_id.set_name(relocated_name);

        // From X -> Y; Send to close group of the relocated name
        let request_content = MessageContent::ExpectCloseNode {
            expect_id: their_public_id,
            client_auth: Authority::Client {
                client_key: client_key,
                proxy_node_name: proxy_name,
                peer_id: peer_id,
            },
            message_id: message_id,
        };

        let request_msg = RoutingMessage {
            src: Authority::NaeManager(dst_name),
            dst: Authority::NaeManager(relocated_name),
            content: request_content,
        };

        self.send_routing_message(request_msg)
    }

    fn handle_get_node_name_response(&mut self,
                                     relocated_id: PublicId,
                                     mut close_group_ids: Vec<PublicId>,
                                     dst: Authority) {
        self.full_id.public_id_mut().set_name(*relocated_id.name());
        self.peer_mgr.reset_routing_table(*self.full_id.public_id());

        close_group_ids.truncate(GROUP_SIZE / 2);

        for close_node_id in close_group_ids {
            debug!("{:?} Sending connection info to {:?} on GetNodeName response.",
                   self,
                   close_node_id);

            if let Err(error) = self.send_connection_info(close_node_id,
                                      dst.clone(),
                                      Authority::ManagedNode(*close_node_id.name())) {
                debug!("{:?} - Failed to send connection info to {:?}: {:?}",
                       self,
                       close_node_id,
                       error);
            }
        }
    }

    // Received by Y; From X -> Y
    fn handle_expect_close_node_request(&mut self,
                                        expect_id: PublicId,
                                        client_auth: Authority,
                                        message_id: MessageId)
                                        -> Result<(), RoutingError> {
        if expect_id == *self.full_id.public_id() {
            return Ok(());
        }

        let now = Instant::now();
        if let Some((_, timestamp)) = self.sent_network_name_to {
            if (now - timestamp).as_secs() <= SENT_NETWORK_NAME_TIMEOUT_SECS {
                return Ok(()); // Not sending node name, as we are already waiting for a node.
            }
            self.sent_network_name_to = None;
        }

        let public_ids = match self.peer_mgr
            .routing_table()
            .close_nodes(expect_id.name(), GROUP_SIZE) {
            Some(close_group) => self.peer_mgr.get_pub_ids(&close_group),
            None => return Err(RoutingError::InvalidDestination),
        };

        self.sent_network_name_to = Some((*expect_id.name(), now));
        // From Y -> A (via B)
        let response_content = MessageContent::GetNodeNameResponse {
            relocated_id: expect_id,
            close_group_ids: public_ids,
            message_id: message_id,
        };

        debug!("{:?} Responding to client {:?}: {:?}.",
               self,
               client_auth,
               response_content);

        let response_msg = RoutingMessage {
            src: Authority::NodeManager(*expect_id.name()),
            dst: client_auth,
            content: response_content,
        };

        self.send_routing_message(response_msg)
    }

    // Received by Y; From A -> Y, or from any node to one of its bucket addresses.
    fn handle_get_close_group_request(&mut self,
                                      src: Authority,
                                      dst_name: XorName,
                                      message_id: MessageId)
                                      -> Result<(), RoutingError> {
        // If msg is from ourselves, ignore it.
        if src.name() == self.name() {
            return Ok(());
        }

        let public_ids = match self.peer_mgr.routing_table().close_nodes(&dst_name, GROUP_SIZE) {
            Some(close_group) => self.peer_mgr.get_pub_ids(&close_group),
            None => return Err(RoutingError::InvalidDestination),
        };

        trace!("{:?} Sending GetCloseGroup response with {:?} to {:?}.",
               self,
               public_ids.iter().map(PublicId::name).collect_vec(),
               src);
        let response_content = MessageContent::GetCloseGroupResponse {
            close_group_ids: public_ids,
            message_id: message_id,
        };

        let response_msg = RoutingMessage {
            src: Authority::ManagedNode(*self.name()),
            dst: src,
            content: response_content,
        };

        self.send_routing_message(response_msg)
    }

    fn handle_get_close_group_response(&mut self,
                                       close_group_ids: Vec<PublicId>,
                                       dst: Authority)
                                       -> Result<(), RoutingError> {
        for close_node_id in close_group_ids {
            if self.peer_mgr.routing_table().need_to_add(close_node_id.name()) {
                debug!("{:?} Sending connection info to {:?} on GetCloseGroup response.",
                       self,
                       close_node_id);
                let ci_dst = Authority::ManagedNode(*close_node_id.name());
                let _ = try!(self.send_connection_info(close_node_id, dst.clone(), ci_dst));
            }
        }
        Ok(())
    }

    fn handle_ack_response(&mut self, ack: Ack) -> Result<(), RoutingError> {
        self.ack_mgr.receive(ack);
        Ok(())
    }

    fn handle_timeout(&mut self, token: u64) -> bool {
        if self.get_node_name_timer_token == Some(token) {
            info!("{:?} Failed to get GetNodeName response.", self);
            self.send_event(Event::RestartRequired);
            return false;
        }

        if self.tick_timer_token == token {
            let _ = self.event_sender.send(Event::Tick);
            let tick_period = Duration::from_secs(TICK_TIMEOUT_SECS);
            self.tick_timer_token = self.timer.schedule(tick_period);
            return true;
        }

        if let Some((bucket_token, delay)) = self.bucket_refresh_token_and_delay {
            if bucket_token == token {
                self.request_bucket_close_groups();
                let new_delay = delay.saturating_mul(2);
                let new_token = self.timer.schedule(Duration::from_secs(new_delay));
                self.bucket_refresh_token_and_delay = Some((new_token, new_delay));
                return true;
            }
        }

        self.resend_unacknowledged_timed_out_msgs(token);

        true
    }

    /// Sends `GetCloseGroup` requests to all incompletely filled buckets and our own address.
    fn request_bucket_close_groups(&mut self) {
        if !self.bucket_filter.contains(&XOR_NAME_BITS) {
            let _ = self.bucket_filter.insert(&XOR_NAME_BITS);
            let our_name = *self.name();
            if let Err(err) = self.request_close_group(our_name) {
                error!("{:?} Failed to request our own close group: {:?}",
                       self,
                       err);
            }
        }
        for index in 0..self.peer_mgr.routing_table().bucket_count() {
            if self.peer_mgr.routing_table().bucket_len(index) < GROUP_SIZE &&
               !self.bucket_filter.contains(&index) {
                let _ = self.bucket_filter.insert(&index);
                if let Err(err) = self.request_bucket_ids(index) {
                    error!("{:?} Failed to request public IDs from bucket {}: {:?}.",
                           self,
                           index,
                           err);
                }
            }
        }
    }

    fn connect(&mut self,
               encrypted_connection_info: Vec<u8>,
               nonce_bytes: [u8; box_::NONCEBYTES],
               their_public_id: PublicId,
               src: Authority,
               dst: Authority)
               -> Result<(), RoutingError> {
        let decipher_result = box_::open(&encrypted_connection_info,
                                         &box_::Nonce(nonce_bytes),
                                         their_public_id.encrypting_public_key(),
                                         self.full_id.encrypting_private_key());

        let serialised_connection_info =
            try!(decipher_result.map_err(|()| RoutingError::AsymmetricDecryptionFailure));
        let their_connection_info: PubConnectionInfo =
            try!(serialisation::deserialise(&serialised_connection_info));
        let peer_id = their_connection_info.id();
        match self.peer_mgr
            .connection_info_received(src, dst, their_public_id, their_connection_info) {
            Ok(ConnectionInfoReceivedResult::Ready(our_info, their_info)) => {
                debug!("{:?} Received connection info. Trying to connect to {:?} ({:?}).",
                       self,
                       their_public_id.name(),
                       peer_id);
                let _ = self.crust_service.connect(our_info, their_info);
            }
            Ok(ConnectionInfoReceivedResult::Prepare(token)) => {
                self.crust_service.prepare_connection_info(token);
            }
            Ok(ConnectionInfoReceivedResult::IsProxy) |
            Ok(ConnectionInfoReceivedResult::IsClient) => {
                try!(self.send_node_identify(peer_id));
                self.handle_node_identify(their_public_id, peer_id);
            }
            Ok(ConnectionInfoReceivedResult::Waiting) |
            Ok(ConnectionInfoReceivedResult::IsConnected) => (),
            Err(error) => {
                warn!("{:?} Failed to insert connection info from {:?} ({:?}): {:?}",
                      self,
                      their_public_id.name(),
                      peer_id,
                      error)
            }
        }

        Ok(())
    }

    fn check_address_for_routing_table(&self, name: &XorName) -> Result<(), RoutingError> {
        if !self.is_proper() || self.peer_mgr.allow_connect(name) {
            Ok(())
        } else {
            Err(RoutingError::RefusedFromRoutingTable)
        }
    }

    // ----- Send Functions -----------------------------------------------------------------------
    fn send_user_message(&mut self,
                         src: Authority,
                         dst: Authority,
                         user_msg: UserMessage,
                         priority: u8)
                         -> Result<(), RoutingError> {
        match user_msg {
            UserMessage::Request(ref request) => self.stats.count_request(request),
            UserMessage::Response(ref response) => self.stats.count_response(response),
        }

        for part in try!(user_msg.to_parts(priority)) {
            try!(self.send_routing_message(RoutingMessage {
                src: src.clone(),
                dst: dst.clone(),
                content: part,
            }));
        }
        Ok(())
    }

    fn send_signed_message(&mut self,
                           signed_msg: &SignedMessage,
                           route: u8,
                           hop: &XorName,
                           sent_to: &[XorName])
                           -> Result<(), RoutingError> {
        if signed_msg.public_id() == self.full_id.public_id() && hop == self.name() {
            self.stats.count_route(route);
        }
        let routing_msg = signed_msg.routing_message();

        if let Authority::Client { ref peer_id, .. } = routing_msg.dst {
            if self.name() == routing_msg.dst.name() {
                // This is a message for a client we are the proxy of. Relay it.
                return self.relay_to_client(signed_msg.clone(), peer_id);
            } else if self.is_recipient(&routing_msg.dst) {
                return Ok(()); // Message is for us as a client.
            }
        }

        let (new_sent_to, target_peer_ids) =
            try!(self.get_targets(routing_msg, route, hop, sent_to));

        if !self.add_to_pending_acks(signed_msg, route) {
            return Ok(());
        }

        let send_msg = try!(self.message_to_send(signed_msg, route, hop));
        let raw_bytes = try!(self.to_hop_bytes(send_msg.clone(), route, new_sent_to.clone()));

        for target_peer_id in target_peer_ids {
            let (peer_id, bytes) = if self.crust_service.is_connected(&target_peer_id) {
                (target_peer_id, raw_bytes.clone())
            } else if let Some(&tunnel_id) = self.tunnels
                .tunnel_for(&target_peer_id) {
                let bytes = try!(self.to_tunnel_hop_bytes(send_msg.clone(),
                                                          route,
                                                          new_sent_to.clone(),
                                                          target_peer_id));
                (tunnel_id, bytes)
            } else {
                trace!("{:?} Not connected or tunneling to {:?}. Dropping peer.",
                       self,
                       target_peer_id);
                self.disconnect_peer(&target_peer_id);
                continue;
            };
            if !self.filter_outgoing_signed_msg(signed_msg, &target_peer_id, route) {
                if let Err(err) = self.send_or_drop(&peer_id, bytes, signed_msg.priority()) {
                    info!("{:?} Error sending message to {:?}: {:?}.",
                          self,
                          target_peer_id,
                          err);
                }
            }
        }
        Ok(())
    }

    fn relay_to_client(&mut self,
                       signed_msg: SignedMessage,
                       peer_id: &PeerId)
                       -> Result<(), RoutingError> {
        let priority = signed_msg.priority();
        if self.peer_mgr.get_client(peer_id).is_some() {
            if self.filter_outgoing_signed_msg(&signed_msg, peer_id, 0) {
                return Ok(());
            }
            let hop_msg =
                try!(HopMessage::new(signed_msg, 0, vec![], self.full_id.signing_private_key()));
            let message = Message::Hop(hop_msg);
            let raw_bytes = try!(serialisation::serialise(&message));
            self.send_or_drop(peer_id, raw_bytes, priority)
        } else {
            // Acknowledge the message so that the sender doesn't retry.
            let hop = *self.name();
            self.send_ack_from(signed_msg.routing_message(), 0, Authority::ManagedNode(hop));
            debug!("{:?} Client connection not found for message {:?}.",
                   self,
                   signed_msg);
            Err(RoutingError::ClientConnectionNotFound)
        }
    }

    /// Returns hash of the `SignedMessage` if its not our turn to send the full message.
    fn message_to_send(&self,
                       signed_msg: &SignedMessage,
                       route: u8,
                       hop: &XorName)
                       -> Result<SignedMessage, RoutingError> {
        // When sending group messages, only one of us needs to send the whole message and everyone
        // else can send only a hash. If it's not our turn, replace `signed_msg` with the hash.
        // TODO: This applies only to messages where we are the original sender. The sending and
        // relaying code should be better separated.
        let src = &signed_msg.routing_message().src;
        if signed_msg.public_id() != self.full_id.public_id() || hop != self.name() ||
           !src.is_group() {
            return Ok(signed_msg.clone());
        }

        let group = self.peer_mgr.routing_table().closest_nodes_to(src.name(), GROUP_SIZE, true);
        // TODO: Better distribute the work among the group.
        if hop == group[route as usize % (group.len())].name() {
            return Ok(signed_msg.clone());
        }

        SignedMessage::new(try!(signed_msg.routing_message().to_grp_msg_hash()),
                           &self.full_id)
    }

    /// Returns whether we are the recipient of a message for the given authority.
    fn is_recipient(&self, dst: &Authority) -> bool {
        if let Authority::Client { ref client_key, .. } = *dst {
            client_key == self.full_id.public_id().signing_public_key()
        } else {
            self.is_proper() && self.peer_mgr.routing_table().is_recipient(dst.to_destination())
        }
    }

    /// Returns a `sent_to` entry for the next hop message, and a list of target peer IDs.
    fn get_targets(&self,
                   routing_msg: &RoutingMessage,
                   route: u8,
                   hop: &XorName,
                   sent_to: &[XorName])
                   -> Result<(Vec<XorName>, Vec<PeerId>), RoutingError> {
        if self.is_proper() {
            let destination = routing_msg.dst.to_destination();
            let targets = self.peer_mgr
                .routing_table()
                .target_nodes(destination, hop, route as usize)
                .into_iter()
                .filter(|target| !sent_to.contains(target.name()))
                .collect_vec();
            let new_sent_to = sent_to.iter()
                .chain(targets.iter())
                .cloned()
                .collect_vec();
            Ok((new_sent_to, self.peer_mgr.get_peer_ids(&targets)))
        } else if let Authority::Client { ref proxy_node_name, .. } = routing_msg.src {
            // We don't have any contacts in our routing table yet. Keep using
            // the proxy connection until we do.
            if let Some(&peer_id) = self.peer_mgr.get_proxy_peer_id(proxy_node_name) {
                Ok((vec![], vec![peer_id]))
            } else {
                error!("{:?} - Unable to find connection to proxy node in proxy map",
                       self);
                Err(RoutingError::ProxyConnectionNotFound)
            }
        } else {
            error!("{:?} - Source should be client if our state is a Client",
                   self);
            Err(RoutingError::InvalidSource)
        }
    }

    fn to_tunnel_hop_bytes(&self,
                           signed_msg: SignedMessage,
                           route: u8,
                           sent_to: Vec<XorName>,
                           dst: PeerId)
                           -> Result<Vec<u8>, RoutingError> {
        let hop_msg = try!(HopMessage::new(signed_msg.clone(),
                                           route,
                                           sent_to,
                                           self.full_id.signing_private_key()));
        let message = Message::TunnelHop {
            content: hop_msg,
            src: self.crust_service.id(),
            dst: dst,
        };

        Ok(try!(serialisation::serialise(&message)))
    }

    fn send_node_identify(&mut self, peer_id: PeerId) -> Result<(), RoutingError> {
        let serialised_public_id = try!(serialisation::serialise(self.full_id().public_id()));
        let signature = sign::sign_detached(&serialised_public_id,
                                            self.full_id().signing_private_key());
        let direct_message = DirectMessage::NodeIdentify {
            serialised_public_id: serialised_public_id,
            signature: signature,
        };

        self.send_direct_message(&peer_id, direct_message)
    }

    fn send_connection_info(&mut self,
                            their_public_id: PublicId,
                            src: Authority,
                            dst: Authority)
                            -> Result<Transition, RoutingError> {
        let their_name = *their_public_id.name();
        if let Some(peer_id) = self.peer_mgr.get_proxy_or_client_peer_id(&their_public_id) {
            try!(self.send_node_identify(peer_id));
            self.handle_node_identify(their_public_id, peer_id);
        } else if self.peer_mgr.allow_connect(&their_name) {
            if let Some(token) = self.peer_mgr.get_connection_token(src, dst, their_public_id) {
                self.crust_service.prepare_connection_info(token);
            } else {
                trace!("{:?} Already sent connection info to {:?}!",
                       self,
                       their_name);
            }
        }

        Ok(Transition::Stay)
    }

    fn dropped_client_connection(&mut self, peer_id: &PeerId) {
        if let Some(info) = self.peer_mgr.remove_client(peer_id) {
            if info.client_restriction {
                debug!("{:?} Client disconnected: {:?}", self, peer_id);
            } else {
                debug!("{:?} Joining node {:?} dropped. {} remaining.",
                       self,
                       peer_id,
                       self.peer_mgr.joining_nodes_num());
            }
        }
    }

    fn dropped_tunnel_client(&mut self, peer_id: &PeerId) {
        for other_id in self.tunnels.drop_client(peer_id) {
            let message = DirectMessage::TunnelClosed(*peer_id);
            let _ = self.send_direct_message(&other_id, message);
        }
    }

    fn dropped_tunnel_node(&mut self, peer_id: &PeerId) {
        let peers = self.tunnels
            .remove_tunnel(peer_id)
            .into_iter()
            .filter_map(|dst_id| {
                self.peer_mgr.get_routing_peer(&dst_id).map(|dst_pub_id| (dst_id, *dst_pub_id))
            })
            .collect_vec();
        for (dst_id, pub_id) in peers {
            self.dropped_routing_node_connection(&dst_id);
            debug!("{:?} Lost tunnel for peer {:?} ({:?}). Requesting new tunnel.",
                   self,
                   dst_id,
                   pub_id.name());
            self.find_tunnel_for_peer(dst_id, &pub_id);
        }
    }

    fn dropped_routing_node_connection(&mut self, peer_id: &PeerId) -> bool {
        if let Some((name, DroppedNodeDetails { incomplete_bucket })) = self.peer_mgr
            .remove_peer(peer_id) {
            info!("{:?} Dropped {:?} from the routing table.", self, &name);

            let common_groups = self.peer_mgr
                .routing_table()
                .is_in_any_close_group_with(self.name().bucket_index(&name), GROUP_SIZE);
            if common_groups {
                // If the lost node shared some close group with us, send a NodeLost event.
                let event = Event::NodeLost(name, self.peer_mgr.routing_table().clone());
                if let Err(err) = self.event_sender.send(event) {
                    error!("{:?} Error sending event to routing user - {:?}", self, err);
                }
            }
            if let Some(bucket_index) = incomplete_bucket {
                if let Err(e) = self.request_bucket_ids(bucket_index) {
                    debug!("{:?} Failed to request replacement connection_info from bucket \
                            {}: {:?}.",
                           self,
                           bucket_index,
                           e);
                }
            }
            if self.peer_mgr.routing_table().len() < GROUP_SIZE - 1 {
                debug!("{:?} Lost connection, less than {} remaining.",
                       self,
                       GROUP_SIZE - 1);
                if !self.is_first_node {
                    self.send_event(Event::RestartRequired);
                    return false;
                }
            }

            self.reset_bucket_refresh_timer();
        };

        true
    }

    fn dropped_proxy_connection(&self, peer_id: &PeerId) -> bool {
        if let Some(proxy_public_id) = self.peer_mgr.get_proxy_public_id(peer_id) {
            debug!("{:?} Lost bootstrap connection to {:?} ({:?}).",
                   self,
                   proxy_public_id.name(),
                   peer_id);
            self.send_event(Event::Terminate);
            false
        } else {
            true
        }
    }

    // Proper node is either the first node in the network or a node which has
    // at least one entry in its routing table.
    fn is_proper(&self) -> bool {
        self.is_first_node || self.peer_mgr.routing_table().len() >= 1
    }
}

impl Base for Node {
    fn crust_service(&self) -> &Service {
        &self.crust_service
    }

    fn full_id(&self) -> &FullId {
        &self.full_id
    }

    fn handle_lost_peer(&mut self, peer_id: PeerId) -> Transition {
        if peer_id == self.crust_service.id() {
            error!("{:?} LostPeer fired with our crust peer id", self);
            return Transition::Stay;
        }

        debug!("{:?} Received LostPeer - {:?}", self, peer_id);

        self.dropped_tunnel_client(&peer_id);
        self.dropped_client_connection(&peer_id);
        self.dropped_tunnel_node(&peer_id);

        let mut result = Transition::Stay;

        if !self.dropped_proxy_connection(&peer_id) {
            result = Transition::Terminate;
        }

        if !self.dropped_routing_node_connection(&peer_id) {
            result = Transition::Terminate;
        }

        result
    }

    fn send_event(&self, event: Event) {
        let _ = self.event_sender.send(event);
    }

    fn stats(&mut self) -> &mut Stats {
        &mut self.stats
    }

    fn wrap_direct_message(&self,
                           dst_id: &PeerId,
                           direct_message: DirectMessage)
                           -> (Message, PeerId) {
        if let Some(&tunnel_id) = self.tunnels.tunnel_for(dst_id) {
            let message = Message::TunnelDirect {
                content: direct_message,
                src: self.crust_service.id(),
                dst: *dst_id,
            };
            (message, tunnel_id)
        } else {
            (Message::Direct(direct_message), *dst_id)
        }
    }
}

impl Bootstrapped for Node {
    fn accumulate(&mut self,
                  routing_msg: &RoutingMessage,
                  public_id: &PublicId)
                  -> Result<Option<RoutingMessage>, RoutingError> {
        self.msg_accumulator.add(routing_msg, public_id)
    }

    fn ack_mgr(&self) -> &AckManager {
        &self.ack_mgr
    }

    fn ack_mgr_mut(&mut self) -> &mut AckManager {
        &mut self.ack_mgr
    }

    fn add_to_user_msg_cache(&mut self,
                             hash: u64,
                             part_count: u32,
                             part_index: u32,
                             payload: Vec<u8>)
                             -> Option<UserMessage> {
        self.user_msg_cache.add(hash, part_count, part_index, payload)
    }

    fn send_routing_message_via_route(&mut self,
                                      routing_msg: RoutingMessage,
                                      route: u8)
                                      -> Result<(), RoutingError> {
        let signed_msg = try!(SignedMessage::new(routing_msg, &self.full_id));
        let hop = *self.name();
        try!(self.send_signed_message(&signed_msg, route, &hop, &[hop]));

        // If we need to handle this message, handle it.
        let sent_msg = try!(self.message_to_send(&signed_msg, route, &hop));
        if self.is_recipient(&sent_msg.routing_message().dst) &&
           self.signed_msg_filter.filter_incoming(&sent_msg) == 1 {
            self.handle_routing_message(sent_msg.routing_message(), sent_msg.public_id())
        } else {
            Ok(())
        }
    }

    fn signed_msg_filter(&mut self) -> &mut SignedMessageFilter {
        &mut self.signed_msg_filter
    }

    fn timer(&mut self) -> &mut Timer {
        &mut self.timer
    }
}

impl Debug for Node {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "Node({})", self.name())
    }
}

// Verify the serialised public id against the signature.
fn verify_signed_public_id(serialised_public_id: &[u8],
                           signature: &sign::Signature)
                           -> Result<PublicId, RoutingError> {
    let public_id: PublicId = try!(serialisation::deserialise(serialised_public_id));
    let public_key = public_id.signing_public_key();
    if sign::verify_detached(signature, serialised_public_id, public_key) {
        Ok(public_id)
    } else {
        Err(RoutingError::FailedSignature)
    }
}
