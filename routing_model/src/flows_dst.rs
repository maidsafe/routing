// Copyright 2019 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

// This is used two ways: inline tests, and integration tests (with mock).
// There's no point configuring each item which is only used in one of these.

use crate::state::{AcceptAsCandidateState, CheckAndProcessElderChangeState, State};
use crate::utilities::{
    Candidate, ChangeElder, Event, LocalEvent, ParsecVote, Proof, Rpc, Section,
};

#[derive(Debug, PartialEq, Default, Clone)]
pub struct TopLevelDst(pub State);

impl TopLevelDst {
    pub fn try_next(&self, event: Event) -> Option<State> {
        match event {
            Event::Rpc(rpc) => self.try_rpc(rpc),
            Event::ParsecConsensus(vote) => self.try_consensus(vote),
            Event::LocalEvent(LocalEvent::TimeoutAccept) => {
                return Some(self.0.failure_event(event));
            }
            _ => None,
        }
        .map(|state| state.0)
    }

    fn try_rpc(&self, rpc: Rpc) -> Option<Self> {
        match rpc {
            Rpc::ExpectCandidate(candidate) => Some(self.vote_parsec_expect_candidate(candidate)),
            Rpc::ResourceProofResponse { .. } | Rpc::CandidateInfo { .. } => Some(self.discard()),
            _ => None,
        }
    }

    fn try_consensus(&self, vote: ParsecVote) -> Option<Self> {
        match vote {
            ParsecVote::ExpectCandidate(candidate) => {
                self.try_consensused_expect_candidate(candidate)
            }
            ParsecVote::Online(_) | ParsecVote::PurgeCandidate(_) => Some(self.discard()),

            // Delegate to other event loops
            _ => None,
        }
    }

    fn try_consensused_expect_candidate(&self, candidate: Candidate) -> Option<Self> {
        match (
            self.0.dst_routine.is_processing_candidate,
            self.0.action.check_shortest_prefix(),
        ) {
            (_, Some(section)) => Some(self.resend_expect_candidate_rpc(candidate, section)),
            (true, None) => Some(self.send_refuse_candidate_rpc(candidate)),
            (false, None) => Some(self.concurrent_transition_to_accept_as_candidate(candidate)),
        }
    }

    fn concurrent_transition_to_accept_as_candidate(&self, candidate: Candidate) -> Self {
        self.set_is_processing_candidate(true)
            .0
            .as_accept_as_candidate()
            .start_event_loop(candidate)
            .0
            .as_top_level_dst()
    }

    fn transition_exit_accept_as_candidate(&self) -> Self {
        self.set_is_processing_candidate(false)
    }

    fn set_is_processing_candidate(&self, value: bool) -> Self {
        let mut state = self.clone();
        state.0.dst_routine.is_processing_candidate = value;
        state
    }

    fn discard(&self) -> Self {
        self.clone()
    }

    fn vote_parsec_expect_candidate(&self, candidate: Candidate) -> Self {
        self.0
            .action
            .vote_parsec(ParsecVote::ExpectCandidate(candidate));
        self.clone()
    }

    fn send_refuse_candidate_rpc(&self, candidate: Candidate) -> Self {
        self.0.action.send_rpc(Rpc::RefuseCandidate(candidate));
        self.clone()
    }

    fn resend_expect_candidate_rpc(&self, candidate: Candidate, section: Section) -> Self {
        self.0
            .action
            .send_rpc(Rpc::ResendExpectCandidate(section, candidate));
        self.clone()
    }
}

#[derive(Debug, PartialEq, Default, Clone)]
pub struct AcceptAsCandidate(pub State);

// AcceptAsCandidate Sub Routine
impl AcceptAsCandidate {
    fn start_event_loop(&self, candidate: Candidate) -> Self {
        self.0
            .with_dst_sub_routine_accept_as_candidate(Some(AcceptAsCandidateState::new(candidate)))
            .as_accept_as_candidate()
            .add_node_ressource_proofing()
            .send_relocate_response_rpc()
    }

    fn exit_event_loop(&self) -> Self {
        self.0
            .with_dst_sub_routine_accept_as_candidate(None)
            .as_top_level_dst()
            .transition_exit_accept_as_candidate()
            .0
            .as_accept_as_candidate()
    }

    pub fn try_next(&self, event: Event) -> Option<State> {
        match event {
            Event::Rpc(Rpc::CandidateInfo {
                candidate, valid, ..
            }) => self.try_rpc_info(candidate, valid),
            Event::Rpc(Rpc::ResourceProofResponse {
                candidate, proof, ..
            }) => self.try_rpc_proof(candidate, proof),
            Event::ParsecConsensus(vote) => self.try_consensus(vote),
            Event::LocalEvent(LocalEvent::TimeoutAccept) => {
                Some(self.vote_parsec_purge_candidate())
            }
            // Delegate to other event loops
            _ => None,
        }
        .map(|state| state.0)
    }

    fn try_rpc_info(&self, candidate: Candidate, valid: bool) -> Option<Self> {
        if candidate != self.candidate() || self.routine_state().got_candidate_info {
            return None;
        }

        Some(match valid {
            true => self.set_got_candidate_info(true).send_resource_proof_rpc(),
            false => self.vote_parsec_purge_candidate(),
        })
    }

    fn try_rpc_proof(&self, candidate: Candidate, proof: Proof) -> Option<Self> {
        if candidate != self.candidate() || self.routine_state().voted_online || !proof.is_valid() {
            return None;
        }

        Some(match proof {
            Proof::ValidPart => self.send_resource_proof_receipt_rpc(),
            Proof::ValidEnd => self.set_voted_online(true).vote_parsec_online_candidate(),
            Proof::Invalid => panic!("Only valid proof"),
        })
    }

    fn try_consensus(&self, vote: ParsecVote) -> Option<Self> {
        if vote.candidate() != Some(self.candidate()) {
            return None;
        }

        match vote {
            ParsecVote::Online(_) => Some(self.make_node_online()),
            ParsecVote::PurgeCandidate(_) => Some(self.remove_node()),

            // Delegate to other event loops
            _ => None,
        }
    }

    fn routine_state(&self) -> &AcceptAsCandidateState {
        match &self.0.dst_routine.sub_routine_accept_as_candidate {
            Some(state) => state,
            _ => panic!("Expect AcceptAsCandidate {:?}", &self),
        }
    }

    fn mut_routine_state(&mut self) -> &mut AcceptAsCandidateState {
        let clone = self.clone();
        match &mut self.0.dst_routine.sub_routine_accept_as_candidate {
            Some(state) => state,
            _ => panic!("Expect AcceptAsCandidate {:?}", &clone),
        }
    }

    fn set_got_candidate_info(&self, value: bool) -> Self {
        let mut state = self.clone();
        state.mut_routine_state().got_candidate_info = value;
        state
    }

    fn set_voted_online(&self, value: bool) -> Self {
        let mut state = self.clone();
        state.mut_routine_state().voted_online = value;
        state
    }

    fn vote_parsec_purge_candidate(&self) -> Self {
        self.0
            .action
            .vote_parsec(ParsecVote::PurgeCandidate(self.candidate()));
        self.clone()
    }

    fn vote_parsec_online_candidate(&self) -> Self {
        self.0
            .action
            .vote_parsec(ParsecVote::Online(self.candidate()));
        self.clone()
    }

    fn add_node_ressource_proofing(&self) -> Self {
        self.0.action.add_node_ressource_proofing(self.candidate());
        self.clone()
    }

    fn make_node_online(&self) -> Self {
        self.0.action.set_candidate_online_state(self.candidate());
        self.0.action.send_node_approval_rpc(self.candidate());
        self.exit_event_loop()
    }

    fn remove_node(&self) -> Self {
        self.0.action.remove_node(self.candidate());
        self.exit_event_loop()
    }

    fn send_relocate_response_rpc(&self) -> Self {
        self.0.action.send_relocate_response_rpc(self.candidate());
        self.clone()
    }

    fn send_resource_proof_rpc(&self) -> Self {
        self.0.action.send_candidate_proof_request(self.candidate());
        self.clone()
    }

    fn send_resource_proof_receipt_rpc(&self) -> Self {
        self.0.action.send_candidate_proof_receipt(self.candidate());
        self.clone()
    }

    fn candidate(&self) -> Candidate {
        self.routine_state().candidate
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct CheckAndProcessElderChange(pub State);

// CheckAndProcessElderChange Sub Routine
impl CheckAndProcessElderChange {
    fn start_event_loop(&self) -> Self {
        self.start_check_elder_timeout()
    }

    pub fn try_next(&self, event: Event) -> Option<State> {
        match event {
            Event::ParsecConsensus(vote) => self.try_consensus(&vote),
            Event::LocalEvent(LocalEvent::TimeoutCheckElder) => {
                Some(self.vote_parsec_check_elder())
            }
            _ => None,
        }
        .map(|state| state.0)
    }

    fn try_consensus(&self, vote: &ParsecVote) -> Option<Self> {
        if ParsecVote::CheckElder == *vote {
            return Some(self.check_elder());
        }

        if !self.routine_state().wait_votes.contains(&vote) {
            return None;
        }

        let mut state = self.clone();
        let wait_votes = &mut state.mut_routine_state().wait_votes;
        wait_votes.retain(|wait_vote| wait_vote != vote);

        if wait_votes.is_empty() {
            Some(state.mark_elder_change().start_check_elder_timeout())
        } else {
            Some(state)
        }
    }

    fn routine_state(&self) -> &CheckAndProcessElderChangeState {
        &self.0.check_and_process_elder_change_routine
    }

    fn mut_routine_state(&mut self) -> &mut CheckAndProcessElderChangeState {
        &mut self.0.check_and_process_elder_change_routine
    }

    fn check_elder(&self) -> Self {
        match self.0.action.check_elder() {
            Some(change_elder) => self.start_vote_elder_change(change_elder),
            None => self.start_check_elder_timeout(),
        }
    }

    fn start_vote_elder_change(&self, change_elder: ChangeElder) -> Self {
        let mut state = self.clone();

        let votes = state.0.action.get_elder_change_votes(&change_elder);
        state.mut_routine_state().change_elder = Some(change_elder);
        state.mut_routine_state().wait_votes = votes;

        for vote in &state.routine_state().wait_votes {
            state.0.action.vote_parsec(*vote);
        }

        state
    }

    fn mark_elder_change(&self) -> Self {
        let mut state = self.clone();

        let change_elder = state.mut_routine_state().change_elder.take().unwrap();
        state.0.action.mark_elder_change(change_elder);

        state
    }

    fn vote_parsec_check_elder(&self) -> Self {
        self.0.action.vote_parsec(ParsecVote::CheckElder);
        self.clone()
    }

    fn start_check_elder_timeout(&self) -> Self {
        self.0.action.schedule_event(LocalEvent::TimeoutCheckElder);
        self.clone()
    }
}