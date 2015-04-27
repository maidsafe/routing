// Copyright 2015 MaidSafe.net limited
//
// This Safe Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the Safe Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.0, found in the root
// directory of this project at LICENSE, COPYING and CONTRIBUTOR respectively and also
// available at: http://maidsafe.net/network-platform-licensing
//
// Unless required by applicable law or agreed to in writing, the Safe Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS
// OF ANY KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations relating to
// use of the Safe Network Software.

use name_type;

/// This trait is required for any type of message to be
/// passed to routing, refresh / account transfer is optional
/// The name will let routing know its a NaeManager and the owner will allow routing to hash
/// the requesters ID with this name (by hashing the requesters ID) for put and post messages
pub trait Sendable {
    fn name(&self)->name_type::NameType;
    fn type_tag(&self)->u64;
    fn serialised_contents(&self)->Vec<u8>;
    fn owner(&self)->Option<name_type::NameType> { Option::None }
    fn refresh(&self)->bool { false } // is this an account transfer type
    fn merge(&self)->bool { false } // how do we merge these
}
