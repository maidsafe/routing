// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.
// Copyright 2019 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use super::*;
use crate::{
    messages::DirectMessage,
    mock::Network,
    outbox::EventBox,
    state_machine::{State, StateMachine, Transition},
    utils::LogIdent,
    NetworkConfig, NetworkParams, NetworkService, ELDER_SIZE,
};
use std::{iter, net::SocketAddr};
use unwrap::unwrap;

// Accumulate even if 1 old node and an additional new node do not vote.
const NO_SINGLE_VETO_VOTE_COUNT: usize = 7;
const ACCUMULATE_VOTE_COUNT: usize = 6;
const NOT_ACCUMULATE_ALONE_VOTE_COUNT: usize = 5;

struct JoiningNodeInfo {
    full_id: FullId,
    addr: SocketAddr,
}

impl JoiningNodeInfo {
    fn with_addr(addr: &str) -> Self {
        Self {
            full_id: FullId::new(),
            addr: unwrap!(addr.parse()),
        }
    }

    fn public_id(&self) -> &PublicId {
        self.full_id.public_id()
    }

    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo::from(self.addr)
    }
}

struct ElderUnderTest {
    pub machine: StateMachine,
    pub full_id: FullId,
    pub other_full_ids: Vec<FullId>,
    pub other_parsec_map: Vec<ParsecMap>,
    pub elders_info: EldersInfo,
    pub candidate: P2pNode,
}

impl ElderUnderTest {
    fn new() -> Self {
        Self::with_section_size(NO_SINGLE_VETO_VOTE_COUNT)
    }

    fn with_section_size(sec_size: usize) -> Self {
        let socket_addr: SocketAddr = unwrap!("127.0.0.1:9999".parse());
        let connection_info = ConnectionInfo::from(socket_addr);
        let full_ids = (0..sec_size).map(|_| FullId::new()).collect_vec();

        let prefix = Prefix::<XorName>::default();
        let elders_info = unwrap!(EldersInfo::new(
            full_ids
                .iter()
                .map(|id| P2pNode::new(*id.public_id(), connection_info.clone()))
                .collect(),
            prefix,
            iter::empty()
        ));
        let first_ages = full_ids
            .iter()
            .map(|id| (*id.public_id(), MIN_AGE_COUNTER))
            .collect();

        let gen_pfx_info = GenesisPfxInfo {
            first_info: elders_info.clone(),
            first_state_serialized: Vec::new(),
            first_ages,
            latest_info: EldersInfo::default(),
        };

        let full_id = full_ids[0].clone();
        let machine = make_state_machine(&full_id, &gen_pfx_info, &mut ());

        let other_full_ids = full_ids[1..].iter().cloned().collect_vec();
        let other_parsec_map = other_full_ids
            .iter()
            .map(|full_id| ParsecMap::new(full_id.clone(), &gen_pfx_info))
            .collect_vec();

        let candidate_addr: SocketAddr = unwrap!("127.0.0.2:9999".parse());
        let candidate = P2pNode::new(
            *FullId::new().public_id(),
            ConnectionInfo::from(candidate_addr),
        );

        let mut elder_test = Self {
            machine,
            full_id,
            other_full_ids,
            other_parsec_map,
            elders_info,
            candidate,
        };

        // Process initial unpolled event
        unwrap!(elder_test.create_gossip());
        elder_test
    }

    fn elder_state(&self) -> &Elder {
        unwrap!(self.machine.current().elder_state())
    }

    fn n_vote_for(&mut self, count: usize, events: impl IntoIterator<Item = AccumulatingEvent>) {
        for event in events {
            self.other_parsec_map
                .iter_mut()
                .zip(self.other_full_ids.iter())
                .take(count)
                .for_each(|(parsec, full_id)| {
                    let sig_event = event
                        .elders_info()
                        .map(|info| unwrap!(SectionInfoSigPayload::new(info, &full_id)));
                    parsec.vote_for(
                        event.clone().into_network_event_with(sig_event),
                        &LogIdent::new(&0),
                    )
                });
        }
    }

    fn create_gossip(&mut self) -> Result<(), RoutingError> {
        let other_pub_id = *self.other_full_ids[0].public_id();
        let addr: SocketAddr = unwrap!("127.0.0.3:9999".parse());
        let connection_info = ConnectionInfo::from(addr);
        let message = unwrap!(self.other_parsec_map[0].create_gossip(0, self.full_id.public_id()));
        self.handle_direct_message((message, P2pNode::new(other_pub_id, connection_info)))
    }

    fn n_vote_for_gossipped(
        &mut self,
        count: usize,
        events: impl IntoIterator<Item = AccumulatingEvent>,
    ) -> Result<(), RoutingError> {
        self.n_vote_for(count, events);
        self.create_gossip()
    }

    fn accumulate_online(&mut self, p2p_node: P2pNode) {
        let _ = self.n_vote_for_gossipped(
            ACCUMULATE_VOTE_COUNT,
            iter::once(AccumulatingEvent::Online(OnlinePayload {
                p2p_node,
                age: MIN_AGE,
            })),
        );
    }

    fn accumulate_add_elder_if_vote(&mut self, public_id: PublicId) {
        let _ = self.n_vote_for_gossipped(
            NOT_ACCUMULATE_ALONE_VOTE_COUNT,
            iter::once(AccumulatingEvent::AddElder(public_id)),
        );
    }

    fn accumulate_section_info_if_vote(&mut self, section_info_payload: EldersInfo) {
        let _ = self.n_vote_for_gossipped(
            NOT_ACCUMULATE_ALONE_VOTE_COUNT,
            iter::once(AccumulatingEvent::SectionInfo(section_info_payload)),
        );
    }

    fn accumulate_offline(&mut self, offline_payload: PublicId) {
        let _ = self.n_vote_for_gossipped(
            ACCUMULATE_VOTE_COUNT,
            iter::once(AccumulatingEvent::Offline(offline_payload)),
        );
    }

    fn accumulate_remove_elder_if_vote(&mut self, offline_payload: PublicId) {
        let _ = self.n_vote_for_gossipped(
            NOT_ACCUMULATE_ALONE_VOTE_COUNT,
            iter::once(AccumulatingEvent::RemoveElder(offline_payload)),
        );
    }

    fn new_elders_info_with_candidate(&self) -> EldersInfo {
        unwrap!(EldersInfo::new(
            self.elders_info
                .p2p_members()
                .iter()
                .chain(iter::once(&self.candidate))
                .cloned()
                .collect(),
            *self.elders_info.prefix(),
            Some(&self.elders_info)
        ))
    }

    fn new_elders_info_without_candidate(&self) -> EldersInfo {
        let old_info = self.new_elders_info_with_candidate();
        unwrap!(EldersInfo::new(
            self.elders_info.p2p_members().clone(),
            *old_info.prefix(),
            Some(&old_info)
        ))
    }

    fn has_unpolled_observations(&self) -> bool {
        self.elder_state().has_unpolled_observations()
    }

    fn is_candidate_member(&self) -> bool {
        self.elder_state()
            .chain()
            .is_peer_our_member(self.candidate.public_id())
    }

    fn is_candidate_elder(&self) -> bool {
        self.elder_state()
            .chain()
            .is_peer_our_elder(self.candidate.public_id())
    }

    fn is_candidate_in_our_elders_info(&self) -> bool {
        self.elder_state()
            .chain()
            .our_info()
            .members()
            .contains(self.candidate.public_id())
    }

    fn handle_direct_message(&mut self, msg: (DirectMessage, P2pNode)) -> Result<(), RoutingError> {
        let _ = self
            .machine
            .elder_state_mut()
            .handle_direct_message(msg.0, msg.1, &mut ())?;
        Ok(())
    }

    fn handle_connected_to(&mut self, conn_info: ConnectionInfo) {
        match self
            .machine
            .elder_state_mut()
            .handle_connected_to(conn_info, &mut ())
        {
            Transition::Stay => (),
            _ => panic!("Unexpected transition"),
        }
    }

    fn handle_bootstrap_request(&mut self, pub_id: PublicId, conn_info: ConnectionInfo) {
        self.handle_connected_to(conn_info.clone());
        unwrap!(self
            .machine
            .elder_state_mut()
            .handle_bootstrap_request(P2pNode::new(pub_id, conn_info), *pub_id.name()));
    }

    fn is_connected(&self, pub_id: &PublicId) -> bool {
        // WIP: potentially slow due to `XorName` lookup
        self.machine
            .current()
            .chain()
            .map(|chain| chain.get_p2p_node(pub_id.name()).is_some())
            .unwrap_or(false)
    }
}

fn new_elder_state(
    full_id: &FullId,
    gen_pfx_info: &GenesisPfxInfo,
    network_service: NetworkService,
    timer: Timer,
    outbox: &mut dyn EventBox,
) -> State {
    let public_id = *full_id.public_id();

    let parsec_map = ParsecMap::new(full_id.clone(), gen_pfx_info);
    let chain = Chain::new(Default::default(), public_id, gen_pfx_info.clone());
    let client_map = ClientMap::new();

    let details = ElderDetails {
        chain,
        network_service,
        event_backlog: Default::default(),
        full_id: full_id.clone(),
        gen_pfx_info: gen_pfx_info.clone(),
        msg_queue: Default::default(),
        parsec_map,
        client_map,
        routing_msg_filter: RoutingMessageFilter::new(),
        timer,
    };

    let section_info = gen_pfx_info.first_info.clone();
    let prefix = *section_info.prefix();
    Elder::from_adult(details, section_info, prefix, outbox)
        .map(State::Elder)
        .unwrap_or(State::Terminated)
}

fn make_state_machine(
    full_id: &FullId,
    gen_pfx_info: &GenesisPfxInfo,
    outbox: &mut dyn EventBox,
) -> StateMachine {
    let network = Network::new(
        NetworkParams {
            elder_size: ELDER_SIZE,
            safe_section_size: ELDER_SIZE,
        },
        None,
    );

    let endpoint = network.gen_addr();
    let config = NetworkConfig::node().with_hard_coded_contact(endpoint);

    StateMachine::new(
        move |network_service, timer, outbox2| {
            new_elder_state(full_id, gen_pfx_info, network_service, timer, outbox2)
        },
        config,
        outbox,
    )
    .1
}

trait StateMachineExt {
    fn elder_state_mut(&mut self) -> &mut Elder;
}

impl StateMachineExt for StateMachine {
    fn elder_state_mut(&mut self) -> &mut Elder {
        unwrap!(self.current_mut().elder_state_mut())
    }
}

#[test]
fn construct() {
    let elder_test = ElderUnderTest::new();

    assert!(!elder_test.has_unpolled_observations());
    assert!(!elder_test.is_candidate_elder());
}

#[test]
fn when_accumulate_online_then_node_is_added_to_our_members() {
    let mut elder_test = ElderUnderTest::new();
    elder_test.accumulate_online(elder_test.candidate.clone());

    assert!(elder_test.has_unpolled_observations()); // voted for AddElder
    assert!(elder_test.is_candidate_member());
    assert!(!elder_test.is_candidate_elder());
    assert!(!elder_test.is_candidate_in_our_elders_info());
}

#[test]
fn when_accumulate_online_and_accumulate_add_elder_then_node_is_promoted_to_elder() {
    let mut elder_test = ElderUnderTest::new();
    elder_test.accumulate_online(elder_test.candidate.clone());
    elder_test.accumulate_add_elder_if_vote(*elder_test.candidate.public_id());

    assert!(!elder_test.has_unpolled_observations());
    assert!(elder_test.is_candidate_member());
    assert!(elder_test.is_candidate_elder());
    assert!(!elder_test.is_candidate_in_our_elders_info());
}

#[test]
fn when_accumulate_online_and_accumulate_add_elder_and_accumulate_section_info_then_node_is_added_to_our_elders_info(
) {
    let mut elder_test = ElderUnderTest::new();
    elder_test.accumulate_online(elder_test.candidate.clone());
    elder_test.accumulate_add_elder_if_vote(*elder_test.candidate.public_id());

    let new_elders_info = elder_test.new_elders_info_with_candidate();
    elder_test.accumulate_section_info_if_vote(new_elders_info);

    assert!(!elder_test.has_unpolled_observations());
    assert!(elder_test.is_candidate_member());
    assert!(elder_test.is_candidate_elder());
    assert!(elder_test.is_candidate_in_our_elders_info());
}

#[test]
fn when_accumulate_offline_then_node_is_removed_from_our_members() {
    let mut elder_test = ElderUnderTest::new();
    elder_test.accumulate_online(elder_test.candidate.clone());
    elder_test.accumulate_add_elder_if_vote(*elder_test.candidate.public_id());
    elder_test.accumulate_section_info_if_vote(elder_test.new_elders_info_with_candidate());

    elder_test.accumulate_offline(*elder_test.candidate.public_id());

    assert!(elder_test.has_unpolled_observations()); // voted for RemoveElder
    assert!(!elder_test.is_candidate_member());
    assert!(elder_test.is_candidate_elder());
    assert!(elder_test.is_candidate_in_our_elders_info());
}

// Note: currently a node is considered demoted from elder only when the new section info
// accumulates. This logic might be seen as inconsistent with the node promotion logic so we might
// consider changing it.
#[test]
fn when_accumulate_offline_and_accumulate_remove_elder_then_node_is_not_yet_demoted_from_elder() {
    let mut elder_test = ElderUnderTest::new();
    elder_test.accumulate_online(elder_test.candidate.clone());
    elder_test.accumulate_add_elder_if_vote(*elder_test.candidate.public_id());
    elder_test.accumulate_section_info_if_vote(elder_test.new_elders_info_with_candidate());

    elder_test.accumulate_offline(*elder_test.candidate.public_id());
    elder_test.accumulate_remove_elder_if_vote(*elder_test.candidate.public_id());

    assert!(!elder_test.has_unpolled_observations());
    assert!(!elder_test.is_candidate_member());
    assert!(elder_test.is_candidate_elder());
    assert!(elder_test.is_candidate_in_our_elders_info());
}

#[test]
fn when_accumulate_offline_and_accumulate_remove_elder_and_accumulate_section_info_then_node_is_removed_from_our_elders_info(
) {
    let mut elder_test = ElderUnderTest::new();
    elder_test.accumulate_online(elder_test.candidate.clone());
    elder_test.accumulate_add_elder_if_vote(*elder_test.candidate.public_id());
    elder_test.accumulate_section_info_if_vote(elder_test.new_elders_info_with_candidate());

    elder_test.accumulate_offline(*elder_test.candidate.public_id());
    elder_test.accumulate_remove_elder_if_vote(*elder_test.candidate.public_id());
    elder_test.accumulate_section_info_if_vote(elder_test.new_elders_info_without_candidate());

    assert!(!elder_test.has_unpolled_observations());
    assert!(!elder_test.is_candidate_member());
    assert!(!elder_test.is_candidate_elder());
    assert!(!elder_test.is_candidate_in_our_elders_info());
}

#[test]
#[ignore]
fn accept_previously_rejected_node_after_reaching_elder_size() {
    // Set section size to one less than the desired number of the elders in a section. This makes
    // us reject any bootstrapping nodes.
    let mut elder_test = ElderUnderTest::with_section_size(ELDER_SIZE - 1);
    let node = JoiningNodeInfo::with_addr("198.51.100.0:5000");

    // Bootstrap fails for insufficient section size.
    elder_test.handle_bootstrap_request(*node.public_id(), node.connection_info());
    assert!(!elder_test.is_connected(node.public_id()));

    // Add new section member to reach elder_size.
    elder_test.accumulate_online(elder_test.candidate.clone());
    elder_test.accumulate_add_elder_if_vote(*elder_test.candidate.public_id());
    elder_test.accumulate_section_info_if_vote(elder_test.new_elders_info_with_candidate());

    // Re-bootstrap now succeeds.
    elder_test.handle_bootstrap_request(*node.public_id(), node.connection_info());
    assert!(elder_test.is_connected(node.public_id()));
}
