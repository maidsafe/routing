// Copyright 2020 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use super::{comm::Comm, joining::Joining, NodeInfo};
use crate::{
    crypto::{keypair_within_range, name},
    error::Result,
    messages::{BootstrapResponse, Message, Variant, VerifyStatus},
    peer::Peer,
    relocation::{RelocatePayload, SignedRelocateDetails},
    section::EldersInfo,
    timer::Timer,
    DstLocation, MIN_AGE,
};
use futures::future;
use std::{iter, net::SocketAddr, sync::Arc};
use xor_name::Prefix;

// TODO: review if we still need to set a timeout for joining
/// Time after which bootstrap is cancelled (and possibly returnried).
// pub const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(20);

// The bootstrapping stage - node is trying to find the section to join.
pub(crate) struct Bootstrapping {
    pub node_info: NodeInfo,
    relocate_details: Option<SignedRelocateDetails>,
    comm: Comm,
    timer: Timer,
}

impl Bootstrapping {
    pub async fn new(
        relocate_details: Option<SignedRelocateDetails>,
        bootstrap_contacts: Vec<SocketAddr>,
        comm: Comm,
        node_info: NodeInfo,
        timer: Timer,
    ) -> Result<Self> {
        let stage = Self {
            node_info,
            relocate_details,
            comm,
            timer,
        };

        for addr in bootstrap_contacts {
            stage.send_bootstrap_request(addr).await?;
        }

        Ok(stage)
    }

    pub async fn process_message(
        &mut self,
        sender: SocketAddr,
        msg: Message,
    ) -> Result<Option<Joining>> {
        match msg.variant() {
            Variant::BootstrapResponse(response) => {
                msg.verify(iter::empty())
                    .and_then(VerifyStatus::require_full)?;

                match self
                    .handle_bootstrap_response(
                        msg.src().to_sender_node(Some(sender))?,
                        response.clone(),
                    )
                    .await?
                {
                    Some(JoinParams {
                        elders_info,
                        section_key,
                        relocate_payload,
                    }) => {
                        let joining = Joining::new(
                            self.comm.clone(),
                            elders_info,
                            section_key,
                            relocate_payload,
                            self.node_info.clone(),
                            self.timer.clone(),
                        )
                        .await?;

                        Ok(Some(joining))
                    }
                    None => Ok(None),
                }
            }

            Variant::NeighbourInfo { .. }
            | Variant::UserMessage(_)
            | Variant::BouncedUntrustedMessage(_) => {
                debug!("Unknown message from {}: {:?} ", sender, msg);
                Ok(None)
            }

            Variant::NodeApproval(_)
            | Variant::Sync { .. }
            | Variant::Relocate(_)
            | Variant::RelocatePromise(_)
            | Variant::BootstrapRequest(_)
            | Variant::JoinRequest(_)
            | Variant::BouncedUnknownMessage { .. }
            | Variant::Vote { .. }
            | Variant::DKGResult { .. }
            | Variant::DKGStart { .. }
            | Variant::DKGMessage { .. } => {
                debug!("Useless message from {}: {:?}", sender, msg);
                Ok(None)
            }
        }
    }

    pub async fn process_timeout(&mut self, _token: u64) -> Result<()> {
        todo!()
    }

    async fn handle_bootstrap_response(
        &mut self,
        sender: Peer,
        response: BootstrapResponse,
    ) -> Result<Option<JoinParams>> {
        match response {
            BootstrapResponse::Join {
                elders_info,
                section_key,
            } => {
                info!(
                    "Joining a section {:?} (given by {:?})",
                    elders_info, sender
                );

                let relocate_payload = self.join_section(&elders_info)?;
                Ok(Some(JoinParams {
                    elders_info,
                    section_key,
                    relocate_payload,
                }))
            }
            BootstrapResponse::Rebootstrap(new_conn_infos) => {
                info!(
                    "Bootstrapping redirected to another set of peers: {:?}",
                    new_conn_infos
                );
                self.reconnect_to_new_section(new_conn_infos).await?;
                Ok(None)
            }
        }
    }

    async fn send_bootstrap_request(&self, dst: SocketAddr) -> Result<()> {
        let destination = match &self.relocate_details {
            Some(details) => *details.destination(),
            None => self.node_info.name(),
        };

        let message = Message::single_src(
            &self.node_info.keypair,
            MIN_AGE,
            DstLocation::Direct,
            Variant::BootstrapRequest(destination),
            None,
            None,
        )?;

        debug!("Sending BootstrapRequest to {}", dst);
        self.comm
            .send_message_to_target(&dst, message.to_bytes())
            .await?;

        Ok(())
    }

    async fn reconnect_to_new_section(&self, new_conn_infos: Vec<SocketAddr>) -> Result<()> {
        future::try_join_all(
            new_conn_infos
                .into_iter()
                .map(|addr| self.send_bootstrap_request(addr)),
        )
        .await
        .map(|_| ())
    }

    fn join_section(&mut self, elders_info: &EldersInfo) -> Result<Option<RelocatePayload>> {
        let relocate_details = if let Some(details) = self.relocate_details.take() {
            details
        } else {
            return Ok(None);
        };

        // We are relocating so we need to change our name.
        // Use a name that will match the destination even after multiple splits
        let extra_split_count = 3;
        let name_prefix = Prefix::new(
            elders_info.prefix.bit_count() + extra_split_count,
            *relocate_details.destination(),
        );

        // FIXME: do we need to reuse MainRng everywhere really??
        // This will currently break tests.
        let mut rng = crate::rng::MainRng::default();
        let new_keypair = keypair_within_range(&mut rng, &name_prefix.range_inclusive());
        let new_name = name(&new_keypair.public);
        let relocate_payload =
            RelocatePayload::new(relocate_details, &new_name, &self.node_info.keypair)?;

        info!("Changing name to {}.", new_name);
        self.node_info.keypair = Arc::new(new_keypair);

        Ok(Some(relocate_payload))
    }
}

pub(crate) struct JoinParams {
    pub elders_info: EldersInfo,
    pub section_key: bls::PublicKey,
    pub relocate_payload: Option<RelocatePayload>,
}
