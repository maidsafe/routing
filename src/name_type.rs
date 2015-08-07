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

use cbor::CborTagEncode;
// use cmp::{PartialEq, Eq, PartialOrd, Ord, Ordering};
use std::hash;
use rustc_serialize::{Decodable, Decoder, Encodable, Encoder};
use std::cmp::*;
use std::fmt;

pub const NAME_TYPE_LEN : usize = 64;

///
/// Returns true if both slices are equal in length and have equal contents
///
pub fn slice_equal<T: PartialEq>(lhs: &[T], rhs: &[T]) -> bool {
    lhs.len() == rhs.len() && lhs.iter().zip(rhs.iter()).all(|(a, b)| a == b)
}

/// NameType can be created using the new function by passing ID as it’s parameter.
#[derive(Eq, Copy)]
pub struct NameType(pub [u8; NAME_TYPE_LEN]);

impl NameType {
    pub fn new(id: [u8; NAME_TYPE_LEN]) -> NameType {
        NameType(id)
    }

    // TODO(Ben): Resolve from_data
    // pub fn from_data(data : &[u8]) -> NameType {
    //     NameType::new(&crypto::hash::sha512::hash(data).0)
    // }

    pub fn get_id(&self) -> [u8; NAME_TYPE_LEN] {
        self.0
    }

    // private function exposed in fmt Debug {:?} and Display {} traits
    fn get_debug_id(&self) -> String {
        format!("{:02x}{:02x}{:02x}..{:02x}{:02x}{:02x}",
            self.0[0],
            self.0[1],
            self.0[2],
            self.0[NAME_TYPE_LEN-3],
            self.0[NAME_TYPE_LEN-2],
            self.0[NAME_TYPE_LEN-1])
    }

    // private function exposed in fmt LowerHex {:x} trait
    // note(ben): UpperHex explicitly not implemented to prevent mixed usage
    fn get_full_id(&self) -> String {
        let mut full_id = String::with_capacity(2 * NAME_TYPE_LEN);
        for char in self.0.iter() {
            full_id.push_str(format!("{:02x}", char).as_str());
        }
        full_id
    }
}

impl fmt::Debug for NameType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
      write!(f, "{}", self.get_debug_id())
    }
}

impl fmt::Display for NameType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
      write!(f, "{}", self.get_debug_id())
    }
}

impl fmt::LowerHex for NameType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.get_full_id())
    }
}


impl PartialEq for NameType {
    fn eq(&self, other: &NameType) -> bool {
        slice_equal(&self.0, &other.0)
    }
}

/// Returns true if `lhs` is closer to `target` than `rhs`.  "Closer" here is as per the Kademlia
/// notion of XOR distance, i.e. the distance between two `NameType`s is the bitwise XOR of their
/// values.
pub fn closer_to_target(lhs: &NameType, rhs: &NameType, target: &NameType) -> bool {
    for i in 0..lhs.0.len() {
        let res_0 = lhs.0[i] ^ target.0[i];
        let res_1 = rhs.0[i] ^ target.0[i];

        if res_0 != res_1 {
            return res_0 < res_1
        }
    }
    false
}

/// Returns true if `lhs` is closer to `target` than `rhs`, or when `lhs == rhs`.
/// "Closer" here is as per the Kademlia notion of XOR distance,
/// i.e. the distance between two `NameType`s is the bitwise XOR of their values.
pub fn closer_to_target_or_equal(lhs: &NameType, rhs: &NameType, target: &NameType) -> bool {
    for i in 0..lhs.0.len() {
        let res_0 = lhs.0[i] ^ target.0[i];
        let res_1 = rhs.0[i] ^ target.0[i];

        if res_0 != res_1 {
            return res_0 < res_1
        }
    }
    true
}

/// The `NameType` can be ordered from zero as a normal Euclidean number
impl Ord for NameType {
    #[inline]
    fn cmp(&self, other : &NameType) -> Ordering {
        Ord::cmp(&&self.0[..], &&other.0[..])
    }
}

impl PartialOrd for NameType {
    #[inline]
    fn partial_cmp(&self, other : &NameType) -> Option<Ordering> {
        PartialOrd::partial_cmp(&&self.0[..], &&other.0[..])
    }
    #[inline]
    fn lt(&self, other : &NameType) -> bool {
        PartialOrd::lt(&&self.0[..], &&other.0[..])
    }
    #[inline]
    fn le(&self, other : &NameType) -> bool {
        PartialOrd::le(&&self.0[..], &&other.0[..])
    }
    #[inline]
    fn gt(&self, other : &NameType) -> bool {
        PartialOrd::gt(&&self.0[..], &&other.0[..])
    }
    #[inline]
    fn ge(&self, other : &NameType) -> bool {
        PartialOrd::ge(&&self.0[..], &&other.0[..])
    }
}

impl hash::Hash for NameType {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        state.write(&self.0[..])
    }
}

impl Clone for NameType {
    fn clone(&self) -> Self {
        let mut arr_cloned = [0u8; NAME_TYPE_LEN];
        let &NameType(arr_self) = self;

        for i in 0..arr_self.len() {
            arr_cloned[i] = arr_self[i];
        }

        NameType(arr_cloned)
    }
}

impl Encodable for NameType {
    fn encode<E: Encoder>(&self, e: &mut E)->Result<(), E::Error> {
        CborTagEncode::new(5483_000, &(self.0.as_ref())).encode(e)
    }
}

impl Decodable for NameType {
    fn decode<D: Decoder>(d: &mut D)->Result<NameType, D::Error> {
        try!(d.read_u64());
        let id : Vec<u8> = try!(Decodable::decode(d));

        match container_of_u8_to_array!(id, NAME_TYPE_LEN) {
            Some(id_arr) => Ok(NameType(id_arr)),
            None => Err(d.error("Bad NameType size"))
        }
    }
}

#[cfg(test)]
mod test {
    use cbor;
    use super::*;
    use id::Id;
    use test_utils::Random;

    #[test]
    fn serialisation_name_type() {
        let obj_before: NameType = Random::generate_random();
        let mut e = cbor::Encoder::from_memory();
        e.encode(&[&obj_before]).unwrap();

        let mut d = cbor::Decoder::from_bytes(e.as_bytes());
        let obj_after: NameType = d.decode().next().unwrap().unwrap();
        assert_eq!(obj_before, obj_after);
    }

    #[test]
    fn name_type_equal_assertion() {
        let type1: NameType = Random::generate_random();
        let type1_clone = type1.clone();
        let type2: NameType = Random::generate_random();
        assert_eq!(type1, type1_clone);
        assert!(type1 == type1_clone);
        assert!(!(type1 != type1_clone));
        assert!(type1 != type2);
    }

    #[test]
    fn closeness() {
        let obj0: NameType = Random::generate_random();
        let obj0_clone = obj0.clone();
        let obj1: NameType = Random::generate_random();
        assert!(closer_to_target(&obj0_clone, &obj1, &obj0));
        assert!(!closer_to_target(&obj1, &obj0_clone, &obj0));
    }

    #[test]
    fn format_id_nametype() {
        // test for Ids
        for _ in 0..5 {
            let my_id = Id::new();
            let my_name = my_id.name();
            let debug_id = my_name.get_debug_id();
            let full_id = my_name.get_full_id();
            assert_eq!(debug_id.len(), 14);
            assert_eq!(full_id.len(), 2 * NAME_TYPE_LEN);
            assert_eq!(&debug_id[0..6], &full_id[0..6]);
            assert_eq!(&debug_id[8..14], &full_id[2*NAME_TYPE_LEN-6..2*NAME_TYPE_LEN]);
            assert_eq!(&debug_id[6..8], "..");
        }
    }

    #[test]
    fn format_random_nametype() {
        // test for Random NameType
        for _ in 0..5 {
            let my_name : NameType = Random::generate_random();
            let debug_id = my_name.get_debug_id();
            let full_id = my_name.get_full_id();
            assert_eq!(debug_id.len(), 14);
            assert_eq!(full_id.len(), 2 * NAME_TYPE_LEN);
            assert_eq!(&debug_id[0..6], &full_id[0..6]);
            assert_eq!(&debug_id[8..14], &full_id[2*NAME_TYPE_LEN-6..2*NAME_TYPE_LEN]);
            assert_eq!(&debug_id[6..8], "..");
        }
    }

    #[test]
    fn format_fixed_low_char_nametype() {
        // test for fixed low char values in NameType
        let low_char_id = [1u8; NAME_TYPE_LEN];
        let my_low_char_name = NameType::new(low_char_id);
        let debug_id = my_low_char_name.get_debug_id();
        let full_id = my_low_char_name.get_full_id();
        assert_eq!(debug_id.len(), 14);
        assert_eq!(full_id.len(), 2 * NAME_TYPE_LEN);
        assert_eq!(&debug_id[0..6], &full_id[0..6]);
        assert_eq!(&debug_id[8..14], &full_id[2*NAME_TYPE_LEN-6..2*NAME_TYPE_LEN]);
        assert_eq!(&debug_id[6..8], "..");
    }

    //TODO(Ben: resolve from_data)
    // #[test]
    // fn name_from_data() {
    //   use rustc_serialize::hex::ToHex;
    //   let data = "this is a known string".to_string().into_bytes();
    //   let expected_name = "8758b09d420bdb901d68fdd6888b38ce9ede06aad7f\
    //                        e1e0ea81feffc76260554b9d46fb6ea3b169ff8bb02\
    //                        ef14a03a122da52f3063bcb1bfb22cffc614def522".to_string();
    //   assert_eq!(&expected_name, &NameType::from_data(&data).0.to_hex());
    // }
}
