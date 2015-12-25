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

use sodiumoxide;
use std::sync::mpsc::{Receiver, Sender, channel};

use action::Action;
use authority::Authority;
use core::Core;
use data::{Data, DataRequest};
use error::{InterfaceError, RoutingError};
use event::Event;
use messages::{RequestContent, RequestMessage, ResponseContent, ResponseMessage, RoutingMessage};
use sodiumoxide::crypto::hash::sha512;
use xor_name::XorName;

type RoutingResult = Result<(), RoutingError>;

/// Node provides an actionable interface to Core.  On constructing a new Node object a
/// Core will also be started.
pub struct Node {
    interface_result_tx: Sender<Result<(), InterfaceError>>,
    interface_result_rx: Receiver<Result<(), InterfaceError>>,
    action_sender: ::types::RoutingActionSender,
    _raii_joiner: ::maidsafe_utilities::thread::RaiiThreadJoiner,
}

impl Node {
    /// Starts a new RoutingIdentity, which will also start a new Core.
    /// The Core will attempt to achieve full routing node status.
    /// The intial Node object will have newly generated keys
    pub fn new(event_sender: Sender<Event>) -> Result<Node, RoutingError> {
        sodiumoxide::init();  // enable shared global (i.e. safe to multithread now)

        // start the handler for routing without a restriction to become a full node
        let (action_sender, raii_joiner) = try!(Core::new(event_sender, false, None));

        let (tx, rx) = channel();

        Ok(Node {
            interface_result_tx: tx,
            interface_result_rx: rx,
            action_sender: action_sender,
            _raii_joiner: raii_joiner,
        })
    }

    /// Send a Get message with a DataRequest to an Authority, signed with given keys.
    pub fn send_get_request(&self,
                            src: Authority,
                            dst: Authority,
                            data_request: DataRequest)
                            -> Result<(), InterfaceError> {
        let routing_msg = RoutingMessage::Request(RequestMessage {
            src: src,
            dst: dst,
            content: RequestContent::Get(data_request),
        });
        self.send_action(routing_msg)
    }

    /// Add something to the network
    pub fn send_put_request(&self, src: Authority, dst: Authority, data: Data) -> Result<(), InterfaceError> {
        let routing_msg = RoutingMessage::Request(RequestMessage {
            src: src,
            dst: dst,
            content: RequestContent::Put(data),
        });
        self.send_action(routing_msg)
    }

    /// Change something already on the network
    pub fn send_post_request(&self, src: Authority, dst: Authority, data: Data) -> Result<(), InterfaceError> {
        let routing_msg = RoutingMessage::Request(RequestMessage {
            src: src,
            dst: dst,
            content: RequestContent::Post(data),
        });
        self.send_action(routing_msg)
    }

    /// Remove something from the network
    pub fn send_delete_request(&self, src: Authority, dst: Authority, data: Data) -> Result<(), InterfaceError> {
        let routing_msg = RoutingMessage::Request(RequestMessage {
            src: src,
            dst: dst,
            content: RequestContent::Delete(data),
        });
        self.send_action(routing_msg)
    }

    /// Respond to a get_request indicating success
    pub fn send_get_success(&self, src: Authority, dst: Authority, data: Data) -> Result<(), InterfaceError> {
        let routing_msg = RoutingMessage::Response(ResponseMessage {
            src: src,
            dst: dst,
            content: ResponseContent::GetSuccess(data),
        });
        self.send_action(routing_msg)
    }

    /// Respond to a get_request indicating failure
    pub fn send_get_failure(&self,
                            src: Authority,
                            dst: Authority,
                            request: RequestMessage,
                            external_error_indicator: Vec<u8>)
                            -> Result<(), InterfaceError> {
        let routing_msg = RoutingMessage::Response(ResponseMessage {
            src: src,
            dst: dst,
            content: ResponseContent::GetFailure {
                request: request,
                external_error_indicator: external_error_indicator,
            },
        });
        self.send_action(routing_msg)
    }

    /// Respond to a put_request indicating success
    pub fn send_put_success(&self,
                            src: Authority,
                            dst: Authority,
                            request_hash: sha512::Digest)
                            -> Result<(), InterfaceError> {
        let routing_msg = RoutingMessage::Response(ResponseMessage {
            src: src,
            dst: dst,
            content: ResponseContent::PutSuccess(request_hash),
        });
        self.send_action(routing_msg)
    }

    /// Respond to a put_request indicating failure
    pub fn send_put_failure(&self,
                            src: Authority,
                            dst: Authority,
                            request: RequestMessage,
                            external_error_indicator: Vec<u8>)
                            -> Result<(), InterfaceError> {
        let routing_msg = RoutingMessage::Response(ResponseMessage {
            src: src,
            dst: dst,
            content: ResponseContent::PutFailure {
                request: request,
                external_error_indicator: external_error_indicator,
            },
        });
        self.send_action(routing_msg)
    }

    /// Respond to a post_request indicating success
    pub fn send_post_success(&self,
                             src: Authority,
                             dst: Authority,
                             request_hash: sha512::Digest)
                             -> Result<(), InterfaceError> {
        let routing_msg = RoutingMessage::Response(ResponseMessage {
            src: src,
            dst: dst,
            content: ResponseContent::PostSuccess(request_hash),
        });
        self.send_action(routing_msg)
    }

    /// Respond to a post_request indicating failure
    pub fn send_post_failure(&self,
                             src: Authority,
                             dst: Authority,
                             request: RequestMessage,
                             external_error_indicator: Vec<u8>)
                             -> Result<(), InterfaceError> {
        let routing_msg = RoutingMessage::Response(ResponseMessage {
            src: src,
            dst: dst,
            content: ResponseContent::PostFailure {
                request: request,
                external_error_indicator: external_error_indicator,
            },
        });
        self.send_action(routing_msg)
    }

    /// Respond to a delete_request indicating success
    pub fn send_delete_success(&self,
                               src: Authority,
                               dst: Authority,
                               request_hash: sha512::Digest)
                               -> Result<(), InterfaceError> {
        let routing_msg = RoutingMessage::Response(ResponseMessage {
            src: src,
            dst: dst,
            content: ResponseContent::DeleteSuccess(request_hash),
        });
        self.send_action(routing_msg)
    }

    /// Respond to a delete_request indicating failure
    pub fn send_delete_failure(&self,
                               src: Authority,
                               dst: Authority,
                               request: RequestMessage,
                               external_error_indicator: Vec<u8>)
                               -> Result<(), InterfaceError> {
        let routing_msg = RoutingMessage::Response(ResponseMessage {
            src: src,
            dst: dst,
            content: ResponseContent::DeleteFailure {
                request: request,
                external_error_indicator: external_error_indicator,
            },
        });
        self.send_action(routing_msg)
    }

    /// Refresh the content in the close group nodes of group address in `src`.
    /// This method needs to be called when churn is triggered.
    /// all the group members need to call this, otherwise it will not be resolved as a valid
    /// content. The authority provided (`src`) should be a group.
    pub fn send_refresh_request(&self,
                                src: Authority,
                                nonce: sha512::Digest,
                                content: Vec<u8>)
                                -> Result<(), InterfaceError> {
        let routing_msg = RoutingMessage::Request(RequestMessage {
            src: src.clone(),
            dst: src,
            content: RequestContent::Refresh {
                nonce: nonce,
                content: content,
            },
        });
        self.send_action(routing_msg)
    }

    /// Returns the name of this node.
    pub fn name(&self) -> Result<XorName, InterfaceError> {
        let (result_tx, result_rx) = channel();
        try!(self.action_sender.send(Action::Name { result_tx: result_tx }));
        Ok(try!(result_rx.recv()))
    }

    /// Returns the names of the close group to this node.
    pub fn close_group(&self) -> Result<Vec<XorName>, InterfaceError> {
        let (result_tx, result_rx) = channel();
        try!(self.action_sender.send(Action::CloseGroup { result_tx: result_tx }));
        Ok(try!(result_rx.recv()))
    }

    /// Returns the quorum size required to achieve consensus for group messages.
    pub fn dynamic_quorum_size(&self) -> Result<usize, InterfaceError> {
        let (result_tx, result_rx) = channel();
        try!(self.action_sender.send(Action::DynamicQuorumSize { result_tx: result_tx }));
        Ok(try!(result_rx.recv()))
    }

    // TODO(Spandan) Ask vaults if this can be removed as Node is now made to implement drop
    // trait and hence is RAII friendly
    /// Signal to Core that it needs to refuse new messages and handle all outstanding
    /// messages.  After handling all messages it will send an Event::Terminated to the user.
    pub fn stop(&self) {
        let _ = self.action_sender.send(Action::Terminate);
    }

    fn send_action(&self, routing_msg: RoutingMessage) -> Result<(), InterfaceError> {
        try!(self.action_sender.send(Action::NodeSendMessage {
            content: routing_msg,
            result_tx: self.interface_result_tx.clone(),
        }));

        try!(self.interface_result_rx.recv())
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        if let Err(err) = self.action_sender.send(Action::Terminate) {
            error!("Error {:?} sending event Core", err);
        }
    }
}

// #[cfg(test)]
// mod test {

// pub struct RoutingNetwork;

// impl RoutingNetwork {

//         fn new(size: u32) -> RoutingNetwork {
//             ::utils::initialise_logger(true);


//             let node = || { let _ =
//                 ::std::thread::spawn(move || ::test_utils::node::Node::new().run());
//             };
//             for i in 0..size { node(); ::std::thread::sleep_ms(1000 + i * 1000); }
//             ::std::thread::sleep_ms(size * 1000);

//             RoutingNetwork
//         }
//     }

//     fn calculate_key_name(key: &::std::string::String) -> XorName {
//         XorName::new(::sodiumoxide::crypto::hash::sha512::hash(key.as_bytes()).0)
//     }

//     #[test]
//     fn unit_client_put_get() {
//         // let _ = RoutingNetwork::new(10u32);
//         debug!("Starting client");
//         let mut client = ::test_utils::client::Client::new();
//         ::std::thread::sleep_ms(2000);

//         let key = ::std::string::String::from("key");
//         let value = ::std::string::String::from("value");
//         let name = calculate_key_name(&key.clone());
//         let data = unwrap_result!(::utils::encode(&(key, value)));
//         let data = ::data::Data::PlainData(::plain_data::PlainData::new(name.clone(), data));

//         debug!("Putting data {:?}", data);
//         client.put(data.clone());
//         ::std::thread::sleep_ms(5000);

//         let recovered_data = match client.get(::data::DataRequest::PlainData(name)) {
//             Some(data) => data,
//             None => panic!("Failed to recover stored data: {}.", name),
//         };

//         debug!("Recovered data {:?}", recovered_data);
//         assert_eq!(recovered_data, data);
//     }
// }
