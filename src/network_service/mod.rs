// Copyright 2019 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

mod sending_targets_cache;

use crate::{
    quic_p2p::{Builder, Peer, QuicP2p, QuicP2pError, Token},
    utils::LogIdent,
    ConnectionInfo, NetworkConfig, NetworkEvent,
};
use bytes::Bytes;
use crossbeam_channel::Sender;
use std::net::SocketAddr;

use sending_targets_cache::SendingTargetsCache;

/// Struct that handles network operations: sending and receiving messages, as well as resending on
/// failure.
pub struct NetworkService {
    quic_p2p: QuicP2p,
    cache: SendingTargetsCache,
    next_msg_token: Token,
}

impl NetworkService {
    pub fn service_mut(&mut self) -> &mut QuicP2p {
        &mut self.quic_p2p
    }

    pub fn next_msg_token(&mut self) -> Token {
        self.next_msg_token = self.next_msg_token.wrapping_add(1);
        self.next_msg_token
    }

    pub fn targets_cache_mut(&mut self) -> &mut SendingTargetsCache {
        &mut self.cache
    }

    pub fn send_message_to_initial_targets(
        &mut self,
        conn_infos: &[ConnectionInfo],
        dg_size: usize,
        msg: Bytes,
    ) {
        let token = self.next_msg_token();

        // initially only send to dg_size targets
        for conn_info in conn_infos.iter().take(dg_size) {
            // NetworkBytes is refcounted and cheap to clone.
            self.quic_p2p.send(
                Peer::Node {
                    node_info: conn_info.clone(),
                },
                msg.clone(),
                token,
            );
        }

        self.cache.insert_message(token, conn_infos, dg_size);
    }

    pub fn send_message_to_next_target(
        &mut self,
        msg: Bytes,
        token: Token,
        failed_tgt: SocketAddr,
        log_ident: LogIdent,
    ) {
        if let Some(tgt) = self.cache.target_failed(token, failed_tgt) {
            info!(
                "{} Sending of message ID {} failed; resending...",
                log_ident, token
            );
            self.quic_p2p
                .send(Peer::Node { node_info: tgt }, msg, token);
        } else {
            error!(
                "{} Resending of message ID {} failed too many times; giving up.",
                log_ident, token
            );
        }
    }

    pub fn our_connection_info(&mut self) -> Result<ConnectionInfo, QuicP2pError> {
        self.quic_p2p.our_connection_info()
    }

    pub fn disconnect(&mut self, addr: SocketAddr) {
        self.quic_p2p.disconnect_from(addr)
    }
}

pub struct NetworkBuilder {
    quic_p2p: Builder,
}

impl NetworkBuilder {
    pub fn new(event_tx: Sender<NetworkEvent>) -> Self {
        Self {
            quic_p2p: Builder::new(event_tx),
        }
    }

    pub fn with_config(self, config: NetworkConfig) -> Self {
        Self {
            quic_p2p: self.quic_p2p.with_config(config),
        }
    }

    pub fn build(self) -> Result<NetworkService, QuicP2pError> {
        Ok(NetworkService {
            quic_p2p: self.quic_p2p.build()?,
            cache: Default::default(),
            next_msg_token: 0,
        })
    }
}
