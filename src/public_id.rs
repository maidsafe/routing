// Copyright 2015 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.0.  This, along with the
// Licenses can be found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

use cbor;
use rustc_serialize::{Decoder, Encodable, Encoder};
use sodiumoxide::crypto::sign;
use sodiumoxide::crypto::box_;
use NameType;
use error::{RoutingError};
use id::Id;
use utils;
use std::fmt::{Debug, Formatter, Error};

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, RustcEncodable, RustcDecodable)]
pub struct PublicId {
    public_encrypt_key: box_::PublicKey,
    public_sign_key: sign::PublicKey,
    name: NameType,
}

impl Debug for PublicId {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), Error> {
        formatter.write_str(&format!("PublicId(name:{:?})", self.name))
    }
}

impl PublicId {
    pub fn new(id : &Id) -> PublicId {
      PublicId {
        public_encrypt_key : id.encrypting_public_key().clone(),
        public_sign_key    : id.signing_public_key().clone(),
        name               : id.name(),
      }
    }

    pub fn name(&self) -> NameType {
        self.name
    }

    pub fn set_name(&mut self, name: NameType) {
        self.name = name;
    }

    pub fn client_name(&self) -> NameType {
        utils::public_key_to_client_name(&self.public_sign_key)
    }

    pub fn serialised_contents(&self)->Result<Vec<u8>, RoutingError> {
        let mut e = cbor::Encoder::from_memory();
        try!(e.encode(&[&self]));
        Ok(e.into_bytes())
    }

    // name field is initially same as original_name, this should be replaced by relocated name
    // calculated by the nodes close to original_name by using this method
    pub fn assign_relocated_name(&mut self, relocated_name: NameType) {
        self.name = relocated_name;
    }

    pub fn signing_public_key(&self) -> sign::PublicKey {
        self.public_sign_key
    }

    // checks if the name is updated to a relocated name
    pub fn is_relocated(&self) -> bool {
        self.name != utils::public_key_to_client_name(&self.public_sign_key)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use NameType;
    use test_utils::Random;
    use sodiumoxide::crypto;

    #[test]
    fn assign_relocated_name_public_id() {
        let before = PublicId::generate_random();
        let original_name = before.name();
        assert_eq!(original_name,
            NameType::new(crypto::hash::sha512::hash(&before.signing_public_key()[..]).0));
        assert!(!before.is_relocated());
        let relocated_name: NameType = Random::generate_random();
        let mut relocated = before.clone();
        relocated.assign_relocated_name(relocated_name.clone());
        assert!(relocated.is_relocated());
        assert_eq!(before.signing_public_key(), relocated.signing_public_key());
        assert_eq!(relocated.client_name(), original_name);
        assert_eq!(relocated.name(), relocated_name);
    }
}
