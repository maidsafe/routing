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


// A routing table to manage contacts for a node in a [Kademlia][1] distributed hash table.
//
// [1]: https://en.wikipedia.org/wiki/Kademlia
//
//
// This uses the Kademlia mechanism for routing messages in a peer-to-peer network, and generalises
// it to provide redundancy in every step: for senders, messages in transit and receivers.
// It contains the routing table and the functionality to decide via which of its entries to route
// a message, but not the networking functionality itself.
//
// It also provides methods to decide which other nodes to connect to, depending on a parameter
// `bucket_size` (see below).
//
//
// # Addresses and distance functions
//
// Nodes in the network are addressed with a [`Xorable`][2] type, an unsigned integer with `B` bits.
// The *[XOR][3] distance* between two nodes with addresses `x` and `y` is `x ^ y`. This
// [distance function][4] has the property that no two points ever have the same distance from a
// given point, i. e. if `x ^ y == x ^ z`, then `y == z`. This property allows us to define the
// `k`-*close group* of an address as the `k` closest nodes to that address, guaranteeing that the
// close group will always have exactly `k` members (unless, of course, the whole network has less
// than `k` nodes).
//
// [2]: trait.Xorable.html
// [3]: https://en.wikipedia.org/wiki/Exclusive_or#Bitwise_operation
// [4]: https://en.wikipedia.org/wiki/Metric_%28mathematics%29
//
// The routing table is associated with a node with some name `x`, and manages a number of contacts
// to other nodes, sorting them into up to `B` *buckets*, depending on their XOR distance from `x`:
//
// * If 2<sup>`B`</sup> > `x ^ y` >= 2<sup>`B - 1`</sup>, then y is in bucket 0.
// * If 2<sup>`B - 1`</sup> > `x ^ y` >= 2<sup>`B - 2`</sup>, then y is in bucket 1.
// * If 2<sup>`B - 2`</sup> > `x ^ y` >= 2<sup>`B - 3`</sup>, then y is in bucket 2.
// * ...
// * If 2 > `x ^ y` >= 1, then y is in bucket `B - 1`.
//
// Equivalently, `y` is in bucket `n` if the longest common prefix of `x` and `y` has length `n`,
// i. e. the first binary digit in which `x` and `y` disagree is the `(n + 1)`-th one. We call the
// length of the remainder, without the common prefix, the *bucket distance* of `x` and `y`. Hence
// `x` and `y` have bucket distance `B - n` if and only if `y` belongs in bucket number `n`.
//
// The bucket distance is coarser than the XOR distance: Whenever the bucket distance from `y` to
// `x` is less than the bucket distance from `z` to `x`, then `y ^ x < z ^ x`. But not vice-versa:
// Often `y ^ x < z ^ x`, even if the bucket distances are equal. The XOR distance ranges from 0
// to 2<sup>`B`</sup> (exclusive), while the bucket distance ranges from 0 to `B` (inclusive).
//
//
// # Guarantees
//
// The routing table provides functions to decide, for a message with a given destination, which
// nodes in the table to pass the message on to, so that it is guaranteed that:
//
// * If the destination is the address of a node, the message will reach that node after at most
//   `B - 1` hops.
// * Otherwise, if the destination is a `k`-close group with `k <= bucket_size`, the message will
//   reach every member of the `k`-close group of the destination address, i. e. all `k` nodes in
//   the network that are XOR-closest to that address, and each node knows whether it belongs to
//   that group.
// * Each node in a given address' close group is connected to each other node in that group. In
//   particular, every node is connected to its own close group.
// * The number of total hop messages created for each message is at most `B`.
// * For each node there are at most `B * bucket_size` other nodes in the network that would
//   accept a connection, at any point in time. All other nodes do not need to disclose their IP
//   address.
// * There are `bucket_size` different paths along which a message can be sent, to provide
//   redundancy.
//
// However, to be able to make these guarantees, the routing table must be filled with sufficiently
// many contacts. Specifically, the following invariant must be ensured:
//
// > Whenever a bucket `n` has fewer than `bucket_size` entries, it contains *all* nodes in the
// > network with bucket distance `B - n`.
//
// The user of this crate therefore needs to make sure that whenever a node joins or leaves, all
// affected nodes in the network update their routing tables accordingly.
//
//
// # Resilience against malfunctioning nodes
//
// The sender may choose to send a message via up to `bucket_size` distinct paths to provide
// redundancy against malfunctioning hop nodes. These paths are likely, but not guaranteed, to be
// disjoint.
//
// The concept of close groups exists to provide resilience even against failures of the source or
// destination itself: If every member of a group tries to send the same message, it will arrive
// even if some members fail. And if a message is sent to a whole group, it will arrive in most,
// even if some of them malfunction.
//
// Close groups can thus be used as inherently redundant authorities in the network that messages
// can be sent to and received from, using a consensus algorithm: A message from a group authority
// is considered to be legitimate, if a majority of group members have sent a message with the same
// content.

mod error;
mod network_tests;
mod prefix;
mod xorable;

use itertools::Itertools;
pub use self::error::Error;
#[cfg(any(test, feature = "use-mock-crust"))]
pub use self::network_tests::verify_network_invariant;
pub use self::prefix::Prefix;
pub use self::xorable::Xorable;
use std::{iter, mem};
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet, hash_map, hash_set};
use std::fmt::{Binary, Debug, Formatter};
use std::fmt::Result as FmtResult;
use std::hash::Hash;
use std::thread;

pub type Groups<T> = HashMap<Prefix<T>, HashSet<T>>;

type MemberIter<'a, T> = hash_set::Iter<'a, T>;
type GroupIter<'a, T> = hash_map::Values<'a, Prefix<T>, HashSet<T>>;
type OtherGroupsIter<'a, T> = iter::FlatMap<GroupIter<'a, T>, MemberIter<'a, T>, FlatMapFn<'a, T>>;
type FlatMapFn<'a, T> = fn(&'a HashSet<T>) -> MemberIter<'a, T>;

// Amount added to `min_group_size` when deciding whether a bucket split can happen.  This helps
// protect against rapid splitting and merging in the face of moderate churn.
const SPLIT_BUFFER: usize = 1;

// Immutable iterator over the entries of a `RoutingTable`.
pub struct Iter<'a, T: 'a + Binary + Clone + Copy + Default + Hash + Xorable> {
    inner: iter::Chain<OtherGroupsIter<'a, T>, hash_set::Iter<'a, T>>,
}

impl<'a, T: 'a + Binary + Clone + Copy + Default + Hash + Xorable> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}



/// A message destination.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Destination<N> {
    /// The group closest to the given name.
    Group(N),
    /// The individual node at the given name.
    Node(N),
}

impl<N> Destination<N> {
    /// Returns the name of the destination, i.e. the node or group name.
    pub fn name(&self) -> &N {
        match *self {
            Destination::Group(ref name) |
            Destination::Node(ref name) => name,
        }
    }

    /// Returns `true` if the destination is a group, and `false` if it is an individual node.
    pub fn is_group(&self) -> bool {
        match *self {
            Destination::Group(_) => true,
            Destination::Node(_) => false,
        }
    }

    /// Returns `true` if the destination is an individual node, and `false` if it is a group.
    pub fn is_node(&self) -> bool {
        !self.is_group()
    }
}



// Used when removal of a contact triggers the need to merge two or more groups.  Sent between all
// members of all merging groups, but not peers outwith the new group.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnMergeDetails<T: Binary + Clone + Copy + Default + Hash + Xorable> {
    pub sender_prefix: Prefix<T>,
    pub merge_prefix: Prefix<T>,
    pub groups: Groups<T>,
}



// Used once merging our own group has completed to send to peers outwith the new group
#[derive(Clone, Debug, PartialEq)]
pub struct OtherMergeDetails<T: Binary + Clone + Copy + Default + Hash + Xorable> {
    pub prefix: Prefix<T>,
    pub group: HashSet<T>,
}



// Details returned by a successful `RoutingTable::remove()`.
#[derive(Debug)]
pub struct RemovalDetails<T: Binary + Clone + Copy + Default + Hash + Xorable> {
    // Peer name
    pub name: T,
    // True if the removed peer was in our group.
    pub was_in_our_group: bool,
}



// Details returned by `RoutingTable::merge_own_group()`.
pub enum OwnMergeState<T: Binary + Clone + Copy + Default + Hash + Xorable> {
    // If no ongoing merge is happening when `merge_own_group()` is called, `Initialised` is
    // returned, containing the merge details they each need to receive (the new prefix and all
    // groups in the table).
    Initialised { merge_details: OwnMergeDetails<T> },
    // If an ongoing merge is happening, and this call to `merge_own_group()` doesn't complete the
    // merge (i.e. at least one of the merging groups hasn't yet sent us its merge details), then
    // `Ongoing` is returned, implying that no further action by the caller is required.
    Ongoing,
    // If an ongoing merge is happening, and this call to `merge_own_group()` completes the merge
    // (i.e. all merging groups have sent us their merge details), then `Completed` is returned,
    // containing the appropriate targets (the `Prefix`es of all groups outwith the merging ones)
    // and the merge details they each need to receive (the new prefix and merged group).
    Completed {
        targets: BTreeSet<Prefix<T>>,
        merge_details: OtherMergeDetails<T>,
    },
    // The merge has already completed, implying that no further action by the caller is required.
    AlreadyMerged,
}



/// A routing table to manage contacts for a node.
///
/// It maintains a list of groups (identified by a `Prefix<T>`), each with a
/// list node identifiers of type `T` (e.g. `XorName`) representing connected
/// peer nodes, and provides algorithms for routing messages.
///
/// See the [crate documentation](index.html) for details.
#[derive(Clone, Eq, PartialEq)]
pub struct RoutingTable<T: Binary + Clone + Copy + Debug + Default + Hash + Xorable> {
    our_name: T,
    min_group_size: usize,
    our_group_prefix: Prefix<T>,
    our_group: HashSet<T>,
    groups: Groups<T>,
    // Peers discovered while merging our own group which should be added but aren't yet.
    needed: Groups<T>,
    // While merging our own group, this is the set of merging prefixes with a flag for each
    // indicating whether we have "heard from" that group yet or not (i.e. if
    // `RoutingTable::merge_own_group()` has been called with that group as the sender).
    merging: HashMap<Prefix<T>, bool>,
}

impl<T: Binary + Clone + Copy + Debug + Default + Hash + Xorable> RoutingTable<T> {
    /// Creates a new `RoutingTable`.
    pub fn new(our_name: T, min_group_size: usize) -> Self {
        RoutingTable {
            our_name: our_name,
            min_group_size: min_group_size,
            our_group: HashSet::new(),
            our_group_prefix: Default::default(),
            groups: HashMap::new(),
            needed: HashMap::new(),
            merging: HashMap::new(),
        }
    }

    /// Creates a new `RoutingTable`, using an existing collection of groups.
    pub fn new_with_groups<U, V>(our_name: T,
                                 min_group_size: usize,
                                 new_groups: U)
                                 -> Result<Self, Error>
        where U: IntoIterator<Item = (Prefix<T>, V)>,
              V: IntoIterator<Item = T>
    {
        let mut needed = HashMap::new();
        let mut our_group_prefix = Default::default();
        let groups = new_groups.into_iter()
            .filter_map(|(prefix, members)| {
                let group: HashSet<T> = members.into_iter().collect();
                if !group.is_empty() {
                    let _ = needed.insert(prefix, group);
                }
                if prefix.matches(&our_name) {
                    our_group_prefix = prefix;
                    None
                } else {
                    Some((prefix, HashSet::new()))
                }
            })
            .collect();
        let result = RoutingTable {
            our_name: our_name,
            min_group_size: min_group_size,
            our_group: HashSet::new(),
            our_group_prefix: our_group_prefix,
            groups: groups,
            needed: needed,
            merging: HashMap::new(),
        };
        result.check_invariant()?;
        Ok(result)
    }

    /// Returns the `Prefix` of our group
    pub fn our_group_prefix(&self) -> &Prefix<T> {
        &self.our_group_prefix
    }

    /// Returns our own group, excluding our own name.
    pub fn our_group(&self) -> &HashSet<T> {
        &self.our_group
    }

    /// Returns the total number of entries in the routing table.
    pub fn len(&self) -> usize {
        self.groups.values().fold(0, |acc, group| acc + group.len()) + self.our_group.len()
    }

    /// Is the table empty? (Returns `true` if no nodes are known; empty groups are ignored.)
    pub fn is_empty(&self) -> bool {
        self.our_group.is_empty() && self.groups.values().all(HashSet::is_empty)
    }

    /// Returns whether the table contains the given `name`
    pub fn has(&self, name: &T) -> bool {
        self.get_group(name).map_or(false, |group| group.contains(name))
    }

    /// Iterates over all nodes known by the routing table.
    pub fn iter(&self) -> Iter<T> {
        let iter: fn(_) -> _ = HashSet::iter;
        Iter { inner: self.groups.values().flat_map(iter).chain(&self.our_group) }
    }

    /// Collects prefixes of all groups known by the routing table into a `HashSet`.
    pub fn prefixes(&self) -> BTreeSet<Prefix<T>> {
        self.groups.keys().cloned().chain(iter::once(self.our_group_prefix)).collect()
    }

    /// If our group is the closest one to `name`, returns all names in our group *including ours*,
    /// otherwise returns `None`.
    pub fn close_names(&self, name: &T) -> Option<HashSet<T>> {
        if self.our_group_prefix.matches(name) {
            let mut our_group = self.our_group.clone();
            let _ = our_group.insert(self.our_name);
            Some(our_group)
        } else {
            None
        }
    }

    /// If our group is the closest one to `name`, returns all names in our group *excluding ours*,
    /// otherwise returns `None`.
    pub fn other_close_names(&self, name: &T) -> Option<HashSet<T>> {
        if self.our_group_prefix.matches(name) {
            Some(self.our_group.clone())
        } else {
            None
        }
    }

    /// Returns true if `name` is in our group (including if it is our own name).
    pub fn is_in_our_group(&self, name: &T) -> bool {
        *name == self.our_name || self.our_group.contains(name)
    }

    /// Returns `Ok(())` if the given contact should be added to the routing table.
    ///
    /// Returns `Err` if `name` already exists in the routing table, or it doesn't fall within any
    /// of our groups, or it's our own name.  Otherwise it returns `true`.
    pub fn need_to_add(&self, name: &T) -> Result<(), Error> {
        if *name == self.our_name {
            return Err(Error::OwnNameDisallowed);
        }
        if let Some(group) = self.get_group(name) {
            if group.contains(name) {
                Err(Error::AlreadyExists)
            } else {
                Ok(())
            }
        } else {
            Err(Error::PeerNameUnsuitable)
        }
    }

    /// Returns the collection of groups to which a peer joining our group should connect.
    ///
    /// This will generally be all the groups in our routing table.  However, if the addition of the
    /// new node will cause our group to split, only the groups which will remain neighbours after
    /// the split are returned.  Returns `Err(Error::PeerNameUnsuitable)` if `name` is not within
    /// our group, or `Err(Error::AlreadyExists)` if `name` is already in our table.
    pub fn expect_add_to_our_group(&self, name: &T) -> Result<Groups<T>, Error> {
        if !self.our_group_prefix.matches(name) {
            return Err(Error::PeerNameUnsuitable);
        }
        if self.our_group.contains(name) {
            return Err(Error::AlreadyExists);
        }
        let mut our_group = self.our_group.clone();
        our_group.insert(self.our_name);
        let next_name_bit = name.bit(self.our_group_prefix.bit_count());
        let mut groups = self.groups.clone();
        if self.should_split_our_group(our_group.iter().chain(iter::once(name))) {
            let name_prefix_after_split = self.our_group_prefix.pushed(next_name_bit);
            groups = groups.into_iter()
                .filter(|&(prefix, _)| prefix.is_neighbour(&name_prefix_after_split))
                .collect();
            let (name_group, other_group) = our_group.into_iter()
                .partition(|x| name_prefix_after_split.matches(x));
            let _ = groups.insert(self.our_group_prefix.pushed(!next_name_bit), other_group);
            let _ = groups.insert(name_prefix_after_split, name_group);
        } else {
            let _ = groups.insert(self.our_group_prefix, our_group);
        }
        Ok(groups)
    }

    /// Adds a contact to the routing table.
    ///
    /// Returns `Err` if `name` already existed in the routing table, or it doesn't fall within any
    /// of our groups, or it's our own name.  Otherwise it returns `Ok(true)` if the addition
    /// succeeded and should cause our group to split or `Ok(false)` if the addition succeeded and
    /// shouldn't cause a split.
    pub fn add(&mut self, name: T) -> Result<bool, Error> {
        if name == self.our_name {
            return Err(Error::OwnNameDisallowed);
        }

        if let Some(group) = self.get_mut_group(&name) {
            if !group.insert(name) {
                return Err(Error::AlreadyExists);
            }
        } else {
            return Err(Error::PeerNameUnsuitable);
        }

        if let Some(needed_prefix) =
            self.needed
                .keys()
                .find(|&prefix| prefix.matches(&name))
                .cloned() {
            // Safe to unwrap as we just found this key
            let mut needed_group = unwrap!(self.needed.remove(&needed_prefix));
            let _ = needed_group.remove(&name);
            if !needed_group.is_empty() {
                let _ = self.needed.insert(needed_prefix, needed_group);
            }
        }

        Ok(self.should_split_our_group(self.our_group.iter().chain(iter::once(&self.our_name))))
    }

    /// Splits a group.
    ///
    /// If the group exists in the routing table, it is split, otherwise this function is a no-op.
    /// If any of the groups don't satisfy the invariant any more (i.e. only differ in one bit from
    /// our own prefix), they are removed and those contacts are returned.  If the split is
    /// happening to our own group, our new prefix is returned in the optional field.
    pub fn split(&mut self, prefix: Prefix<T>) -> (Vec<T>, Option<Prefix<T>>) {
        let mut result = vec![];
        if prefix == self.our_group_prefix {
            result = self.split_our_group();
            return (result, Some(self.our_group_prefix));
        }

        if let Some(to_split) = self.groups.remove(&prefix) {
            let prefix0 = prefix.pushed(false);
            let prefix1 = prefix.pushed(true);
            let (group0, group1) = to_split.into_iter()
                .partition::<HashSet<_>, _>(|name| prefix0.matches(name));

            if self.our_group_prefix.is_neighbour(&prefix0) {
                let _ = self.groups.insert(prefix0, group0);
            } else {
                result.extend(group0);
            }

            if self.our_group_prefix.is_neighbour(&prefix1) {
                let _ = self.groups.insert(prefix1, group1);
            } else {
                result.extend(group1);
            }
        }
        (result, None)
    }

    /// Removes a contact from the routing table.
    ///
    /// If no entry with that name is found, `Err(Error::NoSuchPeer)` is returned.  Otherwise, the
    /// entry is removed from the routing table and `RemovalDetails` is returned.  See that struct's
    /// docs for further info.
    pub fn remove(&mut self, name: &T) -> Result<RemovalDetails<T>, Error> {
        let removal_details = RemovalDetails {
            name: *name,
            was_in_our_group: self.our_group_prefix.matches(name),
        };
        if removal_details.was_in_our_group {
            if !self.our_group.remove(name) {
                return Err(Error::NoSuchPeer);
            }
        } else if let Some(prefix) = self.find_group_prefix(name) {
            if let Some(group) = self.groups.get_mut(&prefix) {
                if !group.remove(name) {
                    return Err(Error::NoSuchPeer);
                }
            }
        } else {
            return Err(Error::NoSuchPeer);
        }
        Ok(removal_details)
    }

    /// If our group is required to merge, returns the details to initiate merging.
    ///
    /// Merging is required if any group has dropped below the minimum size and can only restore it
    /// by ultimately merging with us.
    ///
    /// However, merging happens in simple steps, each of which involves only two groups. If. e.g.
    /// group `1` drops below the minimum size, and the other groups are `01`, `001` and `000`,
    /// then this will return `true` only in the latter two. Once they are merged and have
    /// established all their new connections, it will return `true` in `01` and `00`. Only after
    /// that, the group `0` will merge with group `1`.
    pub fn should_merge(&self) -> Option<OwnMergeDetails<T>> {
        let bit_count = self.our_group_prefix.bit_count();
        let doesnt_need_to_merge_with_us = |(prefix, group): (&Prefix<T>, &HashSet<T>)| {
            !prefix.popped().is_compatible(&self.our_group_prefix) ||
            group.len() >= self.min_group_size
        };
        if bit_count == 0 || !self.needed.is_empty() || !self.merging.is_empty() ||
           !self.groups.contains_key(&self.our_group_prefix.with_flipped_bit(bit_count - 1)) ||
           (self.our_group.len() + 1 >= self.min_group_size &&
            self.groups.iter().all(doesnt_need_to_merge_with_us)) {
            return None;
        }
        let merge_prefix = self.our_group_prefix.popped();
        let mut groups = self.groups.clone();
        let mut our_group = self.our_group.clone();
        let _ = our_group.insert(self.our_name);
        let _ = groups.insert(self.our_group_prefix, our_group);
        Some(OwnMergeDetails {
            sender_prefix: self.our_group_prefix,
            merge_prefix: merge_prefix,
            groups: groups,
        })
    }

    /// When a merge of our own group is triggered (either from our own group or a neighbouring one)
    /// this function handles the incoming merge details from the peers within the merging groups.
    ///
    /// The actual merge of the group is only done once all expected merging groups have provided
    /// details.  See the docs for `OwnMergeState` for full details of the return value.
    pub fn merge_own_group(&mut self, merge_details: OwnMergeDetails<T>) -> OwnMergeState<T> {
        // TODO: Return an error if they are not compatible instead?
        if !self.our_group_prefix.is_compatible(&merge_details.merge_prefix) ||
           self.our_group_prefix.bit_count() <= merge_details.merge_prefix.bit_count() {
            warn!("{:?}: Attempt to call merge_own_group() for an already merged prefix {:?}",
                  self.our_name,
                  merge_details.merge_prefix);
            return OwnMergeState::AlreadyMerged;
        }
        for (prefix, contacts) in &merge_details.groups {
            let compatible_with_ours = self.our_group_prefix.is_compatible(prefix);
            // If it's a merging group...
            if merge_details.merge_prefix.is_compatible(prefix) {
                // This may be a group which has been merged from multiple groups currently still
                // in our RT, so fix up our RT first.
                if !self.groups.contains_key(prefix) && !compatible_with_ours {
                    self.merge(prefix);
                }
                // Cache the prefix, flagging the sender's prefix `true`.  If the prefix is
                // compatible with ours, just add our own prefix, as we could have been merging at
                // the same time as the sender, in which case it could have out-of-date info about
                // our own groups.
                let prefix_to_enter = if compatible_with_ours {
                    self.our_group_prefix
                } else {
                    *prefix
                };
                let prefix_entry = self.merging.entry(prefix_to_enter).or_insert(false);
                if merge_details.sender_prefix == *prefix {
                    *prefix_entry = true;
                }
            }
            // Add an empty group in the table and cache the corresponding contacts.
            let group = if compatible_with_ours {
                &self.our_group
            } else {
                self.groups.entry(*prefix).or_insert_with(HashSet::new)
            };
            if group.is_empty() {
                let needed_group = self.needed.entry(*prefix).or_insert_with(HashSet::new);
                needed_group.extend(contacts);
            }
        }

        let called_count = self.merging.values().filter(|&&flag| flag).count();
        if called_count == self.merging.len() {
            // We've heard from all merging groups - do the merge and return `Completed`.
            self.finish_merging_own_group(merge_details)
        } else if merge_details.sender_prefix == self.our_group_prefix || called_count > 1 {
            // We've either triggered the merge ourself via a `remove()` call or we've already been
            // notified of the merge by a peer group via this function, so we've already notified
            // the merging peers.
            OwnMergeState::Ongoing
        } else {
            // This is the first we've heard of the merge - return `Initialised`.
            self.initialise_merging_own_group(merge_details)
        }
    }

    /// Merges all existing compatible groups into the new one defined by `merge_details.prefix`.
    /// Our own group is not included in the merge.
    ///
    /// The appropriate targets (all contacts from `merge_details.groups` which are not currently
    /// held in the routing table) are returned so the caller can establish connections to these
    /// peers and subsequently add them.
    pub fn merge_other_group(&mut self, merge_details: OtherMergeDetails<T>) -> HashSet<T> {
        if self.our_group_prefix.is_compatible(&merge_details.prefix) {
            // We've already handled this particular merge via `merge_own_group()`.
            return HashSet::new();
        }

        self.merge(&merge_details.prefix);

        // Establish list of provided contacts which are currently missing from our table.
        merge_details.group
            .difference(unwrap!(self.groups.get(&merge_details.prefix)))
            .cloned()
            .collect()
    }

    /// Gets the `route`-th name from a collection of names
    pub fn get_routeth_name<'a, U: IntoIterator<Item = &'a T>>(names: U,
                                                               dst_name: &T,
                                                               route: usize)
                                                               -> &'a T {
        let sorted_names = names.into_iter()
            .sorted_by(|&lhs, &rhs| dst_name.cmp_distance(lhs, rhs));
        sorted_names[route % sorted_names.len()]
    }

    /// Returns the `route`-th node in the given group, sorted by distance to `target`
    pub fn get_routeth_node(&self,
                            group: &HashSet<T>,
                            target: T,
                            exclude: Option<T>,
                            route: usize)
                            -> Result<T, Error> {
        let names = if let Some(exclude) = exclude {
            group.iter().filter(|&x| *x != exclude).collect_vec()
        } else {
            group.iter().collect_vec()
        };

        if names.is_empty() {
            return Err(Error::CannotRoute);
        }

        Ok(*RoutingTable::get_routeth_name(names, &target, route))
    }

    /// Returns a collection of nodes to which a message with the given `Destination` should be sent
    /// onwards.  In all non-error cases below, the returned collection will have the members of
    /// `exclude` removed, possibly resulting in an empty set being returned.
    ///
    /// * If the destination is a group:
    ///     - if our group is the closest on the network (i.e. our group's prefix is a prefix of the
    ///       destination), returns all other members of our group; otherwise
    ///     - if the closest group has more than `route` members, returns the `route`-th member of
    ///       this group; otherwise
    ///     - returns `Err(Error::CannotRoute)`
    ///
    /// * If the destination is an individual node:
    ///     - if our name *is* the destination, returns an empty set; otherwise
    ///     - if the destination name is an entry in the routing table, returns it; otherwise
    ///     - if our group is the closest on the network (i.e. our group's prefix is a prefix of the
    ///       destination), this returns `Err(Error::NoSuchPeer)`; otherwise
    ///     - if the closest group has more than `route` members, returns the `route`-th member of
    ///       this group; otherwise
    ///     - returns `Err(Error::CannotRoute)`
    pub fn targets(&self,
                   dst: &Destination<T>,
                   exclude: T,
                   route: usize)
                   -> Result<HashSet<T>, Error> {
        let (closest_group, target_name) = match *dst {
            Destination::Group(ref target_name) => {
                let (prefix, group) = self.closest_group(target_name);
                if *prefix == self.our_group_prefix {
                    return Ok(group.clone());
                }
                (group, target_name)
            }
            Destination::Node(ref target_name) => {
                if *target_name == self.our_name {
                    return Ok(HashSet::new());
                }
                let (_, group) = self.closest_group(target_name);
                if group.contains(target_name) {
                    return Ok(iter::once(*target_name).collect());
                }
                // TODO: This is temporarily disabled for the cases where we have not connected to
                //       all needed contacts yet and may have empty or incomplete groups.
                // } else if *closest_group_prefix == self.our_group_prefix {
                //     return Err(Error::NoSuchPeer);
                // }
                (group, target_name)
            }
        };
        Ok(iter::once(self.get_routeth_node(closest_group, *target_name, Some(exclude), route)?)
            .collect())
    }

    /// Returns whether a `Destination` represents this node.
    ///
    /// Returns `true` if `dst` is a single node with name equal to `our_name`, or if `dst` is a
    /// group and the closest group is our group.
    pub fn is_recipient(&self, dst: &Destination<T>) -> bool {
        match *dst {
            Destination::Node(ref target_name) => *target_name == self.our_name,
            Destination::Group(ref target_name) => self.our_group_prefix.matches(target_name),
        }
    }

    /// Returns the group matching the given `name`, if present.
    pub fn get_group(&self, name: &T) -> Option<&HashSet<T>> {
        if self.our_group_prefix.matches(name) {
            return Some(&self.our_group);
        }
        if let Some(prefix) = self.find_group_prefix(name) {
            return self.groups.get(&prefix);
        }
        None
    }

    /// Returns our name
    pub fn our_name(&self) -> &T {
        &self.our_name
    }

    fn should_split_our_group<I>(&self, our_group: I) -> bool
        where I: IntoIterator,
              I::Item: Borrow<T>
    {
        // Count the number of names which will end up in each new group if our group is split.
        let mut new_group_size_0 = 0;
        let mut new_group_size_1 = 0;
        for name in our_group {
            if self.our_name.common_prefix(name.borrow()) > self.our_group_prefix.bit_count() {
                new_group_size_0 += 1;
            } else {
                new_group_size_1 += 1;
            }
        }
        // If either of the two new groups will not contain enough entries, return `false`.
        let min_size = self.min_split_size();
        new_group_size_0 >= min_size && new_group_size_1 >= min_size
    }

    fn split_our_group(&mut self) -> Vec<T> {
        let next_bit = self.our_name.bit(self.our_group_prefix.bit_count());
        let other_prefix = self.our_group_prefix.pushed(!next_bit);
        self.our_group_prefix = self.our_group_prefix.pushed(next_bit);
        let (our_new_group, other_group) = self.our_group
            .iter()
            .partition::<HashSet<_>, _>(|name| self.our_group_prefix.matches(name));
        self.our_group = our_new_group;
        // Drop groups that ceased to be our neighbours.
        let groups_to_remove = self.groups
            .keys()
            .filter(|prefix| !prefix.is_neighbour(&self.our_group_prefix))
            .cloned()
            .collect_vec();
        let _ = self.groups.insert(other_prefix, other_group);
        groups_to_remove.into_iter()
            .filter_map(|prefix| self.groups.remove(&prefix))
            .flat_map(HashSet::into_iter)
            .collect()
    }

    fn initialise_merging_own_group(&self,
                                    mut merge_details: OwnMergeDetails<T>)
                                    -> OwnMergeState<T> {
        merge_details.sender_prefix = self.our_group_prefix;
        merge_details.groups
            .extend(self.groups.iter().filter_map(|(prefix, names)| {
                if names.is_empty() {
                    None
                } else {
                    Some((*prefix, names.clone()))
                }
            }));
        let mut our_group = self.our_group.clone();
        our_group.insert(self.our_name);
        let _ = merge_details.groups.insert(self.our_group_prefix, our_group);
        OwnMergeState::Initialised { merge_details: merge_details }
    }

    fn finish_merging_own_group(&mut self, merge_details: OwnMergeDetails<T>) -> OwnMergeState<T> {
        self.merging.clear();
        self.merge(&merge_details.merge_prefix);
        let targets = self.groups.keys().cloned().collect();
        let mut other_details = OtherMergeDetails {
            prefix: merge_details.merge_prefix,
            group: self.our_group.clone(),
        };
        other_details.group.extend(self.needed
            .iter()
            .filter(|&(prefix, _)| self.our_group_prefix.is_compatible(prefix))
            .flat_map(|(_, names)| names.iter().cloned()));
        other_details.group.insert(self.our_name);
        OwnMergeState::Completed {
            targets: targets,
            merge_details: other_details,
        }
    }

    fn merge(&mut self, new_prefix: &Prefix<T>) {
        // Partition the groups into those for merging and the rest
        let original_groups = mem::replace(&mut self.groups, Groups::new());
        let (groups_to_merge, mut groups) = original_groups.into_iter()
            .partition::<HashMap<_, _>, _>(|&(prefix, _)| new_prefix.is_compatible(&prefix));
        // Merge selected groups and add the merged group back in.
        let merged_names = groups_to_merge.into_iter().flat_map(|(_, names)| names).collect();
        if self.our_group_prefix.is_compatible(new_prefix) {
            self.our_group.extend(merged_names);
            self.our_group_prefix = *new_prefix;
        } else {
            let _ = groups.insert(*new_prefix, merged_names);
        }
        self.groups = groups;
    }

    fn get_mut_group(&mut self, name: &T) -> Option<&mut HashSet<T>> {
        if self.our_group_prefix.matches(name) {
            return Some(&mut self.our_group);
        }
        if let Some(prefix) = self.find_group_prefix(name) {
            return self.groups.get_mut(&prefix);
        }
        None
    }

    // Returns the prefix of the group in which `name` belongs, or `None` if there is no such group
    // in the routing table.
    fn find_group_prefix(&self, name: &T) -> Option<Prefix<T>> {
        if self.our_group_prefix.matches(name) {
            return Some(self.our_group_prefix);
        }
        self.groups.keys().find(|&prefix| prefix.matches(name)).cloned()
    }

    // Returns the prefix of the group closest to `name`, regardless of whether `name` belongs in
    // that group or not, and the group itself.
    fn closest_group(&self, name: &T) -> (&Prefix<T>, &HashSet<T>) {
        let mut result = (&self.our_group_prefix, &self.our_group);
        for entry in &self.groups {
            if result.0.cmp_distance(entry.0, name) == Ordering::Greater {
                result = entry
            }
        }
        result
    }

    fn min_split_size(&self) -> usize {
        self.min_group_size + SPLIT_BUFFER
    }

    fn check_invariant(&self) -> Result<(), Error> {
        if !self.our_group_prefix.matches(&self.our_name) {
            warn!("Our prefix does not match our name: {:?}", self);
            return Err(Error::InvariantViolation);
        }
        if self.groups.contains_key(&self.our_group_prefix) {
            warn!("Our own group is in the groups map: {:?}", self);
            return Err(Error::InvariantViolation);
        }
        let has_enough_nodes = self.len() >= self.min_group_size;
        if has_enough_nodes && self.our_group.len() + 1 < self.min_group_size {
            warn!("Minimum group size not met for group {:?}: {:?}",
                  self.our_group_prefix,
                  self);
            return Err(Error::InvariantViolation);
        }
        for name in &self.our_group {
            if !self.our_group_prefix.matches(name) {
                warn!("Name {} doesn't match group prefix {:?}: {:?}",
                      name.debug_binary(),
                      self.our_group_prefix,
                      self);
                return Err(Error::InvariantViolation);
            }
        }
        for (prefix, group) in &self.groups {
            let len = group.len() + self.needed.get(prefix).map(HashSet::len).unwrap_or(0);
            if has_enough_nodes && len < self.min_group_size {
                warn!("Minimum group size not met for group {:?}: {:?}",
                      prefix,
                      self);
                return Err(Error::InvariantViolation);
            }
            for name in group {
                if !prefix.matches(name) {
                    warn!("Name {} doesn't match group prefix {:?}: {:?}",
                          name.debug_binary(),
                          prefix,
                          self);
                    return Err(Error::InvariantViolation);
                }
            }
        }

        let all_are_neighbours =
            self.groups.keys().all(|&x| self.our_group_prefix.is_neighbour(&x));
        let all_neighbours_covered = {
            let prefixes = self.prefixes();
            (0..self.our_group_prefix.bit_count())
                .all(|i| self.our_group_prefix.with_flipped_bit(i).is_covered_by(&prefixes))
        };
        if !all_are_neighbours {
            warn!("Some groups in the RT aren't neighbours of our group: {:?}",
                  self);
            return Err(Error::InvariantViolation);
        }
        if !all_neighbours_covered {
            warn!("Some neighbours aren't fully covered by the RT: {:?}", self);
            return Err(Error::InvariantViolation);
        }

        // TODO: any other invariants to check? What about `self.needed`?
        Ok(())
    }

    /// Returns the list of contacts as a result of a merge to which we aren't currently connected,
    /// but should be.
    #[cfg(any(test, feature = "use-mock-crust"))]
    pub fn needed(&self) -> &Groups<T> {
        &self.needed
    }

    /// Runs the built-in invariant checker
    #[cfg(any(test, feature = "use-mock-crust"))]
    pub fn verify_invariant(&self) {
        unwrap!(self.check_invariant(),
                "Invariant not satisfied for RT: {:?}",
                self);
    }

    #[cfg(test)]
    fn num_of_groups(&self) -> usize {
        self.groups.len()
    }
}

impl<T: Binary + Clone + Copy + Debug + Default + Hash + Xorable> Binary for RoutingTable<T> {
    fn fmt(&self, formatter: &mut Formatter) -> FmtResult {
        writeln!(formatter,
                 "RoutingTable {{\n\tour_name: {:?} ({}),\n\tmin_group_size: \
                  {},\n\tour_group_prefix: {:?},",
                 self.our_name,
                 self.our_name.debug_binary(),
                 self.min_group_size,
                 self.our_group_prefix)?;
        let mut groups = self.groups
            .iter()
            .chain(iter::once((&self.our_group_prefix, &self.our_group)))
            .collect_vec();
        groups.sort_by(|&(lhs_prefix, _), &(rhs_prefix, _)| {
            lhs_prefix.cmp_distance(rhs_prefix, &self.our_name)
        });
        let groups_len = groups.len();
        for (group_index, (prefix, group)) in groups.into_iter().enumerate() {
            write!(formatter, "\tgroup {} with {:?}: {{\n", group_index, prefix)?;
            for (name_index, name) in group.iter().enumerate() {
                let comma = if name_index == group.len() - 1 {
                    ""
                } else {
                    ","
                };
                writeln!(formatter,
                         "\t\t{:?} ({}){}",
                         name,
                         name.debug_binary(),
                         comma)?;
            }
            let comma = if group_index == groups_len - 1 {
                ""
            } else {
                ","
            };
            writeln!(formatter, "\t}}{}", comma)?;
        }
        writeln!(formatter, "\tneeded: {:?}", self.needed)?;
        writeln!(formatter, "\tmerging: {:?}", self.merging)?;
        write!(formatter, "}}")
    }
}

impl<T: Binary + Clone + Copy + Debug + Default + Hash + Xorable> Debug for RoutingTable<T> {
    fn fmt(&self, formatter: &mut Formatter) -> FmtResult {
        Binary::fmt(self, formatter)
    }
}

impl<T: Binary + Clone + Copy + Debug + Default + Hash + Xorable> Drop for RoutingTable<T> {
    fn drop(&mut self) {
        if thread::panicking() {
            trace!("{:?}", self);
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small() {
        let name = 123u32;
        let table = RoutingTable::new(name, 6);
        assert_eq!(*table.our_name(), name);
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
        assert_eq!(table.iter().count(), 0);
    }

    // Test explicitly covers close_names(),  other_close_names(),
    // is_in_our_group() and need_to_add() while also implicitly testing
    // add() and split() through set-up of random groups with invariant.
    #[test]
    fn test_routing_groups() {
        // Use replicable random numbers to initialse a table:
        use rand::{Rng, SeedableRng, XorShiftRng};
        let mut rng: XorShiftRng = SeedableRng::from_seed([1315, 30, 61894, 315]);
        let our_name = rng.next_u32();
        let mut table = RoutingTable::new(our_name, 8);
        unwrap!(table.check_invariant());
        let mut unknown_distant_name = None;

        for _ in 0..1000 {
            let new_name = rng.next_u32();
            // Try to add new_name. We double-check the output to test this too.
            match table.add(new_name) {
                Err(Error::AlreadyExists) => {
                    unwrap!(table.check_invariant());
                    assert!(table.iter().any(|u| *u == new_name));
                    // skip
                }
                Err(Error::PeerNameUnsuitable) => {
                    unwrap!(table.check_invariant());
                    assert!(table.groups.keys().all(|p| !p.matches(&new_name)));
                    // We should get a few of these. Save one for tests, but otherwise ignore.
                    unknown_distant_name = Some(new_name);
                }
                Err(e) => {
                    panic!("unexpected error: {}", e);
                }
                Ok(true) => {
                    unwrap!(table.check_invariant());
                    let our_prefix = *table.our_group_prefix();
                    assert!(our_prefix.matches(&new_name));
                    let _ = table.split(our_prefix);
                    unwrap!(table.check_invariant());
                }
                Ok(false) => {
                    unwrap!(table.check_invariant());
                    assert!(table.iter().any(|u| *u == new_name));
                    if table.is_in_our_group(&new_name) {
                        continue;   // add() already checked for necessary split
                    }

                    // Not a split event for our group, but might be for a different group.
                    let group_prefix = table.find_group_prefix(&new_name)
                        .expect("get group added to");
                    let (group_len, new_group_size) = {
                        let group = table.groups.get(&group_prefix).expect("get group from prefix");
                        // Count size of group after an arbitrary split (note that there is only
                        // one split possible; the arbitrariness is just which half we choose here).
                        (group.len(),
                         group.iter()
                             .filter(|name| new_name.common_prefix(name) > group_prefix.bit_count())
                             .count())
                    };
                    let min_size = table.min_split_size();
                    if new_group_size >= min_size && group_len - new_group_size >= min_size {
                        let _ = table.split(group_prefix);  // do the split
                        unwrap!(table.check_invariant());
                    }
                }
            }
        }

        let unknown_neighbour;
        loop {
            let new_name = rng.next_u32();
            if table.our_group_prefix.matches(&new_name) {
                continue;
            }
            if let Some(prefix) = table.groups.keys().find(|p| p.matches(&new_name)) {
                if !unwrap!(table.groups.get(&prefix)).contains(&new_name) {
                    unknown_neighbour = new_name;
                    break;
                }
            }
        }

        let unknown_distant_name = unwrap!(unknown_distant_name);
        // These numbers depend on distribution of names
        let num_known_nodes = 104;
        let num_groups = 8;
        let len_our_group = 13;
        assert_eq!(table.len(), num_known_nodes);
        assert_eq!(table.groups.len(), num_groups - 1);
        assert_eq!(table.our_group.len() + 1, len_our_group);
        assert_eq!(our_name, table.our_name);

        // Get some names
        let close_name: u32 = *unwrap!(table.our_group.iter().nth(4));
        let mut known_neighbour: Option<u32> = None;
        for (prefix, group) in &table.groups {
            if *prefix == table.our_group_prefix {
                continue;
            }
            known_neighbour = Some(*unwrap!(group.iter().next()));
            break;
        }
        let known_neighbour = unwrap!(known_neighbour);
        assert!(!table.our_group_prefix.matches(&known_neighbour));

        assert!(table.iter().any(|u| *u == close_name));
        assert!(table.iter().any(|u| *u == known_neighbour));
        assert!(table.iter().all(|u| *u != unknown_neighbour));
        assert!(table.iter().all(|u| *u != unknown_distant_name));
        assert!(table.is_in_our_group(&close_name));
        assert!(!table.is_in_our_group(&known_neighbour));

        // Tests on close_names
        assert_eq!(table.close_names(&close_name).unwrap().len(), len_our_group);
        assert!(table.close_names(&known_neighbour).is_none());
        assert!(table.close_names(&unknown_neighbour).is_none());
        assert!(table.close_names(&unknown_distant_name).is_none());

        // Tests on other_close_names
        assert_eq!(table.other_close_names(&close_name).unwrap().len(),
                   len_our_group - 1);
        assert!(table.other_close_names(&known_neighbour).is_none());
        assert!(table.other_close_names(&unknown_neighbour).is_none());
        assert!(table.other_close_names(&unknown_distant_name).is_none());

        // Tests on is_in_our_group
        assert!(table.is_in_our_group(&our_name));
        assert!(table.is_in_our_group(&close_name));
        assert!(!table.is_in_our_group(&known_neighbour));
        assert!(!table.is_in_our_group(&unknown_neighbour));
        assert!(!table.is_in_our_group(&unknown_distant_name));

        // Tests on need_to_add
        assert_eq!(table.need_to_add(&our_name), Err(Error::OwnNameDisallowed));
        assert_eq!(table.need_to_add(&close_name), Err(Error::AlreadyExists));
        assert_eq!(table.need_to_add(&known_neighbour),
                   Err(Error::AlreadyExists));
        assert_eq!(table.need_to_add(&unknown_neighbour), Ok(()));
        assert_eq!(table.need_to_add(&unknown_distant_name),
                   Err(Error::PeerNameUnsuitable));
    }
}
