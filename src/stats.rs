// Copyright 2016 MaidSafe.net limited.
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

use messages::{DirectMessage, MessageContent, RoutingMessage, Request, Response};

/// The number of messages after which the message statistics should be printed.
const MSG_LOG_COUNT: usize = 500;

/// A collection of counters to gather Routing statistics.
#[derive(Default)]
pub struct Stats {
    // TODO: Make these private and move the logic here.
    pub cur_routing_table_size: usize,
    pub cur_client_num: usize,
    pub cumulative_client_num: usize,
    pub tunnel_client_pairs: usize,
    pub tunnel_connections: usize,

    msg_direct_node_identify: usize,
    msg_direct_new_node: usize,
    msg_direct_connection_unneeded: usize,

    msg_get: usize,
    msg_put: usize,
    msg_post: usize,
    msg_delete: usize,
    msg_get_account_info: usize,
    msg_get_close_group: usize,
    msg_get_node_name: usize,
    msg_expect_close_node: usize,
    msg_refresh: usize,
    msg_connection_info: usize,
    msg_get_success: usize,
    msg_get_failure: usize,
    msg_put_success: usize,
    msg_put_failure: usize,
    msg_post_success: usize,
    msg_post_failure: usize,
    msg_delete_success: usize,
    msg_delete_failure: usize,
    msg_get_account_info_success: usize,
    msg_get_account_info_failure: usize,
    msg_get_close_group_rsp: usize,
    msg_get_node_name_rsp: usize,
    msg_ack: usize,

    msg_other: usize,

    msg_total: usize,
}

impl Stats {
    /// Increments the counter for the given routing message type.
    pub fn count_routing_message(&mut self, msg: &RoutingMessage) {
        match msg.content {
            MessageContent::GetNodeName { .. } => self.msg_get_node_name += 1,
            MessageContent::ExpectCloseNode { .. } => self.msg_expect_close_node += 1,
            MessageContent::GetCloseGroup(..) => self.msg_get_close_group += 1,
            MessageContent::ConnectionInfo { .. } => self.msg_connection_info += 1,
            MessageContent::Request(Request::Refresh(..)) => self.msg_refresh += 1,
            MessageContent::Request(Request::Get(..)) => self.msg_get += 1,
            MessageContent::Request(Request::Put(..)) => self.msg_put += 1,
            MessageContent::Request(Request::Post(..)) => self.msg_post += 1,
            MessageContent::Request(Request::Delete(..)) => self.msg_delete += 1,
            MessageContent::Request(Request::GetAccountInfo(..)) => self.msg_get_account_info += 1,
            MessageContent::Response(Response::GetSuccess(..)) => self.msg_get_success += 1,
            MessageContent::Response(Response::GetFailure { .. }) => self.msg_get_failure += 1,
            MessageContent::Response(Response::PutSuccess(..)) => self.msg_put_success += 1,
            MessageContent::Response(Response::PutFailure { .. }) => self.msg_put_failure += 1,
            MessageContent::Response(Response::PostSuccess(..)) => self.msg_post_success += 1,
            MessageContent::Response(Response::PostFailure { .. }) => self.msg_post_failure += 1,
            MessageContent::Response(Response::DeleteSuccess(..)) => self.msg_delete_success += 1,
            MessageContent::Response(Response::DeleteFailure { .. }) => {
                self.msg_delete_failure += 1
            }
            MessageContent::Response(Response::GetAccountInfoSuccess { .. }) => {
                self.msg_get_account_info_success += 1
            }
            MessageContent::Response(Response::GetAccountInfoFailure { .. }) => {
                self.msg_get_account_info_failure += 1
            }
            MessageContent::GetCloseGroupResponse { .. } => self.msg_get_close_group_rsp += 1,
            MessageContent::GetNodeNameResponse { .. } => self.msg_get_node_name_rsp += 1,
            MessageContent::Ack(..) => self.msg_ack += 1,
        }
        self.increment_msg_total();
    }

    /// Increments the counter for the given direct message type.
    pub fn count_direct_message(&mut self, msg: &DirectMessage) {
        match *msg {
            DirectMessage::NodeIdentify { .. } => self.msg_direct_node_identify += 1,
            DirectMessage::NewNode(_) => self.msg_direct_new_node += 1,
            DirectMessage::ConnectionUnneeded(..) => self.msg_direct_connection_unneeded += 1,
            _ => self.msg_other += 1,
        }
        self.increment_msg_total();
    }

    /// Increment the total message count, and if divisible by 100, log a message with the counts.
    fn increment_msg_total(&mut self) {
        self.msg_total += 1;
        if self.msg_total % MSG_LOG_COUNT == 0 {
            info!("Stats - Sent {} messages in total, {} uncategorised",
                  self.msg_total,
                  self.msg_other);
            info!("Stats - Direct - NodeIdentify: {}, NewNode: {}, ConnectionUnneeded: {}",
                  self.msg_direct_node_identify,
                  self.msg_direct_new_node,
                  self.msg_direct_connection_unneeded);
            info!("Stats - Hops - Get: {}, Put: {}, Post: {}, Delete: {}, GetAccountInfo: {}, \
                   GetNodeName: {}, ExpectCloseNode: {}, GetCloseGroup: {}, Refresh: {}, \
                   ConnectionInfo: {}, GetSuccess: {}, GetFailure: {}, PutSuccess: {}, \
                   PutFailure: {}, PostSuccess: {}, PostFailure: {}, DeleteSuccess: {}, \
                   DeleteFailure: {}, GetAccountInfoSuccess: {}, GetAccountInfoFailure: {}, \
                   GetCloseGroupResponse: {}, GetNodeNameResponse: {}, Ack: {}",
                  self.msg_get,
                  self.msg_put,
                  self.msg_post,
                  self.msg_delete,
                  self.msg_get_account_info,
                  self.msg_get_node_name,
                  self.msg_expect_close_node,
                  self.msg_get_close_group,
                  self.msg_refresh,
                  self.msg_connection_info,
                  self.msg_get_success,
                  self.msg_get_failure,
                  self.msg_put_success,
                  self.msg_put_failure,
                  self.msg_post_success,
                  self.msg_post_failure,
                  self.msg_delete_success,
                  self.msg_delete_failure,
                  self.msg_get_account_info_success,
                  self.msg_get_account_info_failure,
                  self.msg_get_close_group_rsp,
                  self.msg_get_node_name_rsp,
                  self.msg_ack);
        }
    }
}
