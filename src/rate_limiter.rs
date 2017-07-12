// Copyright 2017 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement.  This, along with the Licenses can be
// found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

use data::{MAX_IMMUTABLE_DATA_SIZE_IN_BYTES, MAX_MUTABLE_DATA_SIZE_IN_BYTES};
use error::RoutingError;
#[cfg(feature = "use-mock-crust")]
use fake_clock::FakeClock as Instant;
use itertools::Itertools;
use maidsafe_utilities::serialisation::{self, SerialisationError};
use messages::UserMessage;
use sha3::Digest256;
use std::cmp;
use std::collections::BTreeMap;
use std::mem;
use std::net::IpAddr;
#[cfg(not(feature = "use-mock-crust"))]
use std::time::Instant;

/// Maximum total bytes the `RateLimiter` allows at any given moment.
pub const MAX_CLIENTS_PER_PROXY: u64 = CAPACITY / MAX_IMMUTABLE_DATA_SIZE_IN_BYTES;

/// Maximum total bytes the `RateLimiter` allows at any given moment.
///
/// This cannot be less than `MAX_IMMUTABLE_DATA_SIZE_IN_BYTES` else none of the clients would be
/// would be able to do a `GET` operation.
/// This is (1 `MiB` + 10 `KiB`) * 10 => 10 times the `MAX_IMMUTABLE_DATA_SIZE_IN_BYTES`
const CAPACITY: u64 = 10588160;
/// The number of bytes per second the `RateLimiter` will "leak".
const RATE: f64 = CAPACITY as f64 * 1.0;

#[cfg(feature = "use-mock-crust")]
#[doc(hidden)]
pub mod rate_limiter_consts {
    pub const MAX_CLIENTS_PER_PROXY: usize = super::MAX_CLIENTS_PER_PROXY as usize;
    pub const CAPACITY: u64 = super::CAPACITY;
    pub const RATE: f64 = super::RATE;
}

/// Used to throttle the rate at which clients can send messages via this node. It works on a "leaky
/// bucket" principle: there is a set rate at which bytes will leak out of the bucket, there is a
/// maximum capacity for the bucket, and connected clients each get an equal share of this capacity.
#[derive(Debug)]
pub struct RateLimiter {
    /// Map of client IP address to their total bytes remaining in the `RateLimiter`.
    used: BTreeMap<IpAddr, u64>,
    /// Timestamp of when the `RateLimiter` was last updated.
    last_updated: Instant,
}

impl RateLimiter {
    pub fn new() -> Self {
        RateLimiter {
            used: BTreeMap::new(),
            last_updated: Instant::now(),
        }
    }

    /// Try to add a message. If the message is a form of get request, `CLIENT_GET_CHARGE` bytes
    /// will be used, otherwise the actual length of the `payload` will be used. If adding that
    /// amount will cause the client to exceed its share of the `CAPACITY` or cause the total
    /// `CAPACITY` to be exceeded, `Err(ExceedsRateLimit)` is returned. If the message is invalid,
    /// `Err(InvalidMessage)` is returned (this probably indicates malicious behaviour).
    pub fn add_message(&mut self,
                       online_clients: u64,
                       client_ip: &IpAddr,
                       hash: &Digest256,
                       part_count: u32,
                       part_index: u32,
                       payload: &[u8])
                       -> Result<u64, RoutingError> {
        self.update();
        let total_used: u64 = self.used.values().sum();

        let used = self.used.get(client_ip).map_or(0, |used| *used);

        let allowance = cmp::min(CAPACITY - total_used,
                                 (CAPACITY / online_clients).saturating_sub(used));

        let bytes_to_add = if part_index == 0 {
            use self::UserMessage::*;
            use Request::*;
            match serialisation::deserialise::<UserMessage>(payload) {
                Ok(Request(request)) => {
                    if part_count > 1 {
                        return Err(RoutingError::InvalidMessage);
                    }
                    match request {
                        GetIData { .. } => MAX_IMMUTABLE_DATA_SIZE_IN_BYTES,
                        GetAccountInfo { .. } |
                        GetMData { .. } |
                        GetMDataVersion { .. } |
                        GetMDataShell { .. } |
                        ListMDataEntries { .. } |
                        ListMDataKeys { .. } |
                        ListMDataValues { .. } |
                        GetMDataValue { .. } |
                        ListMDataPermissions { .. } |
                        ListMDataUserPermissions { .. } |
                        ListAuthKeysAndVersion { .. } => MAX_MUTABLE_DATA_SIZE_IN_BYTES,
                        PutIData { .. } |
                        PutMData { .. } |
                        MutateMDataEntries { .. } |
                        DeleteMDataEntries { .. } |
                        SetMDataUserPermissions { .. } |
                        DelMDataUserPermissions { .. } |
                        ChangeMDataOwner { .. } |
                        InsAuthKey { .. } |
                        DelAuthKey { .. } => payload.len() as u64,
                        Refresh(..) => return Err(RoutingError::InvalidMessage),
                    }
                }
                Ok(Response(_)) => return Err(RoutingError::InvalidMessage),
                Err(SerialisationError::DeserialiseExtraBytes) => {
                    return Err(RoutingError::InvalidMessage);
                }
                Err(_) => {
                    if part_count == 1 {
                        return Err(RoutingError::InvalidMessage);
                    }
                    payload.len() as u64
                }
            }
        } else {
            payload.len() as u64
        };

        if bytes_to_add > allowance {
            return Err(RoutingError::ExceedsRateLimit(*hash));
        }

        let _ = self.used.insert(*client_ip, used + bytes_to_add);
        Ok(bytes_to_add)
    }

    fn update(&mut self) {
        // If there's nothing else to update, set the timestamp and return.
        if self.used.is_empty() {
            self.last_updated = Instant::now();
            return;
        }

        let now = Instant::now();
        let leak_time = (now - self.last_updated).as_secs() as f64 +
                        ((now - self.last_updated).subsec_nanos() as f64 / 1_000_000_000.0);
        self.last_updated = now;
        let mut leaked_units = (RATE * leak_time) as u64;

        // Sort entries by least-used to most-used and leak each client's quota. For any client
        // which doesn't need its full quota, the unused portion is equally distributed amongst the
        // others.
        let leaking_client_count = self.used.len();
        let mut entries = mem::replace(&mut self.used, Default::default())
            .into_iter()
            .map(|(ip_addr, used)| (used, ip_addr))
            .collect_vec();
        entries.sort();
        for (index, (used, client)) in entries.into_iter().enumerate() {
            let quota = cmp::min(used, leaked_units / (leaking_client_count - index) as u64);
            leaked_units -= quota;
            if used > quota {
                let _ = self.used.insert(client, used - quota);
            }
        }
    }

    #[cfg(feature = "use-mock-crust")]
    pub fn get_clients_usage(&self) -> BTreeMap<IpAddr, u64> {
        self.used.clone()
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(test, feature = "use-mock-crust"))]
mod tests {
    use super::*;
    use fake_clock::FakeClock;
    use messages::Request;
    use rand;
    use tiny_keccak::sha3_256;
    use types::MessageId;

    #[test]
    fn add_message() {
        // First client fills the `RateLimiter` with get requests.
        let mut rate_limiter = RateLimiter::new();
        let client_1 = IpAddr::from([0, 0, 0, 0]);

        let get_req_payload = UserMessage::Request(Request::GetIData {
                                                       name: rand::random(),
                                                       msg_id: MessageId::new(),
                                                   });
        let get_req_payload = unwrap!(serialisation::serialise(&get_req_payload));

        let hash = sha3_256(&get_req_payload);
        let fill_full_iterations = CAPACITY / MAX_IMMUTABLE_DATA_SIZE_IN_BYTES;
        for _ in 0..fill_full_iterations {
            let _ = unwrap!(rate_limiter.add_message(1, &client_1, &hash, 1, 0, &get_req_payload));
        }

        // Check a second client can't add a message just now.
        let client_2 = IpAddr::from([1, 1, 1, 1]);
        match rate_limiter.add_message(1, &client_2, &hash, 1, 0, &get_req_payload) {
            Err(RoutingError::ExceedsRateLimit(returned_hash)) => {
                assert_eq!(hash, returned_hash);
            }
            _ => panic!("unexpected result"),
        }

        // We're waiting until enough has drained to allow the proxy to allow a GET request
        let wait_millis = MAX_IMMUTABLE_DATA_SIZE_IN_BYTES * 1000 / RATE as u64;
        FakeClock::advance_time(wait_millis);

        // Now we consume that final GET allowance from the proxy total allowance.
        let _ = unwrap!(rate_limiter.add_message(2, &client_2, &hash, 1, 0, &get_req_payload));

        // Now however even with the same time elapsed, the proxy has two clients(1 and 2)
        // each of whom will be given the drained amount equally.
        FakeClock::advance_time(wait_millis);

        // Now Client 2 will still have a used amount of 500KiB and thus get rejected by the
        // proxy as the online clients enforces each client to only be allowed 1MiB
        match rate_limiter.add_message(MAX_CLIENTS_PER_PROXY,
                                       &client_2,
                                       &hash,
                                       1,
                                       0,
                                       &get_req_payload) {
            Err(RoutingError::ExceedsRateLimit(returned_hash)) => {
                assert_eq!(hash, returned_hash);
            }
            _ => panic!("unexpected result"),
        }

        // Try adding invalid messages.
        let all_zero_payload = vec![0u8; MAX_IMMUTABLE_DATA_SIZE_IN_BYTES as usize];
        match rate_limiter.add_message(MAX_CLIENTS_PER_PROXY,
                                       &client_2,
                                       &sha3_256(&all_zero_payload),
                                       2,
                                       0,
                                       &all_zero_payload) {
            Err(RoutingError::InvalidMessage) => {}
            _ => panic!("unexpected result"),
        }
        // Try making the second client exceed its own usage cap.
        match rate_limiter.add_message(MAX_CLIENTS_PER_PROXY,
                                       &client_2,
                                       &hash,
                                       1,
                                       0,
                                       &get_req_payload) {
            Err(RoutingError::ExceedsRateLimit(returned_hash)) => {
                assert_eq!(hash, returned_hash);
            }
            _ => panic!("unexpected result"),
        }
        // More request from the second client with expanded per-client usage cap.
        let _ = unwrap!(rate_limiter.add_message(2, &client_2, &hash, 1, 0, &get_req_payload));

        // Wait for the same period, and push up the second client's usage.
        FakeClock::advance_time(wait_millis);
        let _ = unwrap!(rate_limiter.add_message(2, &client_2, &hash, 1, 0, &get_req_payload));
        // Wait for the same period to drain the second client's usage to less than per-client cap.
        FakeClock::advance_time(wait_millis);
        match rate_limiter.add_message(MAX_CLIENTS_PER_PROXY,
                                       &client_2,
                                       &hash,
                                       1,
                                       0,
                                       &get_req_payload) {
            Err(RoutingError::ExceedsRateLimit(returned_hash)) => {
                assert_eq!(hash, returned_hash);
            }
            _ => panic!("unexpected result"),
        }
    }
}
