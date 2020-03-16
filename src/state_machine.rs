// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use crate::{
    action::Action,
    chain::{EldersInfo, GenesisPfxInfo},
    error::RoutingError,
    id::{P2pNode, PublicId},
    network_service::{NetworkBuilder, NetworkService},
    outbox::EventBox,
    pause::PausedState,
    quic_p2p::EventSenders,
    relocation::{RelocatePayload, SignedRelocateDetails},
    states::{common::Base, Adult, BootstrappingPeer, Elder, JoiningPeer},
    timer::Timer,
    xor_space::{Prefix, XorName},
    NetworkConfig, NetworkEvent,
};
#[cfg(feature = "mock_base")]
use crate::{
    chain::Chain,
    location::{DstLocation, SrcLocation},
    rng::MainRng,
};
use crossbeam_channel as mpmc;
use std::{
    fmt::{self, Debug, Display, Formatter},
    mem,
    net::SocketAddr,
};

// Execute $expr on the current variant of $self. Execute $term_expr if the current variant is
// `Terminated`.
macro_rules! state_dispatch {
    ($self:expr, $state:pat => $expr:expr, Terminated => $term_expr:expr) => {
        match $self {
            Self::BootstrappingPeer($state) => $expr,
            Self::JoiningPeer($state) => $expr,
            Self::Adult($state) => $expr,
            Self::Elder($state) => $expr,
            Self::Terminated => $term_expr,
        }
    };
}

/// Holds the current state and handles state transitions.
pub struct StateMachine {
    state: State,
    network_rx: mpmc::Receiver<NetworkEvent>,
    network_rx_idx: usize,
    action_rx: mpmc::Receiver<Action>,
    action_rx_idx: usize,
    is_running: bool,
}

// FIXME - See https://maidsafe.atlassian.net/browse/MAID-2026 for info on removing this exclusion.
#[allow(clippy::large_enum_variant)]
pub enum State {
    BootstrappingPeer(BootstrappingPeer),
    JoiningPeer(JoiningPeer),
    Adult(Adult),
    Elder(Elder),
    Terminated,
}

impl State {
    pub fn handle_action(&mut self, action: Action, outbox: &mut dyn EventBox) -> Transition {
        state_dispatch!(
            *self,
            ref mut state => state.handle_action(action, outbox),
            Terminated => Transition::Terminate
        )
    }

    fn handle_network_event(
        &mut self,
        event: NetworkEvent,
        outbox: &mut dyn EventBox,
    ) -> Transition {
        state_dispatch!(
            *self,
            ref mut state => state.handle_network_event(event, outbox),
            Terminated => Transition::Terminate
        )
    }

    pub fn id(&self) -> Option<PublicId> {
        state_dispatch!(
            *self,
            ref state => Some(*state.id()),
            Terminated => None
        )
    }

    pub fn close_group(&self, name: XorName, count: usize) -> Option<Vec<XorName>> {
        state_dispatch!(
            *self,
            ref state => state.close_group(name, count),
            Terminated => None
        )
    }

    pub fn our_elders(&self) -> Option<impl Iterator<Item = &P2pNode>> {
        match *self {
            Self::Elder(ref state) => Some(state.our_elders()),
            Self::BootstrappingPeer(_)
            | Self::JoiningPeer(_)
            | Self::Adult(_)
            | Self::Terminated => None,
        }
    }

    pub fn matches_our_prefix(&self, name: &XorName) -> Result<bool, RoutingError> {
        match *self {
            Self::Elder(ref state) => Ok(state.our_prefix().matches(name)),
            Self::Adult(ref state) => Ok(state.our_prefix().matches(name)),
            Self::BootstrappingPeer(_) | Self::JoiningPeer(_) | Self::Terminated => {
                Err(RoutingError::InvalidState)
            }
        }
    }

    pub fn closest_known_elders_to<'a>(
        &'a self,
        name: &XorName,
    ) -> Result<Box<dyn Iterator<Item = &P2pNode> + 'a>, RoutingError> {
        match *self {
            Self::Elder(ref state) => Ok(Box::new(state.closest_known_elders_to(name))),
            Self::Adult(ref state) => Ok(Box::new(state.closest_known_elders_to(name))),
            Self::BootstrappingPeer(_) | Self::JoiningPeer(_) | Self::Terminated => {
                Err(RoutingError::InvalidState)
            }
        }
    }

    pub fn our_connection_info(&mut self) -> Result<SocketAddr, RoutingError> {
        state_dispatch!(
            self,
            state => state.network_service_mut().our_connection_info().map_err(RoutingError::from),
            Terminated => Err(RoutingError::InvalidState)
        )
    }

    /// Returns this elder mut state.
    pub fn elder_state_mut(&mut self) -> Option<&mut Elder> {
        match *self {
            Self::Elder(ref mut state) => Some(state),
            _ => None,
        }
    }

    fn replace_with<F, E>(&mut self, f: F)
    where
        F: FnOnce(Self) -> Result<Self, E>,
        E: Debug,
    {
        let old_state = mem::replace(self, Self::Terminated);
        let old_state_log_ident = format!("{}", old_state);

        match f(old_state) {
            Ok(new_state) => *self = new_state,
            Err(error) => error!(
                "{} - Failed state transition: {:?}",
                old_state_log_ident, error
            ),
        }
    }
}

impl Display for State {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        state_dispatch!(
            *self,
            ref state => write!(formatter, "{}", state),
            Terminated => write!(formatter, "Terminated")
        )
    }
}

impl Debug for State {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        state_dispatch!(
            *self,
            ref state => write!(formatter, "State::{}", state),
            Terminated => write!(formatter, "State::Terminated")
        )
    }
}

#[cfg(feature = "mock_base")]
impl State {
    pub fn chain(&self) -> Option<&Chain> {
        match *self {
            Self::Adult(ref state) => Some(state.chain()),
            Self::Elder(ref state) => Some(state.chain()),
            Self::BootstrappingPeer(_) | Self::JoiningPeer(_) | Self::Terminated => None,
        }
    }

    /// Returns this elder state.
    pub fn elder_state(&self) -> Option<&Elder> {
        match *self {
            Self::Elder(ref state) => Some(state),
            _ => None,
        }
    }

    pub fn process_timers(&mut self) {
        state_dispatch!(
            self,
            state => state.process_timers(),
            Terminated => ()
        )
    }

    pub fn has_unpolled_observations(&self) -> bool {
        match *self {
            Self::Terminated | Self::BootstrappingPeer(_) | Self::JoiningPeer(_) => false,
            Self::Adult(ref state) => state.has_unpolled_observations(),
            Self::Elder(ref state) => state.has_unpolled_observations(),
        }
    }

    pub fn unpolled_observations_string(&self) -> String {
        match *self {
            Self::Terminated | Self::BootstrappingPeer(_) | Self::JoiningPeer(_) => String::new(),
            Self::Adult(ref state) => state.unpolled_observations_string(),
            Self::Elder(ref state) => state.unpolled_observations_string(),
        }
    }

    pub fn in_src_location(&self, src: &SrcLocation) -> bool {
        match self {
            Self::Elder(state) => state.in_src_location(src),
            _ => false,
        }
    }

    pub fn in_dst_location(&self, dst: &DstLocation) -> bool {
        state_dispatch!(
            *self,
            ref state => state.in_dst_location(dst),
            Terminated => false
        )
    }

    pub fn rng(&mut self) -> &mut MainRng {
        state_dispatch!(
            self,
            state => state.rng(),
            Terminated => unreachable!()
        )
    }
}

/// Enum returned from many message handlers
// FIXME - See https://maidsafe.atlassian.net/browse/MAID-2026 for info on removing this exclusion.
#[allow(clippy::large_enum_variant)]
#[derive(PartialEq, Eq)]
pub enum Transition {
    Stay,
    // `BootstrappingPeer` state transitioning to `JoiningPeer`
    IntoJoining {
        info: EldersInfo,
        relocate_payload: Option<RelocatePayload>,
    },
    // `JoiningPeer` failing to join and transitioning back to `BootstrappingPeer`
    Rebootstrap,
    // Node getting relocated.
    Relocate {
        conn_infos: Vec<SocketAddr>,
        details: SignedRelocateDetails,
    },
    // `JoiningPeer` state transitioning to `Adult`.
    IntoAdult {
        gen_pfx_info: GenesisPfxInfo,
    },
    // `Adult` state transition to `Elder`.
    IntoElder {
        old_pfx: Prefix<XorName>,
    },
    // `Elder` state transition to `Adult` as a result of demotion
    Demote {
        gen_pfx_info: GenesisPfxInfo,
    },
    Terminate,
}

impl Debug for Transition {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Stay => write!(f, "Stay"),
            Self::IntoJoining { .. } => write!(f, "IntoJoining"),
            Self::Rebootstrap => write!(f, "Rebootstrap"),
            Self::Relocate { .. } => write!(f, "Relocate"),
            Self::IntoAdult { .. } => write!(f, "IntoAdult"),
            Self::IntoElder { .. } => write!(f, "IntoElder"),
            Self::Demote { .. } => write!(f, "Demote"),
            Self::Terminate => write!(f, "Terminate"),
        }
    }
}

impl StateMachine {
    // Construct a new StateMachine by passing a function returning the initial state.
    pub fn new<F>(
        init_state: F,
        network_config: NetworkConfig,
        client_tx: mpmc::Sender<NetworkEvent>,
        outbox: &mut dyn EventBox,
    ) -> (mpmc::Sender<Action>, Self)
    where
        F: FnOnce(NetworkService, Timer, &mut dyn EventBox) -> State,
    {
        let (action_tx, action_rx) = mpmc::unbounded();
        let (network_tx, network_rx) = {
            let (node_tx, node_rx) = mpmc::unbounded();
            (EventSenders { node_tx, client_tx }, node_rx)
        };

        let network_service = match NetworkBuilder::new(network_tx)
            .with_config(network_config)
            .build()
        {
            Ok(network_service) => network_service,
            Err(err) => panic!("Unable to start network service: {:?}", err),
        };

        let timer = Timer::new(action_tx.clone());
        let state = init_state(network_service, timer, outbox);
        let is_running = match state {
            State::Terminated => false,
            _ => true,
        };

        let machine = Self {
            state,
            network_rx,
            network_rx_idx: 0,
            action_rx,
            action_rx_idx: 0,
            is_running,
        };

        (action_tx, machine)
    }

    pub fn pause(self) -> Result<PausedState, RoutingError> {
        info!("{} - Pause", self.current());

        let mut paused_state = match self.state {
            State::Elder(state) => state.pause(),
            State::Adult(state) => state.pause(),
            _ => return Err(RoutingError::InvalidState),
        };

        paused_state.network_rx = Some(self.network_rx);
        Ok(paused_state)
    }

    pub fn resume(mut state: PausedState) -> (mpmc::Sender<Action>, Self) {
        let (action_tx, action_rx) = mpmc::unbounded();
        let network_rx = state.network_rx.take().expect("PausedState is incomplete");

        let timer = Timer::new(action_tx.clone());
        let state = if state.is_elder() {
            State::Elder(Elder::resume(state, timer))
        } else {
            State::Adult(Adult::resume(state, timer))
        };

        let machine = Self {
            state,
            network_rx,
            network_rx_idx: 0,
            action_rx,
            action_rx_idx: 0,
            is_running: true,
        };

        info!("{} - Resume", machine.current());

        (action_tx, machine)
    }

    fn handle_network_event(&mut self, event: NetworkEvent, outbox: &mut dyn EventBox) {
        let transition = self.state.handle_network_event(event, outbox);
        self.apply_transition(transition, outbox)
    }

    fn handle_action(&mut self, action: Action, outbox: &mut dyn EventBox) {
        let transition = self.state.handle_action(action, outbox);
        self.apply_transition(transition, outbox)
    }

    pub fn apply_transition(&mut self, transition: Transition, outbox: &mut dyn EventBox) {
        use self::Transition::*;
        match transition {
            Stay => (),
            Terminate => self.terminate(),
            IntoJoining {
                info,
                relocate_payload,
            } => self.state.replace_with(|state| match state {
                State::BootstrappingPeer(src) => src.into_joining(info, relocate_payload, outbox),
                _ => unreachable!(),
            }),
            Rebootstrap => self.state.replace_with(|state| match state {
                State::JoiningPeer(src) => src.rebootstrap(),
                _ => unreachable!(),
            }),
            Relocate {
                details,
                conn_infos,
            } => self.state.replace_with(|state| match state {
                State::Adult(src) => src.relocate(conn_infos, details),
                _ => unreachable!(),
            }),
            IntoAdult { gen_pfx_info } => self.state.replace_with(|state| match state {
                State::JoiningPeer(src) => src.into_adult(gen_pfx_info, outbox),
                _ => unreachable!(),
            }),
            IntoElder { old_pfx } => self.state.replace_with(|state| match state {
                State::Adult(src) => src.into_elder(old_pfx, outbox),
                _ => unreachable!(),
            }),
            Demote { gen_pfx_info } => self.state.replace_with(|state| match state {
                State::Elder(src) => src.demote(gen_pfx_info, outbox),
                _ => unreachable!(),
            }),
        }
    }

    fn terminate(&mut self) {
        debug!("{} Terminating state machine", self);
        self.is_running = false;
    }

    /// Register the state machine event channels with the provided [selector](mpmc::Select).
    pub fn register<'a>(&'a mut self, select: &mut mpmc::Select<'a>) {
        // Populate action_rx timeouts
        #[cfg(feature = "mock_base")]
        self.state.process_timers();

        let network_rx_idx = select.recv(&self.network_rx);
        let action_rx_idx = select.recv(&self.action_rx);
        self.network_rx_idx = network_rx_idx;
        self.action_rx_idx = action_rx_idx;
    }

    /// Processes events received externally from one of the channels.
    /// For this function to work properly, the state machine event channels need to
    /// be registered by calling [`StateMachine::register`](#method.register).
    /// [`Select::ready`] needs to be called to get `op_index`, the event channel index.
    /// The resulting events are streamed into `outbox`.
    ///
    /// This function is non-blocking.
    ///
    /// Errors are permanent failures due to either: state machine termination,
    /// the permanent closing of one of the event channels, or an invalid (unknown)
    /// channel index.
    ///
    /// [`Select::ready`]: https://docs.rs/crossbeam-channel/0.3/crossbeam_channel/struct.Select.html#method.ready
    ///
    /// The returned `bool` can be safely ignored by the consumers of this crate. It is for
    /// internal uses only and will always be `true` unless compiled with `feature=mock_base`.
    pub fn step(
        &mut self,
        op_index: usize,
        outbox: &mut dyn EventBox,
    ) -> Result<bool, mpmc::RecvError> {
        if !self.is_running {
            return Err(mpmc::RecvError);
        }
        match op_index {
            idx if idx == self.network_rx_idx => {
                let event = self.network_rx.recv()?;
                self.handle_network_event(event, outbox);
                Ok(true)
            }
            idx if idx == self.action_rx_idx => {
                let action = self.action_rx.recv()?;

                let status = is_busy(&action);
                self.handle_action(action, outbox);
                Ok(status)
            }
            _idx => Err(mpmc::RecvError),
        }
    }

    /// Get reference to the current state.
    pub fn current(&self) -> &State {
        &self.state
    }

    /// Get mutable reference to the current state.
    pub fn current_mut(&mut self) -> &mut State {
        &mut self.state
    }
}

impl Display for StateMachine {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "{}", self.state)
    }
}

#[cfg(not(feature = "mock_base"))]
fn is_busy(_: &Action) -> bool {
    true
}

#[cfg(feature = "mock_base")]
fn is_busy(action: &Action) -> bool {
    match action {
        // Don't consider handling a timeout as being busy. This is a workaround to prevent
        // infinite polling.
        Action::HandleTimeout(_) => false,
        _ => true,
    }
}
