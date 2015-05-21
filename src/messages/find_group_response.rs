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

#![allow(unused_assignments)]

use cbor::CborTagEncode;
use rustc_serialize::{Decodable, Decoder, Encodable, Encoder};
use frequency::Frequency;
use types::{PublicId, GROUP_SIZE, QUORUM_SIZE, Mergeable};

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FindGroupResponse {
  pub group : Vec<PublicId>
}

impl Mergeable for FindGroupResponse {
    fn merge<'a, I>(responses: I) -> Option<Self> where I: Iterator<Item=&'a Self> {
        let mut frequency = Frequency::new();

        for response in responses {
            for public_id in &response.group {
                frequency.update(public_id.clone());
            }
        }

        let merged_group = frequency.sort_by_highest().into_iter()
                           .filter(|&(_, ref count)| *count >= QUORUM_SIZE as usize)
                           .take(GROUP_SIZE as usize)
                           .map(|(k, _)| k)
                           .collect::<Vec<_>>();

        if merged_group.is_empty() { return None; }
        Some(FindGroupResponse{ group: merged_group })
    }
}

impl Encodable for FindGroupResponse {
  fn encode<E: Encoder>(&self, e: &mut E)->Result<(), E::Error> {
    CborTagEncode::new(5483_001, &self.group).encode(e)
  }
}

impl Decodable for FindGroupResponse {
  fn decode<D: Decoder>(d: &mut D)->Result<FindGroupResponse, D::Error> {
    try!(d.read_u64());
    let group = try!(Decodable::decode(d));
    Ok(FindGroupResponse { group: group})
  }
}

#[cfg(test)]
mod test {
    use super::*;
    use cbor;
    use types;
    use types::{PublicId, GROUP_SIZE, QUORUM_SIZE};
    use test_utils::Random;
    use rand::{thread_rng, Rng};
    use rand::distributions::{IndependentSample, Range};


    #[test]
    fn find_group_response_serialisation() {
        let obj_before : FindGroupResponse = Random::generate_random();

        let mut e = cbor::Encoder::from_memory();
        e.encode(&[&obj_before]).unwrap();

        let mut d = cbor::Decoder::from_bytes(e.as_bytes());
        let obj_after: FindGroupResponse = d.decode().next().unwrap().unwrap();

        assert_eq!(obj_before, obj_after);
    }

    #[test]
    fn merge() {
        let ids: FindGroupResponse = Random::generate_random();
        // ids.group.len() == types::GROUP_SIZE + 20
        assert!(ids.group.len() >= GROUP_SIZE as usize);

        let group_size = GROUP_SIZE as usize;
        let quorum_size = QUORUM_SIZE as usize;

        // get random GROUP_SIZE groups
        let mut groups = Vec::<PublicId>::with_capacity(quorum_size);
        let mut rng = thread_rng();
        let range = Range::new(0, ids.group.len());

        loop {
            let index = range.ind_sample(&mut rng);
            if groups.contains(&ids.group[index]) { continue; }
            groups.push(ids.group[index].clone());
            if groups.len() == quorum_size { break; }
        };

        let mut responses = Vec::<FindGroupResponse>::with_capacity(quorum_size);

        for _ in 0..quorum_size {
            let mut response = FindGroupResponse{ group: Vec::new() };
            // Take the first QUORUM_SIZE as common...
            for i in 0..quorum_size {
                response.group.push(groups[i].clone());
            }
            // ...and the remainder arbitrary
            for _ in quorum_size..group_size {
                response.group.push(PublicId::generate_random());
            }

            rng.shuffle(&mut response.group[..]);
            responses.push(response);
        }

        let merged = types::Mergeable::merge(responses.iter());
        assert!(merged.is_some());
        let merged_response = merged.unwrap();
        for i in 0..quorum_size {
            assert!(groups.iter().find(|a| **a == merged_response.group[i]).is_some());
        }
    }
}
