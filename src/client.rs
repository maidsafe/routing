// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

#[cfg(feature = "mock_base")]
use crate::event_stream::{EventStepper, EventStream};
use crate::{
    action::Action,
    cache::NullCache,
    config_handler::{self, Config},
    data::{EntryAction, ImmutableData, MutableData, PermissionSet, User},
    error::{InterfaceError, RoutingError},
    event::Event,
    id::{FullId, PublicId},
    messages::{Request, CLIENT_GET_PRIORITY, DEFAULT_PRIORITY},
    outbox::{EventBox, EventBuf},
    quic_p2p::OurType,
    routing_table::Authority,
    state_machine::{State, StateMachine},
    states::{BootstrappingPeer, TargetState},
    types::MessageId,
    xor_name::XorName,
    NetworkConfig, MIN_SECTION_SIZE,
};
use crossbeam_channel as mpmc;
#[cfg(not(feature = "mock_base"))]
use maidsafe_utilities::thread::{self, Joiner};
#[cfg(not(feature = "mock_base"))]
use safe_crypto;
use crate::ed25519::PublicKey;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::mpsc,
    time::Duration,
};
#[cfg(not(feature = "mock_base"))]
use unwrap::unwrap;

/// Interface for sending and receiving messages to and from a network of nodes in the role of a
/// client.
///
/// A client is connected to the network via one or more nodes. Messages are never routed via a
/// client, and a client cannot be part of a section authority.
pub struct Client {
    interface_result_tx: mpsc::Sender<Result<(), InterfaceError>>,
    interface_result_rx: mpsc::Receiver<Result<(), InterfaceError>>,

    #[cfg(not(feature = "mock_base"))]
    action_sender: mpmc::Sender<Action>,
    #[cfg(not(feature = "mock_base"))]
    _joiner: Joiner,

    #[cfg(feature = "mock_base")]
    machine: StateMachine,
    #[cfg(feature = "mock_base")]
    event_buffer: EventBuf,
}

impl Client {
    fn make_state_machine(
        keys: Option<FullId>,
        outbox: &mut dyn EventBox,
        mut network_config: NetworkConfig,
        config: Option<Config>,
        msg_expiry_dur: Duration,
    ) -> (mpmc::Sender<Action>, StateMachine) {
        let full_id = keys.unwrap_or_else(FullId::new);
        let config = config.unwrap_or_else(config_handler::get_config);
        let dev_config = config.dev.unwrap_or_default();
        let min_section_size = dev_config.min_section_size.unwrap_or(MIN_SECTION_SIZE);

        network_config.our_type = OurType::Client;

        StateMachine::new(
            move |action_sender, network_service, timer, _outbox2| {
                State::BootstrappingPeer(BootstrappingPeer::new(
                    action_sender,
                    Box::new(NullCache),
                    TargetState::Client { msg_expiry_dur },
                    network_service,
                    full_id,
                    min_section_size,
                    timer,
                ))
            },
            network_config,
            outbox,
        )
    }

    /// Gets MAID account information.
    pub fn get_account_info(
        &mut self,
        dst: Authority<XorName>,
        msg_id: MessageId,
    ) -> Result<(), InterfaceError> {
        let request = Request::GetAccountInfo(msg_id);
        self.send_request(dst, request, CLIENT_GET_PRIORITY)
    }

    /// Puts ImmutableData to the network
    pub fn put_idata(
        &mut self,
        dst: Authority<XorName>,
        data: ImmutableData,
        msg_id: MessageId,
    ) -> Result<(), InterfaceError> {
        let request = Request::PutIData {
            data: data,
            msg_id: msg_id,
        };

        self.send_request(dst, request, DEFAULT_PRIORITY)
    }

    /// Fetches ImmutableData from the network by the given name.
    pub fn get_idata(
        &mut self,
        dst: Authority<XorName>,
        name: XorName,
        msg_id: MessageId,
    ) -> Result<(), InterfaceError> {
        let request = Request::GetIData {
            name: name,
            msg_id: msg_id,
        };

        self.send_request(dst, request, CLIENT_GET_PRIORITY)
    }

    /// Fetches a latest version number of the provided MutableData
    pub fn get_mdata_version(
        &mut self,
        dst: Authority<XorName>,
        name: XorName,
        tag: u64,
        msg_id: MessageId,
    ) -> Result<(), InterfaceError> {
        let request = Request::GetMDataVersion {
            name: name,
            tag: tag,
            msg_id: msg_id,
        };

        self.send_request(dst, request, CLIENT_GET_PRIORITY)
    }

    /// Fetches the shell of the provided MutableData
    pub fn get_mdata_shell(
        &mut self,
        dst: Authority<XorName>,
        name: XorName,
        tag: u64,
        msg_id: MessageId,
    ) -> Result<(), InterfaceError> {
        let request = Request::GetMDataShell {
            name: name,
            tag: tag,
            msg_id: msg_id,
        };

        self.send_request(dst, request, CLIENT_GET_PRIORITY)
    }

    /// Fetches the entire MutableData
    pub fn get_mdata(
        &mut self,
        dst: Authority<XorName>,
        name: XorName,
        tag: u64,
        msg_id: MessageId,
    ) -> Result<(), InterfaceError> {
        let request = Request::GetMData {
            name: name,
            tag: tag,
            msg_id: msg_id,
        };

        self.send_request(dst, request, CLIENT_GET_PRIORITY)
    }

    /// Fetches a list of entries (keys + values) of the provided MutableData
    /// Note: response to this request is unlikely to accumulate during churn.
    pub fn list_mdata_entries(
        &mut self,
        dst: Authority<XorName>,
        name: XorName,
        tag: u64,
        msg_id: MessageId,
    ) -> Result<(), InterfaceError> {
        let request = Request::ListMDataEntries {
            name: name,
            tag: tag,
            msg_id: msg_id,
        };

        self.send_request(dst, request, CLIENT_GET_PRIORITY)
    }

    /// Fetches a list of keys of the provided MutableData
    /// Note: response to this request is unlikely to accumulate during churn.
    pub fn list_mdata_keys(
        &mut self,
        dst: Authority<XorName>,
        name: XorName,
        tag: u64,
        msg_id: MessageId,
    ) -> Result<(), InterfaceError> {
        let request = Request::ListMDataKeys {
            name: name,
            tag: tag,
            msg_id: msg_id,
        };

        self.send_request(dst, request, CLIENT_GET_PRIORITY)
    }

    /// Fetches a list of values of the provided MutableData
    /// Note: response to this request is unlikely to accumulate during churn.
    pub fn list_mdata_values(
        &mut self,
        dst: Authority<XorName>,
        name: XorName,
        tag: u64,
        msg_id: MessageId,
    ) -> Result<(), InterfaceError> {
        let request = Request::ListMDataValues {
            name: name,
            tag: tag,
            msg_id: msg_id,
        };

        self.send_request(dst, request, CLIENT_GET_PRIORITY)
    }

    /// Fetches a single value from the provided MutableData by the given key
    pub fn get_mdata_value(
        &mut self,
        dst: Authority<XorName>,
        name: XorName,
        tag: u64,
        key: Vec<u8>,
        msg_id: MessageId,
    ) -> Result<(), InterfaceError> {
        let request = Request::GetMDataValue {
            name: name,
            tag: tag,
            key: key,
            msg_id: msg_id,
        };

        self.send_request(dst, request, CLIENT_GET_PRIORITY)
    }

    /// Creates a new `MutableData` in the network
    pub fn put_mdata(
        &mut self,
        dst: Authority<XorName>,
        data: MutableData,
        msg_id: MessageId,
        requester: PublicKey,
    ) -> Result<(), InterfaceError> {
        let request = Request::PutMData {
            data: data,
            msg_id: msg_id,
            requester: requester,
        };

        self.send_request(dst, request, DEFAULT_PRIORITY)
    }

    /// Updates `MutableData` entries in bulk
    pub fn mutate_mdata_entries(
        &mut self,
        dst: Authority<XorName>,
        name: XorName,
        tag: u64,
        actions: BTreeMap<Vec<u8>, EntryAction>,
        msg_id: MessageId,
        requester: PublicKey,
    ) -> Result<(), InterfaceError> {
        let request = Request::MutateMDataEntries {
            name: name,
            tag: tag,
            actions: actions,
            msg_id: msg_id,
            requester: requester,
        };

        self.send_request(dst, request, DEFAULT_PRIORITY)
    }

    /// Lists all permissions for a given `MutableData`
    pub fn list_mdata_permissions(
        &mut self,
        dst: Authority<XorName>,
        name: XorName,
        tag: u64,
        msg_id: MessageId,
    ) -> Result<(), InterfaceError> {
        let request = Request::ListMDataPermissions {
            name: name,
            tag: tag,
            msg_id: msg_id,
        };

        self.send_request(dst, request, CLIENT_GET_PRIORITY)
    }

    /// Lists a permission set for a given user
    pub fn list_mdata_user_permissions(
        &mut self,
        dst: Authority<XorName>,
        name: XorName,
        tag: u64,
        user: User,
        msg_id: MessageId,
    ) -> Result<(), InterfaceError> {
        let request = Request::ListMDataUserPermissions {
            name: name,
            tag: tag,
            user: user,
            msg_id: msg_id,
        };

        self.send_request(dst, request, CLIENT_GET_PRIORITY)
    }

    /// Updates or inserts a permission set for a given user
    #[allow(clippy::too_many_arguments)]
    pub fn set_mdata_user_permissions(
        &mut self,
        dst: Authority<XorName>,
        name: XorName,
        tag: u64,
        user: User,
        permissions: PermissionSet,
        version: u64,
        msg_id: MessageId,
        requester: PublicKey,
    ) -> Result<(), InterfaceError> {
        let request = Request::SetMDataUserPermissions {
            name: name,
            tag: tag,
            user: user,
            permissions: permissions,
            version: version,
            msg_id: msg_id,
            requester: requester,
        };

        self.send_request(dst, request, DEFAULT_PRIORITY)
    }

    /// Deletes a permission set for a given user
    #[allow(clippy::too_many_arguments)]
    pub fn delete_mdata_user_permissions(
        &mut self,
        dst: Authority<XorName>,
        name: XorName,
        tag: u64,
        user: User,
        version: u64,
        msg_id: MessageId,
        requester: PublicKey,
    ) -> Result<(), InterfaceError> {
        let request = Request::DeleteMDataUserPermissions {
            name: name,
            tag: tag,
            user: user,
            version: version,
            msg_id: msg_id,
            requester: requester,
        };

        self.send_request(dst, request, DEFAULT_PRIORITY)
    }

    /// Sends an ownership transfer request
    pub fn change_mdata_owner(
        &mut self,
        dst: Authority<XorName>,
        name: XorName,
        tag: u64,
        new_owners: BTreeSet<PublicKey>,
        version: u64,
        msg_id: MessageId,
    ) -> Result<(), InterfaceError> {
        let request = Request::ChangeMDataOwner {
            name: name,
            tag: tag,
            new_owners: new_owners,
            version: version,
            msg_id: msg_id,
        };

        self.send_request(dst, request, DEFAULT_PRIORITY)
    }

    /// Fetches a list of authorised keys and version in MaidManager
    pub fn list_auth_keys_and_version(
        &mut self,
        dst: Authority<XorName>,
        message_id: MessageId,
    ) -> Result<(), InterfaceError> {
        let request = Request::ListAuthKeysAndVersion(message_id);
        self.send_request(dst, request, CLIENT_GET_PRIORITY)
    }

    /// Adds a new authorised key to MaidManager
    pub fn insert_auth_key(
        &mut self,
        dst: Authority<XorName>,
        key: PublicKey,
        version: u64,
        message_id: MessageId,
    ) -> Result<(), InterfaceError> {
        let request = Request::InsertAuthKey {
            key: key,
            version: version,
            msg_id: message_id,
        };

        self.send_request(dst, request, DEFAULT_PRIORITY)
    }

    /// Removes an authorised key from MaidManager
    pub fn delete_auth_key(
        &mut self,
        dst: Authority<XorName>,
        key: PublicKey,
        version: u64,
        message_id: MessageId,
    ) -> Result<(), InterfaceError> {
        let request = Request::DeleteAuthKey {
            key: key,
            version: version,
            msg_id: message_id,
        };

        self.send_request(dst, request, DEFAULT_PRIORITY)
    }
}

#[cfg(not(feature = "mock_base"))]
impl Client {
    /// Create a new `Client`.
    ///
    /// It will automatically connect to the network, but not attempt to achieve full routing node
    /// status. The name of the client will be the name of the `PublicId` of the `keys` and must
    /// equal the SHA512 hash of its public signing key, otherwise the client will be instantly
    /// terminated.
    ///
    /// Keys will be exchanged with the `ClientAuthority` so that communication with the network is
    /// cryptographically secure and uses section consensus. The restriction for the client name
    /// exists to ensure that the client cannot choose its `ClientAuthority`.
    pub fn new(
        event_sender: mpsc::Sender<Event>,
        keys: Option<FullId>,
        network_config: Option<NetworkConfig>,
        msg_expiry_dur: Duration,
    ) -> Result<Client, RoutingError> {
        safe_crypto::init()?; // enable shared global (i.e. safe to multithread now)

        let (tx, rx) = mpsc::channel();
        let (get_action_sender_tx, get_action_sender_rx) = mpsc::channel();
        let network_config = network_config.unwrap_or_default();

        let joiner = thread::named("Client thread", move || {
            // start the handler for routing with a restriction to become a full node
            let mut event_buffer = EventBuf::new();
            let (action_sender, mut machine) = Self::make_state_machine(
                keys,
                &mut event_buffer,
                network_config,
                None,
                msg_expiry_dur,
            );

            for ev in event_buffer.take_all() {
                unwrap!(event_sender.send(ev));
            }

            unwrap!(get_action_sender_tx.send(action_sender));

            // Gather events from the state machine's event loop and proxy them over the
            // event_sender channel.
            while Ok(()) == machine.step(&mut event_buffer) {
                for ev in event_buffer.take_all() {
                    // If sending the event fails, terminate this thread.
                    if event_sender.send(ev).is_err() {
                        return;
                    }
                }
            }
            // When there are no more events to process, terminate this thread.
        });

        let action_sender = get_action_sender_rx
            .recv()
            .map_err(|_| RoutingError::NotBootstrapped)?;

        Ok(Client {
            interface_result_tx: tx,
            interface_result_rx: rx,
            action_sender: action_sender,
            _joiner: joiner,
        })
    }

    /// Returns the `PublicId` of this client.
    pub fn id(&self) -> Result<PublicId, InterfaceError> {
        let (result_tx, result_rx) = mpsc::channel();
        self.action_sender.send(Action::GetId { result_tx })?;
        Ok(result_rx.recv()?)
    }

    fn send_request(
        &self,
        dst: Authority<XorName>,
        request: Request,
        priority: u8,
    ) -> Result<(), InterfaceError> {
        let action = Action::ClientSendRequest {
            content: request,
            dst: dst,
            priority: priority,
            result_tx: self.interface_result_tx.clone(),
        };

        self.action_sender.send(action)?;
        self.interface_result_rx.recv()?
    }
}

#[cfg(feature = "mock_base")]
impl Client {
    /// Create a new `Client` for testing with mock network.
    pub fn new(
        keys: Option<FullId>,
        network_config: Option<NetworkConfig>,
        config: Config,
        msg_expiry_dur: Duration,
    ) -> Result<Client, RoutingError> {
        let network_config = network_config.unwrap_or_default();

        let mut event_buffer = EventBuf::new();
        let (_, machine) = Self::make_state_machine(
            keys,
            &mut event_buffer,
            network_config,
            Some(config),
            msg_expiry_dur,
        );

        let (tx, rx) = mpsc::channel();

        Ok(Client {
            interface_result_tx: tx,
            interface_result_rx: rx,
            machine: machine,
            event_buffer: event_buffer,
        })
    }

    /// Returns the name of this client.
    pub fn id(&self) -> Result<PublicId, RoutingError> {
        self.machine.current().id().ok_or(RoutingError::Terminated)
    }

    /// FIXME: Review the usage poll here
    pub fn send_request(
        &mut self,
        dst: Authority<XorName>,
        request: Request,
        priority: u8,
    ) -> Result<(), InterfaceError> {
        // Make sure the state machine has processed any outstanding network events.
        let _ = self.poll();

        let action = Action::ClientSendRequest {
            content: request,
            dst: dst,
            priority: priority,
            result_tx: self.interface_result_tx.clone(),
        };

        let transition = self
            .machine
            .current_mut()
            .handle_action(action, &mut self.event_buffer);
        self.machine
            .apply_transition(transition, &mut self.event_buffer);
        self.interface_result_rx.recv()?
    }
}

#[cfg(feature = "mock_base")]
impl EventStepper for Client {
    type Item = Event;

    fn produce_events(&mut self) -> Result<(), mpmc::RecvError> {
        self.machine.step(&mut self.event_buffer)
    }

    fn try_produce_events(&mut self) -> Result<(), mpmc::TryRecvError> {
        self.machine.try_step(&mut self.event_buffer)
    }

    fn pop_item(&mut self) -> Option<Event> {
        self.event_buffer.take_first()
    }
}

#[cfg(not(feature = "mock_base"))]
impl Drop for Client {
    fn drop(&mut self) {
        if let Err(err) = self.action_sender.send(Action::Terminate) {
            debug!("Error {:?} sending event to Core", err);
        }
    }
}

#[cfg(feature = "mock_base")]
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.poll();
        let _ = self
            .machine
            .current_mut()
            .handle_action(Action::Terminate, &mut self.event_buffer);
        let _ = self.event_buffer.take_all();
    }
}
