// Copyright 2015 MaidSafe.net limited.
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

use authority::Authority;
use messages::{Request, Response};
use routing_table::{Prefix, RoutingTable};
use std::fmt::{self, Debug, Formatter};
use xor_name::XorName;

/// An Event raised by a `Node` or `Client` via its event sender.
///
/// These are sent by routing to the library's user. It allows the user to handle requests and
/// responses, and to react to changes in the network.
///
/// `Request` and `Response` events from group authorities are only raised once the quorum has been
/// reached, i. e. enough members of the group have sent the same message.
#[derive(Clone, Eq, PartialEq)]
pub enum Event {
    /// Received a request message.
    Request {
        /// The request message.
        request: Request,
        /// The source authority that sent the request.
        src: Authority,
        /// The destination authority that receives the request.
        dst: Authority,
    },
    /// Received a response message.
    Response {
        /// The response message.
        response: Response,
        /// The source authority that sent the response.
        src: Authority,
        /// The destination authority that receives the response.
        dst: Authority,
    },
    /// A new node joined the network and may be a member of group authorities we also belong to.
    NodeAdded(XorName, RoutingTable<XorName>),
    /// A node left the network and may have been a member of group authorities we also belong to.
    NodeLost(XorName, RoutingTable<XorName>),
    /// Our own group has been split, resulting in the included `Prefix` for our new group.
    GroupSplit(Prefix<XorName>),
    /// Our own group requires merged with others, resulting in the included `Prefix` for our new
    /// group.
    GroupMerge(Prefix<XorName>),
    /// The client has successfully connected to a proxy node on the network.
    Connected,
    /// Disconnected or failed to connect - restart required.
    RestartRequired,
    /// Startup failed - terminate.
    Terminate,
    // TODO: Find a better solution for periodic tasks.
    /// This event is sent periodically every time Routing sends the `Heartbeat` messages.
    Tick,
}

impl Debug for Event {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        match *self {
            Event::Request { ref request, ref src, ref dst } => {
                write!(formatter,
                       "Event::Request {{ request: {:?}, src: {:?}, dst: {:?} }}",
                       request,
                       src,
                       dst)
            }
            Event::Response { ref response, ref src, ref dst } => {
                write!(formatter,
                       "Event::Response {{ response: {:?}, src: {:?}, dst: {:?} }}",
                       response,
                       src,
                       dst)
            }
            Event::NodeAdded(ref node_name, _) => {
                write!(formatter,
                       "Event::NodeAdded({:?}, routing_table)",
                       node_name)
            }
            Event::NodeLost(ref node_name, _) => {
                write!(formatter, "Event::NodeLost({:?}, routing_table)", node_name)
            }
            Event::GroupSplit(ref prefix) => write!(formatter, "Event::GroupSplit({:?})", prefix),
            Event::GroupMerge(ref prefix) => write!(formatter, "Event::GroupMerge({:?})", prefix),
            Event::Connected => write!(formatter, "Event::Connected"),
            Event::RestartRequired => write!(formatter, "Event::RestartRequired"),
            Event::Terminate => write!(formatter, "Event::Terminate"),
            Event::Tick => write!(formatter, "Event::Tick"),
        }
    }
}
