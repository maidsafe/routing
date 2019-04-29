// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use crate::chain::SectionInfo;
use crate::error::RoutingError;
use crate::message_filter::MessageFilter;
use crate::messages::RoutingMessage;
use crate::sha3;
use crate::time::{Duration, Instant};
use maidsafe_utilities::serialisation;
use safe_crypto;
use std::collections::BTreeMap;
use std::fmt;

/// Time (in seconds) after which a message is resent due to being unacknowledged by recipient.
pub const ACK_TIMEOUT: Duration = Duration::from_secs(20);

const EXPIRY_DURATION: Duration = Duration::from_secs(4 * 60);

/// A copy of a message which has been sent and is pending the ack from the recipient.
#[derive(Clone, Debug)]
pub struct UnacknowledgedMessage {
    pub routing_msg: RoutingMessage,
    pub src_section: Option<SectionInfo>,
    pub route: u8,
    pub timer_token: u64,
    pub expires_at: Option<Instant>,
}

pub struct AckManager {
    pending: BTreeMap<Ack, UnacknowledgedMessage>,
    received: MessageFilter<Ack>,
}

/// An identifier for a waiting-to-be-acknowledged message (a hash of the message).
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct Ack {
    m_hash: sha3::Digest256,
}

impl AckManager {
    /// Creates a new manager, with empty lists.
    pub fn new() -> Self {
        AckManager {
            pending: BTreeMap::new(),
            received: MessageFilter::with_expiry_duration(EXPIRY_DURATION),
        }
    }

    /// Handles a received ack (removes the corresponding message from the list of
    /// pending ones, and remembers that we have received this ack).
    pub fn receive(&mut self, ack: Ack) {
        let _ack = self.pending.remove(&ack);
        let _ = self.received.insert(&ack);
    }

    /// Did we receive this ack?
    pub fn did_receive(&mut self, ack: Ack) -> bool {
        self.received.contains(&ack)
    }

    /// Adds a pending message; if another with the same `Ack` identifier exists,
    /// this is removed and returned.
    pub fn add_to_pending(
        &mut self,
        ack: Ack,
        unacked_msg: UnacknowledgedMessage,
    ) -> Option<UnacknowledgedMessage> {
        self.pending.insert(ack, unacked_msg)
    }

    // Find a timed out unacknowledged message corresponding to the given timer token.
    // If such message exists, returns it with the corresponding ack hash. Otherwise
    // returns None.
    pub fn find_timed_out(&mut self, token: u64) -> Option<(UnacknowledgedMessage, Ack)> {
        let timed_out_ack = if let Some((sip_hash, _)) = self
            .pending
            .iter()
            .find(|&(_, unacked_msg)| unacked_msg.timer_token == token)
        {
            *sip_hash
        } else {
            return None;
        };

        // Safe to use `unwrap!()` here as we just got a valid key in the `find` call above.
        let mut unacked_msg = unwrap!(self.pending.remove(&timed_out_ack));
        unacked_msg.route += 1;

        Some((unacked_msg, timed_out_ack))
    }

    // Removes a pending `UnacknowledgedMessage` and returns the same if found.
    pub fn remove(&mut self, ack: &Ack) -> Option<UnacknowledgedMessage> {
        self.pending.remove(ack)
    }

    #[cfg(feature = "mock_base")]
    pub fn has_unacked_msg(&self) -> bool {
        !self.pending.is_empty()
    }
}

impl Ack {
    /// Compute an `Ack` from a message.
    pub fn compute(routing_msg: &RoutingMessage) -> Result<Ack, RoutingError> {
        let hash_msg = serialisation::serialise(routing_msg)?;
        Ok(Ack {
            m_hash: safe_crypto::hash(&hash_msg),
        })
    }
}

impl fmt::Debug for Ack {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(
            formatter,
            "Ack({:02x}{:02x}..)",
            self.m_hash[0], self.m_hash[1]
        )
    }
}
