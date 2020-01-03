// Copyright 2019 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use super::{
    adult::{Adult, AdultDetails},
    bootstrapping_peer::{BootstrappingPeer, BootstrappingPeerDetails},
    common::Base,
};
use crate::{
    chain::{EldersInfo, GenesisPfxInfo, NetworkParams},
    error::{InterfaceError, RoutingError},
    event::{ConnectEvent, Event},
    id::{FullId, P2pNode},
    messages::{
        BootstrapResponse, DirectMessage, HopMessage, JoinRequest, MessageContent, RoutingMessage,
        SignedRoutingMessage,
    },
    outbox::EventBox,
    peer_map::PeerMap,
    relocation::RelocatePayload,
    rng::MainRng,
    routing_message_filter::RoutingMessageFilter,
    state_machine::{State, Transition},
    timer::Timer,
    xor_space::XorName,
    Authority, NetworkService,
};
use log::LogLevel;
use std::{
    fmt::{self, Display, Formatter},
    time::Duration,
};

/// Time after which bootstrap is cancelled (and possibly retried).
pub(crate) const JOIN_TIMEOUT: Duration = Duration::from_secs(600);

pub(crate) struct JoiningPeerDetails {
    pub(crate) network_service: NetworkService,
    pub(crate) full_id: FullId,
    pub(crate) network_cfg: NetworkParams,
    pub(crate) timer: Timer,
    pub(crate) rng: MainRng,
    pub(crate) elders_info: EldersInfo,
    pub(crate) relocate_payload: Option<RelocatePayload>,
}

// State of a node after bootstrapping, while joining a section
pub(crate) struct JoiningPeer {
    network_service: NetworkService,
    routing_msg_filter: RoutingMessageFilter,
    routing_msg_backlog: Vec<SignedRoutingMessage>,
    direct_msg_backlog: Vec<(P2pNode, DirectMessage)>,
    full_id: FullId,
    timer: Timer,
    rng: MainRng,
    elders_info: EldersInfo,
    join_type: JoinType,
    network_cfg: NetworkParams,
}

impl JoiningPeer {
    pub(crate) fn new(details: JoiningPeerDetails) -> Self {
        let join_type = match details.relocate_payload {
            Some(payload) => JoinType::Relocate(payload),
            None => {
                let timeout_token = details.timer.schedule(JOIN_TIMEOUT);
                JoinType::First { timeout_token }
            }
        };

        let mut joining_peer = Self {
            network_service: details.network_service,
            routing_msg_filter: RoutingMessageFilter::new(),
            routing_msg_backlog: vec![],
            direct_msg_backlog: vec![],
            full_id: details.full_id,
            timer: details.timer,
            rng: details.rng,
            elders_info: details.elders_info,
            join_type,
            network_cfg: details.network_cfg,
        };

        joining_peer.send_join_requests();
        joining_peer
    }

    pub(crate) fn into_adult(
        self,
        gen_pfx_info: GenesisPfxInfo,
        outbox: &mut dyn EventBox,
    ) -> Result<State, RoutingError> {
        let details = AdultDetails {
            network_service: self.network_service,
            event_backlog: vec![],
            full_id: self.full_id,
            gen_pfx_info,
            routing_msg_backlog: self.routing_msg_backlog,
            direct_msg_backlog: self.direct_msg_backlog,
            routing_msg_filter: self.routing_msg_filter,
            sig_accumulator: Default::default(),
            timer: self.timer,
            rng: self.rng,
            network_cfg: self.network_cfg,
        };
        let adult = Adult::new(details, Default::default(), outbox).map(State::Adult);

        let connect_type = match self.join_type {
            JoinType::First { .. } => ConnectEvent::First,
            JoinType::Relocate(_) => ConnectEvent::Relocate,
        };
        outbox.send_event(Event::Connected(connect_type));
        adult
    }

    pub(crate) fn rebootstrap(mut self) -> Result<State, RoutingError> {
        let full_id = FullId::gen(&mut self.rng);

        Ok(State::BootstrappingPeer(BootstrappingPeer::new(
            BootstrappingPeerDetails {
                network_service: self.network_service,
                full_id,
                network_cfg: self.network_cfg,
                timer: self.timer,
                rng: self.rng,
            },
        )))
    }

    fn send_join_requests(&mut self) {
        let elders_version = self.elders_info.version();
        for dst in self.elders_info.clone().member_nodes() {
            info!("{} - Sending JoinRequest to {}", self, dst.public_id());

            let relocate_payload = match &self.join_type {
                JoinType::First { .. } => None,
                JoinType::Relocate(payload) => Some(payload.clone()),
            };
            let join_request = JoinRequest {
                elders_version,
                relocate_payload,
            };

            self.send_direct_message(
                dst.connection_info(),
                DirectMessage::JoinRequest(join_request),
            );
        }
    }

    fn dispatch_routing_message(
        &mut self,
        msg: SignedRoutingMessage,
        _outbox: &mut dyn EventBox,
    ) -> Result<Transition, RoutingError> {
        let (msg, metadata) = msg.into_parts();

        match msg {
            RoutingMessage {
                content: MessageContent::NodeApproval(gen_info),
                src: Authority::PrefixSection(_),
                dst: Authority::Node { .. },
            } => Ok(self.handle_node_approval(gen_info)),
            _ => {
                debug!(
                    "{} - Unhandled routing message, adding to backlog: {:?}",
                    self, msg
                );
                self.routing_msg_backlog
                    .push(SignedRoutingMessage::from_parts(msg, metadata));
                Ok(Transition::Stay)
            }
        }
    }

    fn handle_node_approval(&mut self, gen_pfx_info: GenesisPfxInfo) -> Transition {
        info!(
            "{} - This node has been approved to join the network at {:?}!",
            self,
            gen_pfx_info.latest_info.prefix(),
        );
        Transition::IntoAdult { gen_pfx_info }
    }

    #[cfg(feature = "mock_base")]
    pub(crate) fn get_timed_out_tokens(&mut self) -> Vec<u64> {
        self.timer.get_timed_out_tokens()
    }
}

impl Base for JoiningPeer {
    fn network_service(&self) -> &NetworkService {
        &self.network_service
    }

    fn network_service_mut(&mut self) -> &mut NetworkService {
        &mut self.network_service
    }

    fn full_id(&self) -> &FullId {
        &self.full_id
    }

    fn in_authority(&self, dst: &Authority<XorName>) -> bool {
        dst.is_single() && dst.name() == *self.full_id.public_id().name()
    }

    fn peer_map(&self) -> &PeerMap {
        &self.network_service().peer_map
    }

    fn peer_map_mut(&mut self) -> &mut PeerMap {
        &mut self.network_service_mut().peer_map
    }

    fn timer(&mut self) -> &mut Timer {
        &mut self.timer
    }

    fn rng(&mut self) -> &mut MainRng {
        &mut self.rng
    }

    fn handle_send_message(
        &mut self,
        _: Authority<XorName>,
        _: Authority<XorName>,
        _: Vec<u8>,
    ) -> Result<(), InterfaceError> {
        warn!("{} - Cannot handle SendMessage - not joined.", self);
        // TODO: return Err here eventually. Returning Ok for now to
        // preserve the pre-refactor behaviour.
        Ok(())
    }

    fn handle_timeout(&mut self, token: u64, _: &mut dyn EventBox) -> Transition {
        let join_token = match self.join_type {
            JoinType::First { timeout_token } => timeout_token,
            JoinType::Relocate(_) => return Transition::Stay,
        };

        if join_token == token {
            debug!("{} - Timeout when trying to join a section.", self);
            self.network_service_mut().remove_and_disconnect_all();
            Transition::Rebootstrap
        } else {
            Transition::Stay
        }
    }

    fn handle_direct_message(
        &mut self,
        msg: DirectMessage,
        p2p_node: P2pNode,
        _outbox: &mut dyn EventBox,
    ) -> Result<Transition, RoutingError> {
        match msg {
            DirectMessage::BootstrapResponse(BootstrapResponse::Join(info)) => {
                if info.version() > self.elders_info.version() {
                    if info.prefix().matches(self.name()) {
                        info!("{} - Newer Join response for our prefix {:?}", self, info);
                        self.elders_info = info;
                        self.send_join_requests();
                    } else {
                        log_or_panic!(
                            LogLevel::Error,
                            "{} - Newer Join response not for our prefix {:?}",
                            self,
                            info
                        );
                    }
                }
            }
            DirectMessage::ConnectionResponse | DirectMessage::BootstrapResponse(_) => (),
            _ => {
                debug!(
                    "{} Unhandled direct message from {}, adding to backlog: {:?}",
                    self,
                    p2p_node.public_id(),
                    msg
                );
                self.direct_msg_backlog.push((p2p_node, msg));
            }
        }

        Ok(Transition::Stay)
    }

    fn handle_hop_message(
        &mut self,
        msg: HopMessage,
        outbox: &mut dyn EventBox,
    ) -> Result<Transition, RoutingError> {
        let HopMessage { content: msg, .. } = msg;

        if !self
            .routing_msg_filter
            .filter_incoming(msg.routing_message())
            .is_new()
        {
            trace!(
                "{} Known message: {:?} - not handling further",
                self,
                msg.routing_message()
            );
            return Ok(Transition::Stay);
        }

        if self.in_authority(&msg.routing_message().dst) {
            self.check_signed_message_integrity(&msg)?;
            self.dispatch_routing_message(msg, outbox)
        } else {
            self.routing_msg_backlog.push(msg);
            Ok(Transition::Stay)
        }
    }

    fn send_routing_message(&mut self, routing_msg: RoutingMessage) -> Result<(), RoutingError> {
        warn!(
            "{} - Tried to send a routing message: {:?}",
            self, routing_msg
        );
        Ok(())
    }
}

impl Display for JoiningPeer {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "JoiningPeer({})", self.name())
    }
}

#[allow(clippy::large_enum_variant)]
enum JoinType {
    // Node joining the network for the first time.
    First { timeout_token: u64 },
    // Node being relocated.
    Relocate(RelocatePayload),
}
