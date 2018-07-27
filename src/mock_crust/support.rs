// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use super::crust::compat::{ConnectionInfoResult, CrustEventSender, Event};
use super::crust::{CrustUser, PaAddr, PubConnectionInfo, LISTENER_PORT};
use maidsafe_utilities::SeededRng;
use rand::Rng;
use safe_crypto::PublicKeys;
use safe_crypto::{self, SecretKeys};
use std::cell::RefCell;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::rc::{Rc, Weak};
use CrustEvent;

/// Mock network. Create one before testing with mocks. Use it to create `ServiceHandle`s.
#[derive(Clone)]
pub struct Network(Rc<RefCell<NetworkImpl>>);

pub struct NetworkImpl {
    services: HashMap<Endpoint, Weak<RefCell<ServiceImpl>>>,
    min_section_size: usize,
    queue: BTreeMap<(Endpoint, Endpoint), VecDeque<Packet>>,
    blocked_connections: HashSet<(Endpoint, Endpoint)>,
    delayed_connections: HashSet<(Endpoint, Endpoint)>,
    rng: SeededRng,
    message_sent: bool,
}

impl Network {
    /// Create new mock Network.
    pub fn new(min_section_size: usize, optional_seed: Option<[u32; 4]>) -> Self {
        let mut rng = if let Some(seed) = optional_seed {
            SeededRng::from_seed(seed)
        } else {
            SeededRng::new()
        };
        unwrap!(safe_crypto::init_with_rng(&mut rng));
        Network(Rc::new(RefCell::new(NetworkImpl {
            services: HashMap::new(),
            min_section_size,
            queue: BTreeMap::new(),
            blocked_connections: HashSet::new(),
            delayed_connections: HashSet::new(),
            // Use `SeededRng::new()` here rather than passing in `rng`
            // so that a fresh one is used in every test, i.e. it will
            // not have been affected by initialising safe_crypto.
            rng: SeededRng::new(),
            message_sent: false,
        })))
    }

    /// Create new ServiceHandle.
    pub fn new_service_handle(
        &self,
        opt_config: Option<ConfigFile>,
        opt_endpoint: Option<Endpoint>,
    ) -> ServiceHandle {
        let config = opt_config.unwrap_or_else(ConfigFile::new);
        let endpoint = self.gen_endpoint(opt_endpoint);

        let handle = ServiceHandle::new(self.clone(), config, endpoint);
        assert!(
            self.0
                .borrow_mut()
                .services
                .insert(endpoint, Rc::downgrade(&handle.0))
                .is_none(),
            "Tried to insert duplicate service handle."
        );

        handle
    }

    /// Get min_section_size
    pub fn min_section_size(&self) -> usize {
        self.0.borrow().min_section_size
    }

    /// Generate unique `Endpoint`
    pub fn gen_endpoint(&self, opt_endpoint: Option<Endpoint>) -> Endpoint {
        opt_endpoint.unwrap_or_else(|| {
            let imp = self.0.borrow();
            for i in 0.. {
                if !imp.services.contains_key(&Endpoint(i)) {
                    return Endpoint(i);
                }
            }
            panic!("Unable to produce new Endpoint.");
        })
    }

    /// Generate a unique `Endpoint` which has an IP address of `ip`.  `ip` should be an IPv4
    /// address of the form 127.0.x.x which is the form returned by the `to_socket_addr()` function
    /// in this module.
    pub fn gen_endpoint_with_ip(&self, ip: &IpAddr) -> Endpoint {
        match *ip {
            IpAddr::V4(ipv4) => {
                let imp = self.0.borrow();
                assert_eq!(ipv4.octets()[0], 127);
                assert_eq!(ipv4.octets()[1], 0);
                let initial_value = ((ipv4.octets()[2] as usize) << 8) + ipv4.octets()[3] as usize;
                for i in 0..0xFF {
                    let endpoint = Endpoint(initial_value + (i << 16));
                    if !imp.services.contains_key(&endpoint) {
                        return endpoint;
                    }
                }
                panic!("No available endpoints left which will match {:?}", ip);
            }
            IpAddr::V6(_) => panic!("Expected IPv4 address."),
        }
    }

    /// Poll and process all queued Packets.
    pub fn deliver_messages(&self) {
        while let Some((sender, receiver, packet)) = self.pop_packet() {
            self.process_packet(sender, receiver, packet);
        }
    }

    /// Causes all packets from `sender` to `receiver` to fail.
    pub fn block_connection(&self, sender: Endpoint, receiver: Endpoint) {
        let mut imp = self.0.borrow_mut();
        let _ = imp.blocked_connections.insert((sender, receiver));
    }

    /// Make all packets from `sender` to `receiver` succeed.
    pub fn unblock_connection(&self, sender: Endpoint, receiver: Endpoint) {
        let mut imp = self.0.borrow_mut();
        let _ = imp.blocked_connections.remove(&(sender, receiver));
    }

    /// Delay the processing of packets from `sender` to `receiver`.
    pub fn delay_connection(&self, sender: Endpoint, receiver: Endpoint) {
        let mut imp = self.0.borrow_mut();
        let _ = imp.delayed_connections.insert((sender, receiver));
    }

    /// Simulates the loss of a connection.
    pub fn lost_connection(&self, node_1: Endpoint, node_2: Endpoint) {
        let service_1 = unwrap!(
            self.find_service(node_1),
            "Cannot fetch service of {:?}.",
            node_1
        );
        if service_1
            .borrow_mut()
            .remove_connection_by_endpoint(node_2)
            .is_none()
        {
            return;
        }
        let service_2 = unwrap!(
            self.find_service(node_2),
            "Cannot fetch service of {:?}.",
            node_2
        );
        let _ = service_2.borrow_mut().remove_connection_by_endpoint(node_1);

        service_1.borrow_mut().send_event(CrustEvent::LostPeer(
            unwrap!(service_2.borrow().full_id.as_ref())
                .public_keys()
                .clone(),
        ));
        service_2.borrow_mut().send_event(CrustEvent::LostPeer(
            unwrap!(service_1.borrow().full_id.as_ref())
                .public_keys()
                .clone(),
        ));
    }

    /// Simulates a crust event being sent to the node.
    pub fn send_crust_event(&self, node: Endpoint, crust_event: CrustEvent) {
        let service = unwrap!(
            self.find_service(node),
            "Cannot fetch service of {:?}.",
            node
        );
        service.borrow_mut().send_event(crust_event);
    }

    /// Construct a new [`SeededRng`][1] using a seed generated from random data provided by `self`.
    /// [1]: https://docs.rs/maidsafe_utilities/0.10.2/maidsafe_utilities/struct.SeededRng.html
    pub fn new_rng(&self) -> SeededRng {
        self.0.borrow_mut().rng.new_rng()
    }

    /// Return whether sent any message since previous query and reset the flag.
    pub fn reset_message_sent(&self) -> bool {
        let message_sent = self.0.borrow().message_sent;
        self.0.borrow_mut().message_sent = false;
        message_sent
    }

    fn connection_blocked(&self, sender: Endpoint, receiver: Endpoint) -> bool {
        self.0
            .borrow()
            .blocked_connections
            .contains(&(sender, receiver))
    }

    fn send(&self, sender: Endpoint, receiver: Endpoint, packet: Packet) {
        let mut network_impl = self.0.borrow_mut();
        network_impl.message_sent = true;
        network_impl
            .queue
            .entry((sender, receiver))
            .or_insert_with(VecDeque::new)
            .push_back(packet);
    }

    // Drops any pending messages on a specific route (does not automatically
    // drop packets going the other way).
    fn drop_pending(&self, sender: Endpoint, receiver: Endpoint) {
        let _ = self.0.borrow_mut().queue.remove(&(sender, receiver));
    }

    fn pop_packet(&self) -> Option<(Endpoint, Endpoint, Packet)> {
        let mut network_impl = self.0.borrow_mut();
        let keys: Vec<_> = {
            let checker = |&(s, r)| network_impl.delayed_connections.contains(&(s, r));
            if network_impl.queue.keys().all(checker) {
                network_impl.queue.keys().cloned().collect()
            } else {
                network_impl
                    .queue
                    .keys()
                    .filter(|&&(ref s, ref r)| {
                        !network_impl.delayed_connections.contains(&(*s, *r))
                    })
                    .cloned()
                    .collect()
            }
        };

        let (sender, receiver) = if let Some(key) = network_impl.rng.choose(&keys) {
            *key
        } else {
            return None;
        };
        let result = network_impl
            .queue
            .get_mut(&(sender, receiver))
            .and_then(|packets| packets.pop_front().map(|packet| (sender, receiver, packet)));
        if result.is_some() {
            if let Entry::Occupied(entry) = network_impl.queue.entry((sender, receiver)) {
                if entry.get().is_empty() {
                    let (_key, _value) = entry.remove_entry();
                }
            }
        }
        result
    }

    fn process_packet(&self, sender: Endpoint, receiver: Endpoint, packet: Packet) {
        if self.connection_blocked(sender, receiver) {
            if let Some(failure) = packet.to_failure() {
                self.send(receiver, sender, failure);
                return;
            }
        }

        if let Some(service) = self.find_service(receiver) {
            service.borrow_mut().receive_packet(sender, packet);
        } else if let Some(failure) = packet.to_failure() {
            // Packet was sent to a non-existing receiver.
            self.send(receiver, sender, failure);
        }
    }

    fn find_service(&self, endpoint: Endpoint) -> Option<Rc<RefCell<ServiceImpl>>> {
        self.0
            .borrow()
            .services
            .get(&endpoint)
            .and_then(|s| s.upgrade())
    }
}

/// `ServiceHandle` is associated with the mock `Service` and allows to configure
/// and instrument it.
#[derive(Clone)]
pub struct ServiceHandle(pub Rc<RefCell<ServiceImpl>>);

impl ServiceHandle {
    fn new(network: Network, config: ConfigFile, endpoint: Endpoint) -> Self {
        ServiceHandle(Rc::new(RefCell::new(ServiceImpl::new(
            network, config, endpoint,
        ))))
    }

    /// Endpoint of the `Service` bound to this handle.
    pub fn endpoint(&self) -> Endpoint {
        self.0.borrow().endpoint
    }

    /// Returns `true` if this service is connected to the given one.
    pub fn is_connected(&self, handle: &Self) -> bool {
        self.0.borrow().is_peer_connected(
            &unwrap!(handle.0.borrow().full_id.as_ref())
                .public_keys()
                .clone(),
        )
    }

    /// Delivers all messages that are queued in the network.
    pub fn deliver_messages(&self) {
        let network = self.0.borrow().network.clone();
        network.deliver_messages();
    }

    /// Returns whether sent any message across the network since previous query and reset the flag.
    pub fn reset_message_sent(&self) -> bool {
        self.0.borrow().network.reset_message_sent()
    }
}

pub struct ServiceImpl {
    pub network: Network,
    endpoint: Endpoint,
    pub full_id: Option<SecretKeys>,
    config: ConfigFile,
    pub accept_bootstrap: bool,
    pub listening_tcp: bool,
    event_sender: Option<CrustEventSender>,
    pending_bootstraps: u64,
    connections: Vec<(PublicKeys, Endpoint, CrustUser)>,
}

impl ServiceImpl {
    fn new(network: Network, config: ConfigFile, endpoint: Endpoint) -> Self {
        ServiceImpl {
            network,
            endpoint,
            full_id: None,
            config,
            accept_bootstrap: false,
            listening_tcp: false,
            event_sender: None,
            pending_bootstraps: 0,
            connections: Vec::new(),
        }
    }

    pub fn start(&mut self, event_sender: CrustEventSender, full_id: SecretKeys) {
        self.full_id = Some(full_id);
        self.event_sender = Some(event_sender);
    }

    pub fn restart(&mut self, event_sender: CrustEventSender, full_id: SecretKeys) {
        trace!("{:?} restart", self.endpoint);

        self.disconnect_all();

        self.full_id = Some(full_id.clone());
        self.listening_tcp = false;

        self.start(event_sender, full_id)
    }

    pub fn start_bootstrap(&mut self, blacklist: HashSet<PaAddr>, kind: CrustUser) {
        let mut pending_bootstraps = 0;

        for endpoint in &self.config.hard_coded_contacts {
            if *endpoint != self.endpoint && !blacklist.contains(&to_pa_addr(endpoint)) {
                self.send_packet(
                    *endpoint,
                    Packet::BootstrapRequest(
                        unwrap!(self.full_id.as_ref()).public_keys().clone(),
                        kind,
                    ),
                );
                pending_bootstraps += 1;
            }
        }

        // If we have no contacts in the config, we can fire BootstrapFailed
        // immediately.
        if pending_bootstraps == 0 {
            unwrap!(unwrap!(self.event_sender.as_ref()).send(Event::BootstrapFailed));
        }

        self.pending_bootstraps = pending_bootstraps;
    }

    pub fn get_peer_ip_addr(&self, pub_id: &PublicKeys) -> Option<IpAddr> {
        if let Some(endpoint) = self.find_endpoint_by_uid(pub_id) {
            Some(to_socket_addr(&endpoint).ip())
        } else {
            None
        }
    }

    pub fn send_message(&self, pub_id: &PublicKeys, data: Vec<u8>) -> bool {
        if let Some(endpoint) = self.find_endpoint_by_uid(pub_id) {
            self.send_packet(endpoint, Packet::Message(data));
            true
        } else {
            false
        }
    }

    pub fn is_peer_connected(&self, pub_id: &PublicKeys) -> bool {
        self.find_endpoint_by_uid(pub_id).is_some()
    }

    pub fn prepare_connection_info(&self, result_token: u32) {
        // TODO: should we also simulate failure here?
        // TODO: should we simulate asynchrony here?

        let result = ConnectionInfoResult {
            result_token,
            result: Ok(PubConnectionInfo {
                id: unwrap!(self.full_id.as_ref()).public_keys().clone(),
                endpoint: self.endpoint,
            }),
        };

        self.send_event(CrustEvent::ConnectionInfoPrepared(result));
    }

    pub fn connect(&self, _our_info: PubConnectionInfo, their_info: PubConnectionInfo) {
        let packet = Packet::ConnectRequest(
            unwrap!(self.full_id.as_ref()).public_keys().clone(),
            their_info.id,
        );
        self.send_packet(their_info.endpoint, packet);
    }

    pub fn set_accept_bootstrap(&mut self, accept: bool) {
        self.accept_bootstrap = accept;
    }

    pub fn start_listening(&mut self) {
        self.listening_tcp = true;
        let addr = PaAddr::Tcp(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(127, 0, 0, 1),
            LISTENER_PORT,
        )));
        self.send_event(CrustEvent::ListenerStarted(addr));
    }

    fn send_packet(&self, receiver: Endpoint, packet: Packet) {
        self.network.send(self.endpoint, receiver, packet);
    }

    fn receive_packet(&mut self, sender: Endpoint, packet: Packet) {
        match packet {
            Packet::BootstrapRequest(uid, kind) => self.handle_bootstrap_request(sender, uid, kind),
            Packet::BootstrapSuccess(uid) => self.handle_bootstrap_success(sender, uid),
            Packet::BootstrapFailure => self.handle_bootstrap_failure(sender),
            Packet::ConnectRequest(their_id, _) => self.handle_connect_request(sender, their_id),
            Packet::ConnectSuccess(their_id, _) => self.handle_connect_success(sender, their_id),
            Packet::ConnectFailure(their_id, _) => self.handle_connect_failure(sender, their_id),
            Packet::Message(data) => self.handle_message(sender, data),
            Packet::Disconnect => self.handle_disconnect(sender),
        }
    }

    fn handle_bootstrap_request(
        &mut self,
        peer_endpoint: Endpoint,
        pub_id: PublicKeys,
        kind: CrustUser,
    ) {
        if self.is_listening() && self.accept_bootstrap {
            self.handle_bootstrap_accept(peer_endpoint, pub_id, kind);
            self.send_packet(
                peer_endpoint,
                Packet::BootstrapSuccess(unwrap!(self.full_id.as_ref()).public_keys().clone()),
            );
        } else {
            self.send_packet(peer_endpoint, Packet::BootstrapFailure);
        }
    }

    fn handle_bootstrap_accept(
        &mut self,
        peer_endpoint: Endpoint,
        pub_id: PublicKeys,
        kind: CrustUser,
    ) {
        let _ = self.add_connection(pub_id.clone(), peer_endpoint, kind);
        self.send_event(CrustEvent::BootstrapAccept(pub_id, kind));
    }

    fn handle_bootstrap_success(&mut self, peer_endpoint: Endpoint, pub_id: PublicKeys) {
        let _ = self.add_connection(pub_id.clone(), peer_endpoint, CrustUser::Node);
        self.send_event(CrustEvent::BootstrapConnect(
            pub_id,
            to_pa_addr(&peer_endpoint),
        ));
        self.decrement_pending_bootstraps();
    }

    fn handle_bootstrap_failure(&mut self, _peer_endpoint: Endpoint) {
        self.decrement_pending_bootstraps();
    }

    fn handle_connect_request(&mut self, peer_endpoint: Endpoint, their_pub_id: PublicKeys) {
        if self.is_connected(&peer_endpoint, &their_pub_id) {
            return;
        }

        self.add_rendezvous_connection(their_pub_id.clone(), peer_endpoint);
        self.send_packet(
            peer_endpoint,
            Packet::ConnectSuccess(
                unwrap!(self.full_id.as_ref()).public_keys().clone(),
                their_pub_id,
            ),
        );
    }

    fn handle_connect_success(&mut self, peer_endpoint: Endpoint, their_pub_id: PublicKeys) {
        self.add_rendezvous_connection(their_pub_id, peer_endpoint);
    }

    fn handle_connect_failure(&self, _peer_endpoint: Endpoint, their_pub_id: PublicKeys) {
        self.send_event(CrustEvent::ConnectFailure(their_pub_id));
    }

    fn handle_message(&self, peer_endpoint: Endpoint, data: Vec<u8>) {
        if let Some((uid, kind)) = self.find_uid_and_kind_by_endpoint(&peer_endpoint) {
            self.send_event(CrustEvent::NewMessage(uid, kind, data));
        } else {
            debug!("Received message from non-connected {:?}", peer_endpoint);
        }
    }

    fn handle_disconnect(&mut self, peer_endpoint: Endpoint) {
        if let Some(uid) = self.remove_connection_by_endpoint(peer_endpoint) {
            self.send_event(CrustEvent::LostPeer(uid));
        }
    }

    fn send_event(&self, event: CrustEvent) {
        let sender = unwrap!(self.event_sender.as_ref(), "Could not get event sender.");
        unwrap!(sender.send(event));
    }

    fn is_listening(&self) -> bool {
        self.listening_tcp
    }

    fn decrement_pending_bootstraps(&mut self) {
        if self.pending_bootstraps == 0 {
            return;
        }

        self.pending_bootstraps -= 1;

        if self.pending_bootstraps == 0 && self.connections.is_empty() {
            self.send_event(CrustEvent::BootstrapFailed);
        }
    }

    fn add_connection(
        &mut self,
        pub_id: PublicKeys,
        peer_endpoint: Endpoint,
        kind: CrustUser,
    ) -> bool {
        let checker =
            |&(ref id, ep, _): &(PublicKeys, _, _)| id.clone() == pub_id && ep == peer_endpoint;
        if self.connections.iter().any(checker) {
            // Connection already exists
            return false;
        }

        self.connections.push((pub_id.clone(), peer_endpoint, kind));
        true
    }

    fn add_rendezvous_connection(&mut self, pub_id: PublicKeys, peer_endpoint: Endpoint) {
        if self.add_connection(pub_id.clone(), peer_endpoint, CrustUser::Node) {
            self.send_event(CrustEvent::ConnectSuccess(pub_id));
        }
    }

    // Remove connected peer with the given uid and return its endpoint,
    // or None if no such peer exists.
    fn remove_connection_by_uid(&mut self, pub_id: &PublicKeys) -> Option<Endpoint> {
        if let Some(i) = self
            .connections
            .iter()
            .position(|&(ref id, _, _)| id == pub_id)
        {
            Some(self.connections.swap_remove(i).1)
        } else {
            None
        }
    }

    fn remove_connection_by_endpoint(&mut self, endpoint: Endpoint) -> Option<PublicKeys> {
        if let Some(i) = self
            .connections
            .iter()
            .position(|&(_, ep, _)| ep == endpoint)
        {
            Some(self.connections.swap_remove(i).0)
        } else {
            None
        }
    }

    fn find_endpoint_by_uid(&self, pub_id: &PublicKeys) -> Option<Endpoint> {
        self.connections
            .iter()
            .find(|&&(ref id, _, _)| id == pub_id)
            .map(|&(_, ep, _)| ep)
    }

    fn find_uid_and_kind_by_endpoint(
        &self,
        endpoint: &Endpoint,
    ) -> Option<(PublicKeys, CrustUser)> {
        self.connections
            .iter()
            .find(|&&(_, ep, _)| ep == *endpoint)
            .map(|&(ref id, _, user)| (id.clone(), user))
    }

    fn is_connected(&self, endpoint: &Endpoint, pub_id: &PublicKeys) -> bool {
        self.connections
            .iter()
            .any(|conn| conn.0 == *pub_id && conn.1 == *endpoint)
    }

    pub fn disconnect(&mut self, pub_id: &PublicKeys) -> bool {
        if let Some(endpoint) = self.remove_connection_by_uid(pub_id) {
            // We immediately drop all messages going in both directions. This is
            // possibly not realistic since in the real CRust some of these might
            // have already been sent and still be received by the far end, but
            // this is a worst case for routing to deal with.
            self.network.drop_pending(self.endpoint, endpoint);
            self.network.drop_pending(endpoint, self.endpoint);

            // Now send a new message to tell the other end to disconnect.
            self.send_packet(endpoint, Packet::Disconnect);
            true
        } else {
            false
        }
    }

    pub fn disconnect_all(&mut self) {
        let endpoints = self
            .connections
            .drain(..)
            .map(|(_, ep, _)| ep)
            .collect::<Vec<_>>();

        for endpoint in endpoints {
            self.send_packet(endpoint, Packet::Disconnect);
        }
    }
}

impl Drop for ServiceImpl {
    fn drop(&mut self) {
        self.disconnect_all();
    }
}

/// Creates a `SocketAddr` with the endpoint as its port, so that endpoints and addresses can be
/// easily mapped to each other during testing.
pub fn to_socket_addr(endpoint: &Endpoint) -> SocketAddr {
    SocketAddr::new(
        IpAddr::from([127, 0, (endpoint.0 >> 8) as u8, endpoint.0 as u8]),
        endpoint.0 as u16,
    )
}

/// Creates a `PaAddr` based on this endpoint.
pub fn to_pa_addr(endpoint: &Endpoint) -> PaAddr {
    PaAddr::Tcp(to_socket_addr(endpoint))
}

/// Simulated crust config file.
#[derive(Clone)]
pub struct ConfigFile {
    /// Contacts to bootstrap against.
    pub hard_coded_contacts: Vec<Endpoint>,
}

impl ConfigFile {
    /// Create default `ConfigFile`.
    pub fn new() -> Self {
        Self::with_contacts(&[])
    }

    /// Create `ConfigFile` with the given hardcoded contacts.
    pub fn with_contacts(contacts: &[Endpoint]) -> Self {
        ConfigFile {
            hard_coded_contacts: contacts.into_iter().cloned().collect(),
        }
    }
}

impl Default for ConfigFile {
    fn default() -> ConfigFile {
        ConfigFile::new()
    }
}

/// Simulated network endpoint (think socket address). This is used to identify
/// and address `Service`s in the mock network.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct Endpoint(pub usize);

#[derive(Clone, Debug)]
enum Packet {
    BootstrapRequest(PublicKeys, CrustUser),
    BootstrapSuccess(PublicKeys),
    BootstrapFailure,

    ConnectRequest(PublicKeys, PublicKeys),
    ConnectSuccess(PublicKeys, PublicKeys),
    ConnectFailure(PublicKeys, PublicKeys),

    Message(Vec<u8>),
    Disconnect,
}

impl Packet {
    // Given a request packet, returns the corresponding failure packet.
    fn to_failure(&self) -> Option<Packet> {
        match *self {
            Packet::BootstrapRequest(..) => Some(Packet::BootstrapFailure),
            Packet::ConnectRequest(ref our_id, ref their_id) => {
                Some(Packet::ConnectFailure(their_id.clone(), our_id.clone()))
            }
            _ => None,
        }
    }
}

// The following code facilitates passing ServiceHandles to mock Services, so we
// don't need separate test and non-test version of `routing::Core::new`.
thread_local! {
    static CURRENT: RefCell<Option<ServiceHandle>> = RefCell::new(None)
}

/// Make the `ServiceHandle` current so it can be picked up by mock `Service`s created
/// inside the passed-in lambda.
pub fn make_current<F, R>(handle: &ServiceHandle, f: F) -> R
where
    F: FnOnce() -> R,
{
    CURRENT.with(|current| {
        *current.borrow_mut() = Some(handle.clone());
        let result = f();
        *current.borrow_mut() = None;
        result
    })
}

/// Unsets and returns the `ServiceHandle` set with `make_current`.
pub fn take_current() -> ServiceHandle {
    CURRENT.with(|current| {
        unwrap!(
            current.borrow_mut().take(),
            "Current ServiceHandle is not set."
        )
    })
}

/// Invokes the given lambda with a reference to the `ServiceHandle` set with `make_current`.
pub fn with_current<F, R>(f: F) -> R
where
    F: FnOnce(&ServiceHandle) -> R,
{
    CURRENT.with(|current| {
        f(unwrap!(
            current.borrow_mut().as_ref(),
            "Current ServiceHandle is not set."
        ))
    })
}
