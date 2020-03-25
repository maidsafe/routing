// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use crate::{
    error::RoutingError,
    location::{DstLocation, SrcLocation},
};
use bytes::Bytes;
use crossbeam_channel::Sender;
use hex_fmt::HexFmt;
use quic_p2p::Token;
use std::{
    fmt::{self, Debug, Formatter},
    net::SocketAddr,
};

/// An Action initiates a message flow < A | B > where we are (a part of) A.
///    1. `Action::SendMessage` hands a fully formed `SignedMessage` over to `Core`
///       for it to be sent on across the network.
///    2. `Action::Terminate` indicates to `Core` that no new actions should be taken and all
///       pending events should be handled.
///       After completion `Core` will send `Event::Terminated`.
#[allow(clippy::large_enum_variant)]
pub enum Action {
    SendMessage {
        src: SrcLocation,
        dst: DstLocation,
        content: Vec<u8>,
        result_tx: Sender<Result<(), RoutingError>>,
    },
    HandleTimeout(u64),
    DisconnectClient {
        peer_addr: SocketAddr,
        result_tx: Sender<Result<(), RoutingError>>,
    },
    SendMessageToClient {
        peer_addr: SocketAddr,
        msg: Bytes,
        token: Token,
        result_tx: Sender<Result<(), RoutingError>>,
    },
}

impl Debug for Action {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        match *self {
            Self::SendMessage { ref content, .. } => write!(
                formatter,
                "Action::SendMessage {{ \"{:<8}\", result_tx }}",
                HexFmt(content)
            ),
            Self::HandleTimeout(token) => write!(formatter, "Action::HandleTimeout({})", token),
            Self::DisconnectClient { peer_addr, .. } => {
                write!(formatter, "Action::DisconnectClient: {}", peer_addr)
            }
            Self::SendMessageToClient {
                peer_addr, token, ..
            } => write!(
                formatter,
                "Action::SendMessageToClient: {}, token: {}",
                peer_addr, token
            ),
        }
    }
}
