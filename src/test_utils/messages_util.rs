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

use rand;
use xor_name::XorName;
use messages::{RequestMessage, RequestContent};

fn generate_random_authority(name: XorName,
                             key: &::sodiumoxide::crypto::sign::PublicKey)
                             -> ::authority::Authority {
    use rand::distributions::IndependentSample;

    let mut rng = ::rand::thread_rng();
    let range = ::rand::distributions::Range::new(0, 5);
    let index = range.ind_sample(&mut rng);

    match index {
        0 => ::authority::Authority::ClientManager(name),
        1 => ::authority::Authority::NaeManager(name),
        2 => ::authority::Authority::NodeManager(name),
        3 => ::authority::Authority::ManagedNode(name),
        4 => {
            ::authority::Authority::Client {
                client_key: key.clone(),
                proxy_node_name: name,
            }
        }
        _ => unreachable!(),
    }
}

fn generate_random_data(public_sign_key: &::sodiumoxide::crypto::sign::PublicKey,
                        secret_sign_key: &::sodiumoxide::crypto::sign::SecretKey)
                        -> ::data::Data {
    use rand::distributions::IndependentSample;

    let mut rng = ::rand::thread_rng();
    let range = ::rand::distributions::Range::new(0, 3);
    let index = range.ind_sample(&mut rng);

    match index {
        0 => {
            let structured_data =
                match ::structured_data::StructuredData::new(0,
                                                             ::rand::random(),
                                                             0,
                                                             vec![],
                                                             vec![public_sign_key.clone()],
                                                             vec![],
                                                             Some(&secret_sign_key)) {
                    Ok(structured_data) => structured_data,
                    Err(error) => panic!("StructuredData error: {:?}", error),
                };
            ::data::Data::StructuredData(structured_data)
        }
        1 => {
            let type_tag = ::immutable_data::ImmutableDataType::Normal;
            let immutable_data =
                ::immutable_data::ImmutableData::new(type_tag,
                                                     ::types::generate_random_vec_u8(1025));
            ::data::Data::ImmutableData(immutable_data)
        }
        2 => {
            let plain_data = ::plain_data::PlainData::new(rand::random(),
                                                          ::types::generate_random_vec_u8(1025));
            ::data::Data::PlainData(plain_data)
        }
        _ => panic!("Unexpected index."),
    }
}

/// Semi-random routing message.
// TODO Randomize Content and rename to random_routing_message.
pub fn arbitrary_routing_message(public_key: &::sodiumoxide::crypto::sign::PublicKey,
                                 secret_key: &::sodiumoxide::crypto::sign::SecretKey)
                                 -> ::messages::RequestMessage {
    let source_authority = generate_random_authority(rand::random(), public_key);
    let destination_authority = generate_random_authority(rand::random(), public_key);
    let data = generate_random_data(public_key, secret_key);
    let content = RequestContent::Put(data);

    RequestMessage {
        src: source_authority,
        dst: destination_authority,
        content: content,
    }
}

#[cfg(test)]
pub mod test {
    use rand;

    // TODO: Use IPv6 and non-TCP
    pub fn random_socket_addr() -> ::std::net::SocketAddr {
        ::std::net::SocketAddr::V4(::std::net::SocketAddrV4::new(
            ::std::net::Ipv4Addr::new(::rand::random::<u8>(),
                                      ::rand::random::<u8>(),
                                      ::rand::random::<u8>(),
                                      ::rand::random::<u8>()),
            ::rand::random::<u16>()))
    }

    pub fn random_endpoint() -> ::crust::Endpoint {
        // TODO: Udt
        ::crust::Endpoint::Tcp(random_socket_addr())
    }

    pub fn random_connection() -> ::crust::Connection {
        let local = random_socket_addr();
        let remote = random_socket_addr();
        // TODO: Udt
        ::crust::Connection::new(::crust::Protocol::Tcp, local, remote)
    }

    pub fn random_endpoints<R: rand::Rng>(rng: &mut R) -> Vec<::crust::Endpoint> {
        use rand::distributions::IndependentSample;
        let range = ::rand::distributions::Range::new(1, 10);
        let count = range.ind_sample(rng);
        let mut endpoints = vec![];
        for _ in 0..count {
            endpoints.push(random_endpoint());
        }
        endpoints
    }
}
