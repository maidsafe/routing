// Copyright 2020 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.
// Copyright 2019 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use crate::{
    consensus::{self, Proof, Proven},
    error::Result,
    id::{FullId, P2pNode},
    messages::{AccumulatingMessage, Message, MessageWithBytes},
    node::Node,
    quic_p2p::{EventSenders, Peer, QuicP2p},
    rng::MainRng,
    section::EldersInfo,
    TransportConfig, TransportEvent,
};
use crossbeam_channel::Receiver;

use itertools::Itertools;
use mock_quic_p2p::Network;
use serde::Serialize;
use std::{collections::BTreeMap, net::SocketAddr};
use xor_name::{Prefix, XorName};

pub fn create_elders_info(
    rng: &mut MainRng,
    network: &Network,
    elder_size: usize,
) -> (Proven<EldersInfo>, BTreeMap<XorName, FullId>) {
    let full_ids: BTreeMap<_, _> = (0..elder_size)
        .map(|_| {
            let id = FullId::gen(rng);
            (*id.public_id().name(), id)
        })
        .collect();

    let members_map: BTreeMap<_, _> = full_ids
        .iter()
        .map(|(name, full_id)| {
            let node = P2pNode::new(*full_id.public_id(), network.gen_addr());
            (*name, node)
        })
        .collect();

    let elders_info = EldersInfo::new(members_map, Prefix::default());

    let sk = consensus::test_utils::gen_secret_key(rng);
    let elders_info = consensus::test_utils::proven(&sk, elders_info);

    (elders_info, full_ids)
}

pub fn create_proof<T: Serialize>(sk_set: &bls::SecretKeySet, payload: &T) -> Proof {
    let pk_set = sk_set.public_keys();
    let bytes = bincode::serialize(payload).unwrap();
    let signature_shares: Vec<_> = (0..sk_set.threshold() + 1)
        .map(|index| sk_set.secret_key_share(index).sign(&bytes))
        .collect();
    let signature = pk_set
        .combine_signatures(signature_shares.iter().enumerate())
        .unwrap();

    Proof {
        public_key: pk_set.public_key(),
        signature,
    }
}

pub fn handle_message(node: &mut Node, sender: SocketAddr, msg: Message) -> Result<()> {
    let msg = MessageWithBytes::new(msg)?;
    node.try_handle_message(sender, msg)?;
    node.handle_messages();
    Ok(())
}

pub fn accumulate_messages<I>(accumulating_msgs: I) -> Message
where
    I: IntoIterator<Item = AccumulatingMessage>,
{
    accumulating_msgs
        .into_iter()
        .fold1(|mut msg0, msg1| {
            msg0.add_signature_shares(msg1);
            msg0
        })
        .expect("there are no messages to accumulate")
        .combine_signatures()
        .expect("failed to combine signatures")
}

pub struct MockTransport {
    _inner: QuicP2p,
    rx: Receiver<TransportEvent>,
    addr: SocketAddr,
}

impl MockTransport {
    pub fn new(addr: Option<&SocketAddr>) -> Self {
        let (tx, rx) = {
            let (client_tx, _) = crossbeam_channel::unbounded();
            let (node_tx, node_rx) = crossbeam_channel::unbounded();
            (EventSenders { node_tx, client_tx }, node_rx)
        };

        let config = addr.map(|addr| TransportConfig {
            ip: Some(addr.ip()),
            port: Some(addr.port()),
            ..Default::default()
        });

        let mut inner = QuicP2p::with_config(tx, config, Default::default(), false).unwrap();
        let addr = inner.our_connection_info().unwrap();

        Self {
            _inner: inner,
            rx,
            addr,
        }
    }

    pub fn addr(&self) -> &SocketAddr {
        &self.addr
    }

    pub fn received_messages(&self) -> impl Iterator<Item = (SocketAddr, Message)> + '_ {
        self.rx.try_iter().filter_map(|event| match event {
            TransportEvent::NewMessage {
                peer: Peer::Node(addr),
                msg,
            } => Some((addr, Message::from_bytes(&msg).unwrap())),
            _ => None,
        })
    }
}
