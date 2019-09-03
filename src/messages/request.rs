// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use crate::data::{EntryAction, ImmutableData, MutableData, PermissionSet, User};
use crate::types::MessageId as MsgId;
use crate::xor_name::XorName;
use crate::ed25519::PublicKey;
use std::collections::{BTreeMap, BTreeSet};

/// Request message types
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum Request {
    /// Represents a refresh message sent between vaults. Vec<u8> is the message content.
    Refresh(Vec<u8>, MsgId),
    /// Gets MAID account information.
    GetAccountInfo(MsgId),

    // --- ImmutableData ---
    // ==========================
    /// Puts ImmutableData to the network.
    PutIData {
        /// ImmutableData to be stored
        data: ImmutableData,
        /// Unique message identifier
        msg_id: MsgId,
    },
    /// Fetches ImmutableData from the network by the given name.
    GetIData {
        /// Network identifier of ImmutableData
        name: XorName,
        /// Unique message identifier
        msg_id: MsgId,
    },

    // --- MutableData ---
    /// Fetches whole MutableData from the network.
    /// Note: responses to this request are unlikely to accumulate during churn.
    GetMData {
        /// Network identifier of MutableData
        name: XorName,
        /// Type tag
        tag: u64,
        /// Unique message identifier
        msg_id: MsgId,
    },
    // ==========================
    /// Creates a new MutableData in the network.
    PutMData {
        /// MutableData to be stored
        data: MutableData,
        /// Unique message identifier
        msg_id: MsgId,
        /// Requester public key
        requester: PublicKey,
    },
    /// Fetches a latest version number.
    GetMDataVersion {
        /// Network identifier of MutableData
        name: XorName,
        /// Type tag
        tag: u64,
        /// Unique message identifier
        msg_id: MsgId,
    },
    /// Fetches the shell (everything except the entries).
    GetMDataShell {
        /// Network identifier of MutableData
        name: XorName,
        /// Type tag
        tag: u64,
        /// Unique message identifier
        msg_id: MsgId,
    },

    // Data Actions
    /// Fetches a list of entries (keys + values).
    /// Note: responses to this request are unlikely to accumulate during churn.
    ListMDataEntries {
        /// Network identifier of MutableData
        name: XorName,
        /// Type tag
        tag: u64,
        /// Unique message identifier
        msg_id: MsgId,
    },
    /// Fetches a list of keys in MutableData.
    /// Note: responses to this request are unlikely to accumulate during churn.
    ListMDataKeys {
        /// Network identifier of MutableData
        name: XorName,
        /// Type tag
        tag: u64,
        /// Unique message identifier
        msg_id: MsgId,
    },
    /// Fetches a list of values in MutableData.
    /// Note: responses to this request are unlikely to accumulate during churn.
    ListMDataValues {
        /// Network identifier of MutableData
        name: XorName,
        /// Type tag
        tag: u64,
        /// Unique message identifier
        msg_id: MsgId,
    },
    /// Fetches a single value from MutableData
    GetMDataValue {
        /// Network identifier of MutableData
        name: XorName,
        /// Type tag
        tag: u64,
        /// Key of an entry to be fetched
        key: Vec<u8>,
        /// Unique message identifier
        msg_id: MsgId,
    },
    /// Updates MutableData entries in bulk.
    MutateMDataEntries {
        /// Network identifier of MutableData
        name: XorName,
        /// Type tag
        tag: u64,
        /// A list of mutations (inserts, updates, or deletes) to be performed
        /// on MutableData in bulk.
        actions: BTreeMap<Vec<u8>, EntryAction>,
        /// Unique message identifier
        msg_id: MsgId,
        /// Requester public key
        requester: PublicKey,
    },

    // Permission Actions
    /// Fetches a complete list of permissions.
    ListMDataPermissions {
        /// Network identifier of MutableData
        name: XorName,
        /// Type tag
        tag: u64,
        /// Unique message identifier
        msg_id: MsgId,
    },
    /// Fetches a list of permissions for a particular User.
    ListMDataUserPermissions {
        /// Network identifier of MutableData
        name: XorName,
        /// Type tag
        tag: u64,
        /// A user identifier used to fetch permissions
        user: User,
        /// Unique message identifier
        msg_id: MsgId,
    },
    /// Updates or inserts a list of permissions for a particular User in the given MutableData.
    SetMDataUserPermissions {
        /// Network identifier of MutableData
        name: XorName,
        /// Type tag
        tag: u64,
        /// A user identifier used to set permissions
        user: User,
        /// Permissions to be set for a user
        permissions: PermissionSet,
        /// Incremented version of MutableData
        version: u64,
        /// Unique message identifier
        msg_id: MsgId,
        /// Requester public key
        requester: PublicKey,
    },
    /// Deletes a list of permissions for a particular User in the given MutableData.
    DeleteMDataUserPermissions {
        /// Network identifier of MutableData
        name: XorName,
        /// Type tag
        tag: u64,
        /// A user identifier used to delete permissions
        user: User,
        /// Incremented version of MutableData
        version: u64,
        /// Unique message identifier
        msg_id: MsgId,
        /// Requester public key
        requester: PublicKey,
    },

    // Ownership Actions
    /// Changes an owner of the given MutableData. Only the current owner can perform this action.
    ChangeMDataOwner {
        /// Network identifier of MutableData
        name: XorName,
        /// Type tag
        tag: u64,
        /// A list of new owners
        new_owners: BTreeSet<PublicKey>,
        /// Incremented version of MutableData
        version: u64,
        /// Unique message identifier
        msg_id: MsgId,
    },

    // --- Client (Owner) to MM ---
    // ==========================
    /// Lists authorised keys and version stored in MaidManager.
    ListAuthKeysAndVersion(MsgId),
    /// Inserts an authorised key (for an app, user, etc.) to MaidManager.
    InsertAuthKey {
        /// Authorised key to be inserted
        key: PublicKey,
        /// Incremented version
        version: u64,
        /// Unique message identifier
        msg_id: MsgId,
    },
    /// Deletes an authorised key from MaidManager.
    DeleteAuthKey {
        /// Authorised key to be deleted
        key: PublicKey,
        /// Incremented version
        version: u64,
        /// Unique message identifier
        msg_id: MsgId,
    },
}

impl Request {
    /// Message ID getter.
    pub fn message_id(&self) -> &MsgId {
        use crate::Request::*;
        match *self {
            Refresh(_, ref msg_id)
            | GetAccountInfo(ref msg_id)
            | PutIData { ref msg_id, .. }
            | GetIData { ref msg_id, .. }
            | GetMData { ref msg_id, .. }
            | PutMData { ref msg_id, .. }
            | GetMDataVersion { ref msg_id, .. }
            | GetMDataShell { ref msg_id, .. }
            | ListMDataEntries { ref msg_id, .. }
            | ListMDataKeys { ref msg_id, .. }
            | ListMDataValues { ref msg_id, .. }
            | GetMDataValue { ref msg_id, .. }
            | MutateMDataEntries { ref msg_id, .. }
            | ListMDataPermissions { ref msg_id, .. }
            | ListMDataUserPermissions { ref msg_id, .. }
            | SetMDataUserPermissions { ref msg_id, .. }
            | DeleteMDataUserPermissions { ref msg_id, .. }
            | ChangeMDataOwner { ref msg_id, .. }
            | ListAuthKeysAndVersion(ref msg_id)
            | InsertAuthKey { ref msg_id, .. }
            | DeleteAuthKey { ref msg_id, .. } => msg_id,
        }
    }

    /// Is the response corresponding to this request cacheable?
    pub fn is_cacheable(&self) -> bool {
        if let Request::GetIData { .. } = *self {
            true
        } else {
            false
        }
    }
}
