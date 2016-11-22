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

// These tests are almost straight up copied from crust::service::tests

use maidsafe_utilities::event_sender::{MaidSafeEventCategory, MaidSafeObserver};
use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver};
use super::crust::{CrustEventSender, Event, Service};
use super::support::{Config, Network};

fn get_event_sender() -> (CrustEventSender, Receiver<MaidSafeEventCategory>, Receiver<Event>) {
    let (category_tx, category_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();

    (MaidSafeObserver::new(event_tx, MaidSafeEventCategory::Crust, category_tx),
     category_rx,
     event_rx)
}

// Receive an event from the given receiver and asserts that it matches the
// given pattern.
macro_rules! expect_event {
    ($rx:expr, $pattern:pat) => {
        match unwrap!($rx.try_recv()) {
            $pattern => (),
            e => panic!("unexpected event {:?}", e),
        }
    };

    ($rx:expr, $pattern:pat => $arm:expr) => {
        match unwrap!($rx.try_recv()) {
            $pattern => $arm,
            e => panic!("unexpected event {:?}", e),
        }
    }
}

#[test]
fn start_two_services_bootstrap_communicate_exit() {
    let network = Network::new(None);
    let endpoint0 = network.gen_endpoint(None);
    let endpoint1 = network.gen_endpoint(None);
    let config = Config::with_contacts(&[endpoint0, endpoint1]);

    let handle0 = network.new_service_handle(Some(config.clone()), Some(endpoint0));
    let handle1 = network.new_service_handle(Some(config.clone()), Some(endpoint1));

    let (event_sender_0, _category_rx_0, event_rx_0) = get_event_sender();
    let (event_sender_1, _category_rx_1, event_rx_1) = get_event_sender();

    let mut service_0 = unwrap!(Service::with_handle(&handle0, event_sender_0));

    unwrap!(service_0.start_listening_tcp());
    expect_event!(event_rx_0, Event::ListenerStarted(..));

    service_0.start_service_discovery();

    let mut service_1 = unwrap!(Service::with_handle(&handle1, event_sender_1));

    unwrap!(service_1.start_bootstrap(HashSet::new()));
    let id_0 = expect_event!(event_rx_1, Event::BootstrapConnect(id, _) => id);
    let id_1 = expect_event!(event_rx_0, Event::BootstrapAccept(id) => id);

    assert!(id_0 != id_1);

    // send data from 0 to 1
    let data_sent = vec![0, 1, 255, 254, 222, 1];
    unwrap!(service_0.send(id_1, data_sent.clone(), 0));

    // 1 should rx data
    let (data_recvd, peer_id) = expect_event!(event_rx_1,
                      Event::NewMessage(their_id, msg) => (msg, their_id));

    assert_eq!(data_recvd, data_sent);
    assert_eq!(peer_id, id_0);

    // send data from 1 to 0
    let data_sent = vec![10, 11, 155, 214, 202];
    unwrap!(service_1.send(id_0, data_sent.clone(), 0));

    // 0 should rx data
    let (data_recvd, peer_id) = expect_event!(event_rx_0,
                      Event::NewMessage(their_id, msg) => (msg, their_id));

    assert_eq!(data_recvd, data_sent);
    assert_eq!(peer_id, id_1);

    assert!(service_0.disconnect(id_1));
    expect_event!(event_rx_1, Event::LostPeer(id) => assert_eq!(id, id_0));
}

#[test]
fn start_two_services_rendezvous_connect() {
    const PREPARE_CI_TOKEN: u32 = 1;

    let network = Network::new(None);
    let handle0 = network.new_service_handle(None, None);
    let handle1 = network.new_service_handle(None, None);

    let (event_sender_0, _category_rx_0, event_rx_0) = get_event_sender();
    let (event_sender_1, _category_rx_1, event_rx_1) = get_event_sender();

    let service_0 = unwrap!(Service::with_handle(&handle0, event_sender_0));
    let service_1 = unwrap!(Service::with_handle(&handle1, event_sender_1));

    service_0.prepare_connection_info(PREPARE_CI_TOKEN);
    let our_ci_0 = expect_event!(event_rx_0, Event::ConnectionInfoPrepared(cir) => {
        assert_eq!(cir.result_token, PREPARE_CI_TOKEN);
        unwrap!(cir.result)
    });

    service_1.prepare_connection_info(PREPARE_CI_TOKEN);
    let our_ci_1 = expect_event!(event_rx_1, Event::ConnectionInfoPrepared(cir) => {
        assert_eq!(cir.result_token, PREPARE_CI_TOKEN);
        unwrap!(cir.result)
    });

    let their_ci_0 = our_ci_0.to_pub_connection_info();
    let their_ci_1 = our_ci_1.to_pub_connection_info();

    unwrap!(service_0.connect(our_ci_0, their_ci_1));
    unwrap!(service_1.connect(our_ci_1, their_ci_0));

    let id_1 = expect_event!(event_rx_0, Event::ConnectSuccess(id) => id);
    let id_0 = expect_event!(event_rx_1, Event::ConnectSuccess(id) => id);

    // send data from 0 to 1
    let data_sent = vec![0, 1, 255, 254, 222, 1];
    unwrap!(service_0.send(id_1, data_sent.clone(), 0));

    // 1 should rx data
    let (data_recvd, peer_id) = expect_event!(event_rx_1,
                      Event::NewMessage(their_id, msg) => (msg, their_id));

    assert_eq!(data_recvd, data_sent);
    assert_eq!(peer_id, id_0);

    // send data from 1 to 0
    let data_sent = vec![10, 11, 155, 214, 202];
    unwrap!(service_1.send(id_0, data_sent.clone(), 0));

    // 0 should rx data
    let (data_recvd, peer_id) = expect_event!(event_rx_0,
                      Event::NewMessage(their_id, msg) => (msg, their_id));

    assert_eq!(data_recvd, data_sent);
    assert_eq!(peer_id, id_1);
}

#[test]
fn unidirectional_rendezvous_connect() {
    const PREPARE_CI_TOKEN: u32 = 1;

    let network = Network::new(None);
    let handle0 = network.new_service_handle(None, None);
    let handle1 = network.new_service_handle(None, None);

    let (event_tx_0, _category_rx_0, event_rx_0) = get_event_sender();
    let (event_tx_1, _category_rx_1, event_rx_1) = get_event_sender();

    let service_0 = unwrap!(Service::with_handle(&handle0, event_tx_0));
    let service_1 = unwrap!(Service::with_handle(&handle1, event_tx_1));

    service_0.prepare_connection_info(PREPARE_CI_TOKEN);
    let our_ci_0 = expect_event!(event_rx_0, Event::ConnectionInfoPrepared(cir) => {
        unwrap!(cir.result)
    });

    service_1.prepare_connection_info(PREPARE_CI_TOKEN);
    let our_ci_1 = expect_event!(event_rx_1, Event::ConnectionInfoPrepared(cir) => {
        unwrap!(cir.result)
    });

    let their_ci_1 = our_ci_1.to_pub_connection_info();

    unwrap!(service_0.connect(our_ci_0, their_ci_1));

    expect_event!(event_rx_0, Event::ConnectSuccess(_));
    expect_event!(event_rx_1, Event::ConnectSuccess(_));
}

#[test]
fn drop() {
    use std::mem;

    let network = Network::new(None);
    let handle0 = network.new_service_handle(None, None);

    let config = Config::with_contacts(&[handle0.endpoint()]);
    let handle1 = network.new_service_handle(Some(config), None);

    let (event_sender_0, _category_rx_0, event_rx_0) = get_event_sender();
    let (event_sender_1, _category_rx_1, event_rx_1) = get_event_sender();

    let mut service_0 = unwrap!(Service::with_handle(&handle0, event_sender_0));

    unwrap!(service_0.start_listening_tcp());
    expect_event!(event_rx_0, Event::ListenerStarted(_));

    let mut service_1 = unwrap!(Service::with_handle(&handle1, event_sender_1));
    unwrap!(service_1.start_bootstrap(HashSet::new()));

    let id_0 = expect_event!(event_rx_1, Event::BootstrapConnect(id, _) => id);
    expect_event!(event_rx_0, Event::BootstrapAccept(..));

    // Dropping service_0 should make service_1 receive a LostPeer event.
    mem::drop(service_0);
    expect_event!(event_rx_1, Event::LostPeer(id) => assert_eq!(id, id_0));
}
