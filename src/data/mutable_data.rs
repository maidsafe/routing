// Copyright 2016 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.1.  This, along with the
// Licenses can be found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

use error::RoutingError;
use maidsafe_utilities::serialisation::serialised_size;
use rust_sodium::crypto::sign::PublicKey;
use std::collections::BTreeSet;
use std::collections::btree_map::{BTreeMap, Entry};
use std::fmt::{self, Debug, Formatter};
use super::DataIdentifier;
use xor_name::XorName;

/// Maximum allowed size for MutableData (1 MiB)
pub const MAX_MUTABLE_DATA_SIZE_IN_BYTES: u64 = 1024 * 1024;

/// Maximum allowed entries in MutableData
pub const MAX_MUTABLE_DATA_ENTRIES: u64 = 100;

/// Mutable data.
#[derive(Hash, Eq, PartialEq, PartialOrd, Ord, Clone, RustcDecodable, RustcEncodable)]
pub struct MutableData {
    /// Network address
    name: XorName,
    /// Type tag
    tag: u64,
    // ---- owner and vault access only ----
    /// Maps an arbitrary key to a (version, data) tuple value
    data: BTreeMap<Vec<u8>, Value>,
    /// Maps an application key to a list of allowed or forbidden actions
    permissions: BTreeMap<User, PermissionSet>,
    /// Version should be increased for every change in MutableData fields
    /// except for data
    version: u64,
    /// Contains a set of owners which are allowed to mutate permissions.
    /// Currently limited to one owner to disallow multisig.
    owners: BTreeSet<PublicKey>,
}

/// A value in MutableData
#[derive(Hash, Eq, PartialEq, PartialOrd, Ord, Clone, RustcDecodable, RustcEncodable)]
pub struct Value {
    content: Vec<u8>,
    entry_version: u64,
}

#[derive(Hash, Eq, PartialEq, PartialOrd, Ord, Clone, RustcDecodable, RustcEncodable)]
pub enum User {
    Anyone,
    Key(PublicKey),
}

#[derive(Hash, Eq, PartialEq, PartialOrd, Ord, Copy, Clone, RustcEncodable, RustcDecodable)]
pub enum Action {
    Insert,
    Update,
    Delete,
    ManagePermission,
}

#[derive(Hash, Eq, PartialEq, PartialOrd, Ord, Clone, RustcEncodable, RustcDecodable)]
pub struct PermissionSet(BTreeMap<Action, bool>);

impl PermissionSet {
    pub fn new() -> PermissionSet {
        PermissionSet(BTreeMap::new())
    }

    pub fn allow(&mut self, action: Action) -> &mut PermissionSet {
        let _ = self.0.insert(action, true);
        self
    }

    pub fn deny(&mut self, action: Action) -> &mut PermissionSet {
        let _ = self.0.insert(action, false);
        self
    }

    pub fn clear(&mut self, action: Action) -> &mut PermissionSet {
        let _ = self.0.remove(&action);
        self
    }

    pub fn is_allowed(&self, action: Action) -> Option<bool> {
        self.0.get(&action).cloned()
    }
}

#[derive(Hash, Eq, PartialEq, Clone, PartialOrd, Ord)]
pub enum EntryAction {
    /// Inserts a new entry
    Ins(Value),
    /// Updates an entry with a new value and version
    Update(Value),
    /// Deletes an entry by emptying its contents. Contains the version number
    Del(u64),
}

impl MutableData {
    /// Creates a new MutableData
    pub fn new(name: XorName,
               tag: u64,
               permissions: BTreeMap<User, PermissionSet>,
               data: BTreeMap<Vec<u8>, Value>,
               owners: BTreeSet<PublicKey>)
               -> Result<MutableData, RoutingError> {
        if owners.len() > 1 {
            return Err(RoutingError::InvalidOwners);
        }
        if data.len() >= (MAX_MUTABLE_DATA_ENTRIES + 1) as usize {
            return Err(RoutingError::TooManyEntries);
        }

        let md = MutableData {
            name: name,
            tag: tag,
            data: data,
            permissions: permissions,
            version: 0,
            owners: owners,
        };

        if serialised_size(&md) > MAX_MUTABLE_DATA_SIZE_IN_BYTES {
            return Err(RoutingError::ExceededSizeLimit);
        }

        Ok(md)
    }

    /// Returns the name.
    pub fn name(&self) -> &XorName {
        &self.name
    }

    /// Returns `DataIdentifier` for this data element.
    pub fn identifier(&self) -> DataIdentifier {
        DataIdentifier::Mutable(self.name)
    }

    /// Returns the type tag of this MutableData
    pub fn tag(&self) -> u64 {
        self.tag
    }

    /// Returns the current version of this MutableData
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Returns a value by the given key
    pub fn get(&self, key: &Vec<u8>) -> Option<&Value> {
        self.data.get(key)
    }

    /// Returns keys of all entries
    pub fn keys(&self) -> BTreeSet<&Vec<u8>> {
        self.data.keys().collect()
    }

    /// Returns values of all entries
    pub fn values(&self) -> Vec<&Value> {
        self.data.values().collect()
    }

    /// Returns all entries
    pub fn entries(&self) -> &BTreeMap<Vec<u8>, Value> {
        &self.data
    }

    fn rollback(&mut self,
                inserted: BTreeSet<Vec<u8>>,
                updated: BTreeMap<Vec<u8>, Value>,
                deleted: BTreeMap<Vec<u8>, Value>) {
        for (key, val) in deleted {
            let _ = self.data.insert(key, val);
        }
        for (key, val) in updated {
            let _ = self.data.insert(key, val);
        }
        for key in inserted {
            let _ = self.data.remove(&key);
        }
    }

    /// Mutates entries (key + value pairs) in bulk
    pub fn mutate_entries(&mut self,
                          actions: BTreeMap<Vec<u8>, EntryAction>,
                          requester: PublicKey)
                          -> Result<(), RoutingError> {
        // Deconstruct actions into inserts, updates, and deletes
        let (insert, update, delete) = actions.into_iter()
            .fold((BTreeMap::new(), BTreeMap::new(), BTreeMap::new()),
                  |(mut insert, mut update, mut delete), (key, item)| {
                match item {
                    EntryAction::Ins(value) => {
                        let _ = insert.insert(key, value);
                    }
                    EntryAction::Update(value) => {
                        let _ = update.insert(key, value);
                    }
                    EntryAction::Del(version) => {
                        let _ = delete.insert(key, version);
                    }
                };
                (insert, update, delete)
            });

        if (insert.len() > 0 && !self.is_action_allowed(requester, Action::Insert)) ||
           (update.len() > 0 && !self.is_action_allowed(requester, Action::Update)) ||
           (delete.len() > 0 && !self.is_action_allowed(requester, Action::Delete)) {
            return Err(RoutingError::AccessDenied);
        }
        if (insert.len() > 0 || update.len() > 0) &&
           self.data.len() > MAX_MUTABLE_DATA_ENTRIES as usize {
            return Err(RoutingError::TooManyEntries);
        }

        let mut inserted: BTreeSet<Vec<u8>> = BTreeSet::new();
        let mut updated: BTreeMap<Vec<u8>, Value> = BTreeMap::new();
        let mut deleted: BTreeMap<Vec<u8>, Value> = BTreeMap::new();

        for (key, val) in insert {
            if self.data.contains_key(&key) {
                self.rollback(inserted, updated, deleted);
                return Err(RoutingError::EntryAlreadyExist);
            }
            let _ = self.data.insert(key.clone(), val);
            inserted.insert(key);
        }

        for (key, val) in update {
            if !self.data.contains_key(&key) {
                self.rollback(inserted, updated, deleted);
                return Err(RoutingError::EntryNotFound);
            }
            let version_valid = if let Entry::Occupied(mut oe) = self.data.entry(key.clone()) {
                if val.entry_version != oe.get().entry_version + 1 {
                    false
                } else {
                    let prev = oe.insert(val);
                    let _ = updated.insert(key, prev);
                    true
                }
            } else {
                false
            };
            if !version_valid {
                self.rollback(inserted, updated, deleted);
                return Err(RoutingError::InvalidSuccessor);
            }
        }

        for (key, version) in delete {
            if !self.data.contains_key(&key) {
                self.rollback(inserted, updated, deleted);
                return Err(RoutingError::EntryNotFound);
            }
            let version_valid = if let Entry::Occupied(oe) = self.data.entry(key.clone()) {
                // let prev = oe.insert(Value { content: vec![], entry_version: version });
                if version != oe.get().entry_version + 1 {
                    false
                } else {
                    let (key, prev) = oe.remove_entry();
                    let _ = deleted.insert(key, prev);
                    true
                }
            } else {
                false
            };
            if !version_valid {
                self.rollback(inserted, updated, deleted);
                return Err(RoutingError::InvalidSuccessor);
            }
        }

        if !self.validate_mut_size() {
            self.rollback(inserted, updated, deleted);
            return Err(RoutingError::ExceededSizeLimit);
        }

        Ok(())
    }

    /// Insert or update permissions for the provided user.
    pub fn set_user_permissions(&mut self,
                                user: User,
                                permissions: PermissionSet,
                                requester: PublicKey)
                                -> Result<(), RoutingError> {
        if !self.is_action_allowed(requester, Action::ManagePermission) {
            return Err(RoutingError::AccessDenied);
        }
        let prev = self.permissions.insert(user.clone(), permissions);
        if !self.validate_mut_size() {
            // Serialised data size limit is exceeded
            let _ = match prev {
                None => self.permissions.remove(&user),
                Some(perms) => self.permissions.insert(user, perms),
            };
            return Err(RoutingError::ExceededSizeLimit);
        }
        Ok(())
    }

    /// Delete permissions for the provided user.
    pub fn del_user_permissions(&mut self,
                                user: &User,
                                requester: PublicKey)
                                -> Result<(), RoutingError> {
        if !self.is_action_allowed(requester, Action::ManagePermission) {
            return Err(RoutingError::AccessDenied);
        }
        if !self.permissions.contains_key(user) {
            return Err(RoutingError::EntryNotFound);
        }
        let _ = self.permissions.remove(user);
        Ok(())
    }

    /// Change owner of the mutable data.
    pub fn change_owner(&mut self,
                        new_owner: PublicKey,
                        requester: PublicKey)
                        -> Result<(), RoutingError> {
        if !self.owners.contains(&requester) {
            return Err(RoutingError::AccessDenied);
        }
        self.owners.clear();
        self.owners.insert(new_owner);
        Ok(())
    }

    /// Return true if the size is valid
    pub fn validate_size(&self) -> bool {
        serialised_size(self) <= MAX_MUTABLE_DATA_SIZE_IN_BYTES
    }

    /// Return true if the size is valid after a mutation. We need to have this
    /// because of eventual consistency requirements - in certain cases entries
    /// can go over the default cap of 1 MiB.
    fn validate_mut_size(&self) -> bool {
        serialised_size(self) <= MAX_MUTABLE_DATA_SIZE_IN_BYTES * 2
    }

    fn check_anyone_permissions(&self, action: Action) -> bool {
        match self.permissions.get(&User::Anyone) {
            None => false,
            Some(perms) => perms.is_allowed(action).unwrap_or(false),
        }
    }

    fn is_action_allowed(&self, requester: PublicKey, action: Action) -> bool {
        if self.owners.contains(&requester) {
            return true;
        }
        match self.permissions.get(&User::Key(requester)) {
            Some(perms) => {
                perms.is_allowed(action)
                    .unwrap_or_else(|| self.check_anyone_permissions(action))
            }
            None => self.check_anyone_permissions(action),
        }
    }
}

impl Debug for MutableData {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        // TODO(nbaksalyar): write all other fields
        write!(formatter,
               "MutableData {{ name: {}, tag: {}, version: {}, owners: {:?} }}",
               self.name(),
               self.tag,
               self.version,
               self.owners)
    }
}

#[cfg(test)]
mod tests {
    use rand;
    use rust_sodium::crypto::sign;
    use std::collections::{BTreeMap, BTreeSet};
    use std::iter;
    use error::RoutingError;
    use super::*;

    macro_rules! assert_err {
        ($left: expr, $err: path) => {{
            let result = $left; // required to prevent multiple repeating expansions
            assert!(if let Err($err) = result {
                true
            } else {
                false
            }, "Expected Err({:?}), found {:?}", $err, result);
        }}
    }

    #[test]
    fn mutable_data_permissions() {
        let (owner, _) = sign::gen_keypair();
        let (pk1, _) = sign::gen_keypair();
        let (pk2, _) = sign::gen_keypair();

        let mut perms = BTreeMap::new();

        let mut ps1 = PermissionSet::new();
        let _ = ps1.allow(Action::Update);
        let _ = perms.insert(User::Anyone, ps1);

        let mut ps2 = PermissionSet::new();
        let _ = ps2.deny(Action::Update).allow(Action::Insert);
        let _ = perms.insert(User::Key(pk1), ps2);

        let k1 = "123".as_bytes().to_owned();
        let k2 = "234".as_bytes().to_owned();

        let mut v1 = BTreeMap::new();
        let _ = v1.insert(k1.clone(),
                          EntryAction::Ins(Value {
                              content: "abc".as_bytes().to_owned(),
                              entry_version: 0,
                          }));

        let mut v2 = BTreeMap::new();
        let _ = v2.insert(k2.clone(),
                          EntryAction::Ins(Value {
                              content: "def".as_bytes().to_owned(),
                              entry_version: 0,
                          }));

        let mut owners = BTreeSet::new();
        owners.insert(owner);
        let mut md = unwrap!(MutableData::new(rand::random(), 0, perms, BTreeMap::new(), owners));

        // Check insert permissions
        assert!(md.mutate_entries(v1.clone(), pk1).is_ok());
        assert_err!(md.mutate_entries(v2.clone(), pk2),
                    RoutingError::AccessDenied);

        assert!(md.get(&k1).is_some());
        assert!(md.get(&k2).is_none()); // check that rollback is working

        // Check update permissions
        let _ = v1.insert(k1.clone(),
                          EntryAction::Update(Value {
                              content: "def".as_bytes().to_owned(),
                              entry_version: 1,
                          }));
        assert_err!(md.mutate_entries(v1.clone(), pk1),
                    RoutingError::AccessDenied);
        // check that rollback is working
        assert_eq!(md.get(&k1).unwrap().content, "abc".as_bytes());

        assert!(md.mutate_entries(v1.clone(), pk2).is_ok());

        // Check delete permissions (which should be implicitly forbidden)
        let mut del = BTreeMap::new();
        let _ = del.insert(k1.clone(), EntryAction::Del(2));
        assert_err!(md.mutate_entries(del.clone(), pk1),
                    RoutingError::AccessDenied);
        assert!(md.get(&k1).is_some());

        // Actions requested by owner should always be allowed
        assert!(md.mutate_entries(del, owner).is_ok());
        assert!(md.get(&k1).is_none());
    }

    #[test]
    fn permissions() {
        let mut anyone = PermissionSet::new();
        let _ = anyone.allow(Action::Insert).deny(Action::Delete);
        assert!(unwrap!(anyone.is_allowed(Action::Insert)));
        assert!(anyone.is_allowed(Action::Update).is_none());
        assert!(!unwrap!(anyone.is_allowed(Action::Delete)));
        assert!(anyone.is_allowed(Action::ManagePermission).is_none());

        let mut user1 = anyone;
        let _ = user1.clear(Action::Delete).deny(Action::ManagePermission);
        assert!(unwrap!(user1.is_allowed(Action::Insert)));
        assert!(user1.is_allowed(Action::Update).is_none());
        assert!(user1.is_allowed(Action::Delete).is_none());
        assert!(!unwrap!(user1.is_allowed(Action::ManagePermission)));

        let _ = user1.allow(Action::Update);
        assert!(unwrap!(user1.is_allowed(Action::Insert)));
        assert!(unwrap!(user1.is_allowed(Action::Update)));
        assert!(user1.is_allowed(Action::Delete).is_none());
        assert!(!unwrap!(user1.is_allowed(Action::ManagePermission)));
    }

    #[test]
    fn max_entries_limit() {
        let val = Value {
            content: "123".as_bytes().to_owned(),
            entry_version: 0,
        };

        // It must not be possible to create MutableData with more than 101 entries
        let mut data = BTreeMap::new();
        for i in 0..105 {
            let _ = data.insert(vec![i], val.clone());
        }
        assert_err!(MutableData::new(rand::random(), 0, BTreeMap::new(), data, BTreeSet::new()),
                    RoutingError::TooManyEntries);

        let mut data = BTreeMap::new();
        for i in 0..100 {
            let _ = data.insert(vec![i], val.clone());
        }

        let (owner, _) = sign::gen_keypair();

        let mut owners = BTreeSet::new();
        assert!(owners.insert(owner), true);

        let mut md = unwrap!(MutableData::new(rand::random(), 0, BTreeMap::new(), data, owners));

        assert_eq!(md.keys().len(), 100);
        assert_eq!(md.values().len(), 100);
        assert_eq!(md.entries().len(), 100);

        // Try to get over the limit
        let mut v1 = BTreeMap::new();
        let _ = v1.insert(vec![101u8], EntryAction::Ins(val.clone()));
        assert!(md.mutate_entries(v1, owner).is_ok());

        let mut v2 = BTreeMap::new();
        let _ = v2.insert(vec![102u8], EntryAction::Ins(val.clone()));
        assert_err!(md.mutate_entries(v2.clone(), owner),
                    RoutingError::TooManyEntries);

        let mut del = BTreeMap::new();
        let _ = del.insert(vec![101u8], EntryAction::Del(1));
        assert!(md.mutate_entries(del, owner).is_ok());

        assert!(md.mutate_entries(v2, owner).is_ok());
    }

    #[test]
    fn size_limit() {
        let big_val = Value {
            content: iter::repeat(0)
                .take((MAX_MUTABLE_DATA_SIZE_IN_BYTES - 1024) as usize)
                .collect(),
            entry_version: 0,
        };

        let small_val = Value {
            content: iter::repeat(0).take(2048).collect(),
            entry_version: 0,
        };

        // It must not be possible to create MutableData with size of more than 1 MiB
        let mut data = BTreeMap::new();
        let _ = data.insert(vec![0], big_val.clone());
        let _ = data.insert(vec![1], small_val.clone());

        assert_err!(MutableData::new(rand::random(), 0, BTreeMap::new(), data, BTreeSet::new()),
                    RoutingError::ExceededSizeLimit);

        let mut data = BTreeMap::new();
        let _ = data.insert(vec![0], big_val.clone());

        let (owner, _) = sign::gen_keypair();
        let mut owners = BTreeSet::new();
        assert!(owners.insert(owner), true);

        let mut md = unwrap!(MutableData::new(rand::random(), 0, BTreeMap::new(), data, owners));

        // Try to get over the mutation limit of 2 MiB
        let mut v1 = BTreeMap::new();
        let _ = v1.insert(vec![1], EntryAction::Ins(big_val.clone()));
        assert!(md.mutate_entries(v1, owner).is_ok());

        let mut v2 = BTreeMap::new();
        let _ = v2.insert(vec![2], EntryAction::Ins(small_val.clone()));
        assert_err!(md.mutate_entries(v2.clone(), owner),
                    RoutingError::ExceededSizeLimit);

        let mut del = BTreeMap::new();
        let _ = del.insert(vec![0], EntryAction::Del(1));
        assert!(md.mutate_entries(del, owner).is_ok());

        assert!(md.mutate_entries(v2, owner).is_ok());
    }

    #[test]
    fn transfer_ownership() {
        let (owner, _) = sign::gen_keypair();
        let (pk1, _) = sign::gen_keypair();

        let mut owners = BTreeSet::new();
        owners.insert(owner);

        let mut md =
            unwrap!(MutableData::new(rand::random(), 0, BTreeMap::new(), BTreeMap::new(), owners));

        // Try to do ownership transfer from a non-owner requester
        assert_err!(md.change_owner(pk1, pk1), RoutingError::AccessDenied);

        // Transfer ownership from an owner
        assert!(md.change_owner(pk1, owner).is_ok());
        assert_err!(md.change_owner(owner, owner), RoutingError::AccessDenied);
    }

    #[test]
    fn versions_succession() {
        let (owner, _) = sign::gen_keypair();

        let mut owners = BTreeSet::new();
        owners.insert(owner);
        let mut md =
            unwrap!(MutableData::new(rand::random(), 0, BTreeMap::new(), BTreeMap::new(), owners));

        let mut v1 = BTreeMap::new();
        let _ = v1.insert(vec![1],
                          EntryAction::Ins(Value {
                              content: vec![100],
                              entry_version: 0,
                          }));
        assert!(md.mutate_entries(v1, owner).is_ok());

        // Check update with invalid versions
        let mut v2 = BTreeMap::new();
        let _ = v2.insert(vec![1],
                          EntryAction::Update(Value {
                              content: vec![105],
                              entry_version: 0,
                          }));
        assert_err!(md.mutate_entries(v2.clone(), owner),
                    RoutingError::InvalidSuccessor);

        let _ = v2.insert(vec![1],
                          EntryAction::Update(Value {
                              content: vec![105],
                              entry_version: 2,
                          }));
        assert_err!(md.mutate_entries(v2.clone(), owner),
                    RoutingError::InvalidSuccessor);

        // Check update with a valid version
        let _ = v2.insert(vec![1],
                          EntryAction::Update(Value {
                              content: vec![105],
                              entry_version: 1,
                          }));
        assert!(md.mutate_entries(v2, owner).is_ok());

        // Check delete version
        let mut del = BTreeMap::new();
        let _ = del.insert(vec![1], EntryAction::Del(1));
        assert_err!(md.mutate_entries(del.clone(), owner),
                    RoutingError::InvalidSuccessor);

        let _ = del.insert(vec![1], EntryAction::Del(2));
        assert!(md.mutate_entries(del, owner).is_ok());
    }

    #[test]
    fn changing_permissions() {
        let (owner, _) = sign::gen_keypair();
        let (pk1, _) = sign::gen_keypair();

        let mut owners = BTreeSet::new();
        owners.insert(owner);

        let mut md =
            unwrap!(MutableData::new(rand::random(), 0, BTreeMap::new(), BTreeMap::new(), owners));

        // Trying to do inserts without having a permission must fail
        let mut v1 = BTreeMap::new();
        let _ = v1.insert(vec![0],
                          EntryAction::Ins(Value {
                              content: vec![1],
                              entry_version: 0,
                          }));
        assert_err!(md.mutate_entries(v1.clone(), pk1),
                    RoutingError::AccessDenied);

        // Now allow inserts for pk1
        let mut ps1 = PermissionSet::new();
        let _ = ps1.allow(Action::Insert).allow(Action::ManagePermission);
        assert!(md.set_user_permissions(User::Key(pk1), ps1, owner).is_ok());

        assert!(md.mutate_entries(v1, pk1).is_ok());

        // pk1 now can change permissions
        let mut ps2 = PermissionSet::new();
        let _ = ps2.allow(Action::Insert).deny(Action::ManagePermission);
        assert!(md.set_user_permissions(User::Key(pk1), ps2, pk1).is_ok());

        // Revoke permissions for pk1
        assert_err!(md.del_user_permissions(&User::Key(pk1), pk1),
                    RoutingError::AccessDenied);

        assert!(md.del_user_permissions(&User::Key(pk1), owner).is_ok());

        let mut v2 = BTreeMap::new();
        let _ = v2.insert(vec![1],
                          EntryAction::Ins(Value {
                              content: vec![1],
                              entry_version: 0,
                          }));
        assert_err!(md.mutate_entries(v2, pk1), RoutingError::AccessDenied);

        // Revoking permissions for a non-existing user should return an error
        assert_err!(md.del_user_permissions(&User::Key(pk1), owner),
                    RoutingError::EntryNotFound);

        // Get must always be allowed
        assert!(md.get(&vec![0]).is_some());
        assert!(md.get(&vec![1]).is_none());
    }
}
