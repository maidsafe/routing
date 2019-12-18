// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use super::routing_table::Error as RoutingTableError;
use crate::{action::Action, event::Event, id::PublicId, quic_p2p};
use bincode::ErrorKind;
use crossbeam_channel as mpmc;
use err_derive::Error;
use std::sync::mpsc;

/// The type returned by the routing message handling methods.
pub type Result<T> = ::std::result::Result<T, RoutingError>;

/// The type of errors that can occur if routing is unable to handle a send request.
#[derive(Debug, Error, derive_more::From)]
// FIXME - See https://maidsafe.atlassian.net/browse/MAID-2026 for info on removing this exclusion.
#[allow(clippy::large_enum_variant, missing_docs)]
pub enum InterfaceError {
    #[error(display = "We are not connected to the network.")]
    NotConnected,
    #[error(display = "We are not in a state to handle the action.")]
    InvalidState,
    #[error(display = "Error while trying to receive a message from a mpsc channel.")]
    MpscRecvError(mpsc::RecvError),
    #[error(display = "Error while trying to receive a message from a mpsc channel.")]
    MpmcSendActionError(mpmc::SendError<Action>),
}

/// The type of errors that can occur during handling of routing events.
#[derive(Debug, Error, derive_more::From)]
// FIXME - See https://maidsafe.atlassian.net/browse/MAID-2026 for info on removing this exclusion.
#[allow(clippy::large_enum_variant, missing_docs)]
pub enum RoutingError {
    #[error(display = "Invalid State.")]
    Terminated,
    #[error(display = "Invalid requester or handler authorities.")]
    BadAuthority,
    #[error(display = "Failed signature check.")]
    FailedSignature,
    #[error(display = "Duplicate request received.")]
    FilterCheckFailed,
    #[error(display = "Routing Table error.")]
    RoutingTable(RoutingTableError),
    #[error(display = "Interface error.")]
    Interface(InterfaceError),
    #[error(display = "Network layer error.")]
    Network(quic_p2p::Error),
    #[error(display = " Channel sending error.")]
    MpscSendEventError(mpsc::SendError<Event>),
    #[error(display = "Current state is invalid for the operation.")]
    InvalidStateForOperation,
    /// Serialisation Error
    // SerialisationError(serialisation::SerialisationError),
    #[error(display = "bincode.")]
    Bincode(ErrorKind),
    #[error(display = "Peer not found.")]
    PeerNotFound(PublicId),
    #[error(display = "Invalid Destination.")]
    InvalidDestination,
    #[error(display = "Invalid Source.")]
    InvalidSource,
    #[error(display = "Content of a received message is inconsistent.")]
    InvalidMessage,
    #[error(display = "A signed message's chain of proving sections is invalid.")]
    InvalidProvingSection,
    #[error(display = "A signed message could not be trusted.")]
    UntrustedMessage,
    #[error(display = "A new SectionInfo is invalid.")]
    InvalidNewSectionInfo,
    #[error(display = "An Elder DKG result is invalid.")]
    InvalidElderDkgResult,
}

impl From<Box<ErrorKind>> for RoutingError {
    fn from(error: Box<ErrorKind>) -> RoutingError {
        RoutingError::Bincode(*error)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum BootstrapResponseError {
    NotApproved,
    TooFewPeers,
}
