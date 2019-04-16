// Copyright 2019 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

// This is used two ways: inline tests, and integration tests (with mock).
// There's no point configuring each item which is only used in one of these.

#![allow(dead_code)]
#![allow(unused_imports)]

mod actions;
mod utilities;

use itertools::Itertools;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::{self, Debug, Display, Formatter};
use std::rc::Rc;

use crate::actions::{Action, InnerAction};
use crate::utilities::{
    Age, Attributes, Candidate, ChangeElder, Event, GenesisPfxInfo, LocalEvent, Name, Node,
    NodeChange, NodeState, ParsecVote, Proof, ProofRequest, ProofSource, Rpc, Section, SectionInfo,
};

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate unwrap;

//////////////////
/// Dst
//////////////////

#[derive(Debug, PartialEq, Default, Clone)]
struct TopLevelDst(State);

impl TopLevelDst {
    fn try_next(&self, event: Event) -> Option<State> {
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
struct AcceptAsCandidate(State);

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

    fn try_next(&self, event: Event) -> Option<State> {
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
struct CheckAndProcessElderChange(State);

// CheckAndProcessElderChange Sub Routine
impl CheckAndProcessElderChange {
    fn start_event_loop(&self) -> Self {
        self.start_check_elder_timeout()
    }

    fn try_next(&self, event: Event) -> Option<State> {
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

//////////////////
/// Scr
//////////////////

#[derive(Debug, PartialEq, Default, Clone)]
struct TopLevelSrc(State);

impl TopLevelSrc {
    fn try_next(&self, event: Event) -> Option<State> {
        match event {
            Event::Rpc(rpc) => self.try_rpc(rpc),
            Event::ParsecConsensus(vote) => self.try_consensus(vote),
            Event::LocalEvent(local_event) => self.try_local_event(local_event),
        }
        .map(|state| state.0)
    }

    fn try_rpc(&self, rpc: Rpc) -> Option<Self> {
        match rpc {
            Rpc::RefuseCandidate(candidate) => Some(self.vote_parsec_refuse_candidate(candidate)),
            Rpc::RelocateResponse(candidate, section) => {
                Some(self.vote_parsec_relocation_response(candidate, section))
            }
            _ => None,
        }
    }

    fn try_consensus(&self, vote: ParsecVote) -> Option<Self> {
        match vote {
            ParsecVote::RelocationTrigger => self.try_consensused_relocation_trigger(),

            // Delegate to other event loops
            _ => None,
        }
    }

    fn try_local_event(&self, local_event: LocalEvent) -> Option<Self> {
        match local_event {
            LocalEvent::RelocationTrigger => Some(self.vote_parsec_relocation_trigger()),
            _ => None,
        }
    }

    fn try_consensused_relocation_trigger(&self) -> Option<Self> {
        match self.0.src_routine.relocating_candidate {
            Some(_) => Some(self.discard()),
            None => Some(
                self.set_relocating_candidate(Some(self.0.action.get_relocating_candidate()))
                    .set_candidate_relocating_state_if_needed()
                    .check_if_relocating_elder(),
            ),
        }
    }

    fn check_if_relocating_elder(&self) -> Self {
        if self
            .0
            .action
            .is_elder_state(self.0.src_routine.relocating_candidate.unwrap())
        {
            self.set_relocating_candidate(None)
        } else {
            self.concurrent_transition_to_try_relocating()
        }
    }

    fn concurrent_transition_to_try_relocating(&self) -> Self {
        self.0
            .as_try_relocating()
            .start_event_loop(self.0.src_routine.relocating_candidate.unwrap())
            .0
            .as_top_level_src()
    }

    fn transition_exit_try_relocating(&self) -> Self {
        self.set_relocating_candidate(None)
    }

    fn discard(&self) -> Self {
        self.clone()
    }

    fn vote_parsec_relocation_trigger(&self) -> Self {
        self.0.action.vote_parsec(ParsecVote::RelocationTrigger);
        self.clone()
    }

    fn set_relocating_candidate(&self, candidate: Option<Candidate>) -> Self {
        let mut state = self.clone();
        state.0.src_routine.relocating_candidate = candidate;
        state
    }

    fn set_candidate_relocating_state_if_needed(&self) -> Self {
        let candidate = self.0.src_routine.relocating_candidate.unwrap();
        if !self.0.action.is_candidate_relocating_state(candidate) {
            self.0.action.set_candidate_relocating_state(candidate);
        }
        self.clone()
    }

    fn vote_parsec_refuse_candidate(&self, candiddate: Candidate) -> Self {
        self.0
            .action
            .vote_parsec(ParsecVote::RefuseCandidate(candiddate));
        self.clone()
    }

    fn vote_parsec_relocation_response(&self, candiddate: Candidate, section: SectionInfo) -> Self {
        self.0
            .action
            .vote_parsec(ParsecVote::RelocateResponse(candiddate, section));
        self.clone()
    }
}

#[derive(Debug, PartialEq, Default, Clone)]
struct TryRelocating(State);

// TryRelocating Sub Routine
impl TryRelocating {
    fn start_event_loop(&self, candidate: Candidate) -> Self {
        self.0
            .with_src_sub_routine_try_relocating(Some(TryRelocatingState { candidate }))
            .as_try_relocating()
            .send_expect_candidate_rpc()
    }

    fn exit_event_loop(&self) -> Self {
        self.0
            .with_src_sub_routine_try_relocating(None)
            .as_top_level_src()
            .transition_exit_try_relocating()
            .0
            .as_try_relocating()
    }

    fn try_next(&self, event: Event) -> Option<State> {
        match event {
            Event::ParsecConsensus(vote) => self.try_consensus(vote),
            _ => None,
        }
        .map(|state| state.0)
    }

    fn try_consensus(&self, vote: ParsecVote) -> Option<Self> {
        if vote.candidate() != Some(self.candidate()) {
            return None;
        }

        match vote {
            ParsecVote::RefuseCandidate(_) => Some(self.exit_event_loop()),
            ParsecVote::RelocateResponse(_, section) => Some(self.remove_node(section)),
            // Delegate to other event loops
            _ => None,
        }
    }

    fn routine_state(&self) -> &TryRelocatingState {
        match &self.0.src_routine.sub_routine_try_relocating {
            Some(state) => state,
            _ => panic!("Expect AcceptAsCandidate {:?}", &self),
        }
    }

    fn mut_routine_state(&mut self) -> &mut TryRelocatingState {
        let clone = self.clone();
        match &mut self.0.src_routine.sub_routine_try_relocating {
            Some(state) => state,
            _ => panic!("Expect AcceptAsCandidate {:?}", &clone),
        }
    }

    fn send_expect_candidate_rpc(&self) -> Self {
        self.0
            .action
            .send_rpc(Rpc::ExpectCandidate(self.candidate()));
        self.clone()
    }

    fn remove_node(&self, section: SectionInfo) -> Self {
        self.0
            .action
            .send_candidate_relocated_info(self.candidate(), section);
        self.0.action.remove_node(self.candidate());
        self.exit_event_loop()
    }

    fn candidate(&self) -> Candidate {
        self.routine_state().candidate
    }
}

//////////////////
/// Joining Single Node
//////////////////

#[derive(Debug, PartialEq, Default, Clone)]
struct JoiningRelocateCandidate(JoiningState);

impl JoiningRelocateCandidate {
    fn start_event_loop(&self, new_section: &SectionInfo) -> Self {
        self.store_destination_members(new_section)
            .send_connection_info_requests()
            .start_resend_info_timeout()
            .start_refused_timeout()
    }

    fn try_next(&self, event: Event) -> Option<JoiningState> {
        match event {
            Event::Rpc(rpc) => self.try_rpc(rpc),
            Event::LocalEvent(local_event) => self.try_local_event(local_event),
            _ => None,
        }
        .or_else(|| Some(self.discard()))
        .map(|state| state.0)
    }

    fn try_rpc(&self, rpc: Rpc) -> Option<Self> {
        if let Rpc::NodeApproval(candidate, info) = &rpc {
            if self.0.action.is_our_name(Name(candidate.0.name)) {
                return Some(self.exit(*info));
            } else {
                return None;
            }
        }

        if !rpc
            .destination()
            .map(|name| self.0.action.is_our_name(name))
            .unwrap_or(false)
        {
            return None;
        }

        match rpc {
            Rpc::ConnectionInfoResponse {
                source,
                connection_info,
                ..
            } => Some(self.connect_and_send_candidate_info(source, connection_info)),
            Rpc::ResourceProof { proof, source, .. } => {
                Some(self.start_compute_resource_proof(source, proof))
            }
            Rpc::ResourceProofReceipt { source, .. } => Some(self.send_next_proof_response(source)),
            _ => None,
        }
    }

    fn try_local_event(&self, local_event: LocalEvent) -> Option<Self> {
        match local_event {
            LocalEvent::ComputeResourceProofForElder(source, proof) => {
                Some(self.send_first_proof_response(source, proof))
            }
            LocalEvent::JoiningTimeoutResendCandidateInfo => Some(
                self.send_connection_info_requests()
                    .start_resend_info_timeout(),
            ),
            _ => None,
        }
    }

    fn exit(&self, info: GenesisPfxInfo) -> Self {
        let mut state = self.clone();
        state.0.join_routine.has_resource_proofs.clear();
        state.0.join_routine.routine_complete = Some(info);
        state
    }

    fn discard(&self) -> Self {
        self.clone()
    }

    fn store_destination_members(&self, section: &SectionInfo) -> Self {
        let mut state = self.clone();

        let members = state.0.action.get_section_members(section);
        state.0.join_routine.has_resource_proofs = members
            .iter()
            .map(|node| (Name(node.0.name), (false, None)))
            .collect();
        state
    }

    fn send_connection_info_requests(&self) -> Self {
        let has_resource_proofs = &self.0.join_routine.has_resource_proofs;
        for (name, _) in has_resource_proofs.iter().filter(|(_, value)| !value.0) {
            self.0.action.send_connection_info_request(*name);
        }

        self.clone()
    }

    fn send_first_proof_response(&self, source: Name, mut proof_source: ProofSource) -> Self {
        let mut state = self.clone();
        let proof = state
            .0
            .join_routine
            .has_resource_proofs
            .get_mut(&source)
            .unwrap();

        let next_part = proof_source.next_part();
        proof.1 = Some(proof_source);

        state
            .0
            .action
            .send_resource_proof_response(source, next_part);
        state
    }

    fn send_next_proof_response(&self, source: Name) -> Self {
        let mut state = self.clone();
        let proof_source = &mut state
            .0
            .join_routine
            .has_resource_proofs
            .get_mut(&source)
            .unwrap()
            .1
            .as_mut()
            .unwrap();

        let next_part = proof_source.next_part();
        state
            .0
            .action
            .send_resource_proof_response(source, next_part);
        state
    }

    fn connect_and_send_candidate_info(&self, source: Name, connect_info: i32) -> Self {
        self.0.action.send_candidate_info(source);
        self.clone()
    }

    fn start_resend_info_timeout(&self) -> Self {
        self.0
            .action
            .schedule_event(LocalEvent::JoiningTimeoutResendCandidateInfo);
        self.clone()
    }

    fn start_refused_timeout(&self) -> Self {
        self.0
            .action
            .schedule_event(LocalEvent::JoiningTimeoutRefused);
        self.clone()
    }

    fn start_compute_resource_proof(&self, source: Name, proof: ProofRequest) -> Self {
        let mut state = self.clone();
        state.0.action.start_compute_resource_proof(source, proof);
        let proof = state
            .0
            .join_routine
            .has_resource_proofs
            .get_mut(&source)
            .unwrap();
        if !proof.0 {
            *proof = (true, None);
        }
        state
    }
}

//////////////////
/// Utilities
//////////////////

#[derive(Debug, PartialEq, Default, Clone)]
struct CheckAndProcessElderChangeState {
    wait_votes: Vec<ParsecVote>,
    change_elder: Option<ChangeElder>,
}

#[derive(Debug, PartialEq, Clone)]
struct AcceptAsCandidateState {
    candidate: Candidate,
    got_candidate_info: bool,
    voted_online: bool,
}

impl AcceptAsCandidateState {
    fn new(candidate: Candidate) -> Self {
        Self {
            candidate,
            got_candidate_info: false,
            voted_online: false,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
struct TryRelocatingState {
    candidate: Candidate,
}

#[derive(Debug, PartialEq, Default, Clone)]
struct DstRoutineState {
    is_processing_candidate: bool,
    sub_routine_accept_as_candidate: Option<AcceptAsCandidateState>,
}

#[derive(Debug, PartialEq, Default, Clone)]
struct SrcRoutineState {
    relocating_candidate: Option<Candidate>,
    sub_routine_try_relocating: Option<TryRelocatingState>,
}

// The very top level event loop deciding how the sub event loops are processed
#[derive(Debug, PartialEq, Default, Clone)]
struct State {
    action: Action,
    failure: Option<Event>,
    dst_routine: DstRoutineState,
    src_routine: SrcRoutineState,
    check_and_process_elder_change_routine: CheckAndProcessElderChangeState,
}

impl State {
    fn try_next(&self, event: Event) -> Option<Self> {
        let dst = &self.dst_routine;
        let src = &self.src_routine;

        if let Some(next) = self.as_check_and_process_elder_change().try_next(event) {
            return Some(next);
        }

        if src.sub_routine_try_relocating.is_some() {
            if let Some(next) = self.as_try_relocating().try_next(event) {
                return Some(next);
            }
        }

        if let Some(next) = self.as_top_level_src().try_next(event) {
            return Some(next);
        }

        if dst.sub_routine_accept_as_candidate.is_some() {
            if let Some(next) = self.as_accept_as_candidate().try_next(event) {
                return Some(next);
            }
        }

        if let Some(next) = self.as_top_level_dst().try_next(event) {
            return Some(next);
        }

        match event {
            // These should only happen if a routine started them, so it should have
            // handled them too, but other routine are not there yet and we want to test
            // these do not fail.
            Event::ParsecConsensus(ParsecVote::RemoveElderNode(_))
            | Event::ParsecConsensus(ParsecVote::AddElderNode(_))
            | Event::ParsecConsensus(ParsecVote::NewSectionInfo(_)) => Some(self.clone()),

            _ => None,
        }
    }

    fn as_top_level_dst(&self) -> TopLevelDst {
        TopLevelDst(self.clone())
    }

    fn as_accept_as_candidate(&self) -> AcceptAsCandidate {
        AcceptAsCandidate(self.clone())
    }

    fn as_check_and_process_elder_change(&self) -> CheckAndProcessElderChange {
        CheckAndProcessElderChange(self.clone())
    }

    fn as_top_level_src(&self) -> TopLevelSrc {
        TopLevelSrc(self.clone())
    }

    fn as_try_relocating(&self) -> TryRelocating {
        TryRelocating(self.clone())
    }

    fn failure_event(&self, event: Event) -> Self {
        Self {
            failure: Some(event),
            ..self.clone()
        }
    }

    fn with_dst_sub_routine_accept_as_candidate(
        &self,
        sub_routine_accept_as_candidate: Option<AcceptAsCandidateState>,
    ) -> Self {
        Self {
            dst_routine: DstRoutineState {
                sub_routine_accept_as_candidate,
                ..self.dst_routine.clone()
            },
            ..self.clone()
        }
    }

    fn with_src_sub_routine_try_relocating(
        &self,
        sub_routine_try_relocating: Option<TryRelocatingState>,
    ) -> Self {
        Self {
            src_routine: SrcRoutineState {
                sub_routine_try_relocating,
                ..self.src_routine.clone()
            },
            ..self.clone()
        }
    }
}

#[derive(Debug, PartialEq, Default, Clone)]
struct JoiningRelocateCandidateState {
    has_resource_proofs: BTreeMap<Name, (bool, Option<ProofSource>)>,
    routine_complete: Option<GenesisPfxInfo /*output*/>,
}

// The very top level event loop deciding how the sub event loops are processed
#[derive(Debug, PartialEq, Default, Clone)]
struct JoiningState {
    action: Action,
    failure: Option<Event>,
    join_routine: JoiningRelocateCandidateState,
}

impl JoiningState {
    fn start(&self, new_section: &SectionInfo) -> Self {
        self.as_joining_relocate_candidate()
            .start_event_loop(new_section)
            .0
    }

    fn try_next(&self, event: Event) -> Option<Self> {
        if let Some(next) = self.as_joining_relocate_candidate().try_next(event) {
            return Some(next);
        }

        None
    }

    fn as_joining_relocate_candidate(&self) -> JoiningRelocateCandidate {
        JoiningRelocateCandidate(self.clone())
    }

    fn failure_event(&self, event: Event) -> Self {
        Self {
            failure: Some(event),
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! to_collect {
    ($($item:expr),*) => {{
        let mut val = Vec::new();
        $(
            let _ = val.push($item.clone());
        )*
        val.into_iter().collect()
    }}
}

    const CANDIDATE_1: Candidate = Candidate(Attributes { name: 1, age: 10 });
    const CANDIDATE_2: Candidate = Candidate(Attributes { name: 2, age: 10 });
    const CANDIDATE_130: Candidate = Candidate(Attributes { name: 130, age: 30 });
    const CANDIDATE_205: Candidate = Candidate(Attributes { name: 205, age: 5 });
    const OTHER_SECTION_1: Section = Section(1);
    const DST_SECTION_200: Section = Section(200);

    const NODE_1: Node = Node(Attributes { name: 1, age: 10 });
    const ADD_PROOFING_NODE_1: NodeChange =
        NodeChange::AddResourceProofing(Node(Attributes { name: 1, age: 10 }));
    const SET_ONLINE_NODE_1: NodeChange = NodeChange::Online(Node(Attributes { name: 1, age: 10 }));
    const REMOVE_NODE_1: NodeChange = NodeChange::Remove(Node(Attributes { name: 1, age: 10 }));

    const ADD_PROOFING_NODE_2: NodeChange =
        NodeChange::AddResourceProofing(Node(Attributes { name: 2, age: 10 }));

    const NODE_ELDER_109: Node = Node(Attributes { name: 109, age: 9 });
    const NODE_ELDER_110: Node = Node(Attributes { name: 110, age: 10 });
    const NODE_ELDER_111: Node = Node(Attributes { name: 111, age: 11 });
    const NODE_ELDER_130: Node = Node(Attributes { name: 130, age: 30 });
    const NODE_ELDER_131: Node = Node(Attributes { name: 131, age: 31 });
    const NODE_ELDER_132: Node = Node(Attributes { name: 132, age: 32 });

    const NAME_109: Name = Name(NODE_ELDER_109.0.name);
    const NAME_110: Name = Name(NODE_ELDER_110.0.name);
    const NAME_111: Name = Name(NODE_ELDER_111.0.name);

    const YOUNG_ADULT_205: Node = Node(Attributes { name: 205, age: 5 });
    const SECTION_INFO_1: SectionInfo = SectionInfo(OUR_SECTION, 1);
    const SECTION_INFO_2: SectionInfo = SectionInfo(OUR_SECTION, 2);
    const DST_SECTION_INFO_200: SectionInfo = SectionInfo(DST_SECTION_200, 0);

    const CANDIDATE_INFO_VALID_RPC_1: Rpc = Rpc::CandidateInfo {
        candidate: CANDIDATE_1,
        destination: OUR_NAME,
        valid: true,
    };
    const OUR_SECTION: Section = Section(0);
    const OUR_NODE: Node = NODE_ELDER_132;
    const OUR_NAME: Name = Name(OUR_NODE.0.name);
    const OUR_NODE_CANDIDATE: Candidate = Candidate(NODE_ELDER_132.0);
    const OUR_PROOF_REQUEST: ProofRequest = ProofRequest { value: OUR_NAME.0 };
    const OUR_INITIAL_SECTION_INFO: SectionInfo = SectionInfo(OUR_SECTION, 0);
    const OUR_GENESIS_INFO: GenesisPfxInfo = GenesisPfxInfo(OUR_INITIAL_SECTION_INFO);

    lazy_static! {
        static ref INNER_ACTION_132: InnerAction = InnerAction::new_with_our_attributes(OUR_NODE.0);
        static ref INNER_ACTION_YOUNG_ELDERS: InnerAction = INNER_ACTION_132
            .clone()
            .extend_current_nodes_with(
                &NodeState {
                    is_elder: true,
                    ..NodeState::default()
                },
                &[NODE_ELDER_109, NODE_ELDER_110, NODE_ELDER_132]
            )
            .extend_current_nodes_with(&NodeState::default(), &[YOUNG_ADULT_205]);
        static ref INNER_ACTION_OLD_ELDERS: InnerAction = INNER_ACTION_132
            .clone()
            .extend_current_nodes_with(
                &NodeState {
                    is_elder: true,
                    ..NodeState::default()
                },
                &[NODE_ELDER_130, NODE_ELDER_131, NODE_ELDER_132]
            )
            .extend_current_nodes_with(&NodeState::default(), &[YOUNG_ADULT_205]);
        static ref INNER_ACTION_YOUNG_ELDERS_WITH_WAITING_ELDER: InnerAction = INNER_ACTION_132
            .clone()
            .extend_current_nodes_with(
                &NodeState {
                    is_elder: true,
                    ..NodeState::default()
                },
                &[NODE_ELDER_109, NODE_ELDER_110, NODE_ELDER_111]
            )
            .extend_current_nodes_with(&NodeState::default(), &[NODE_ELDER_130]);
        static ref INNER_ACTION_WITH_DST_SECTION_200: InnerAction =
            INNER_ACTION_132.clone().with_section_members(
                &DST_SECTION_INFO_200,
                &[NODE_ELDER_109, NODE_ELDER_110, NODE_ELDER_111]
            );
        static ref SWAP_ELDER_109_NODE_1_SECTION_INFO_1: (ChangeElder, Vec<ParsecVote>) = (
            ChangeElder {
                changes: vec![(NODE_1, true), (NODE_ELDER_109, false),],
                new_section: SECTION_INFO_1,
            },
            vec![
                ParsecVote::AddElderNode(NODE_1),
                ParsecVote::RemoveElderNode(NODE_ELDER_109),
                ParsecVote::NewSectionInfo(SECTION_INFO_1),
            ]
        );
        static ref SWAP_ELDER_130_YOUNG_205_SECTION_INFO_1: (ChangeElder, Vec<ParsecVote>) = (
            ChangeElder {
                changes: vec![(YOUNG_ADULT_205, true), (NODE_ELDER_130, false),],
                new_section: SECTION_INFO_1,
            },
            vec![
                ParsecVote::AddElderNode(YOUNG_ADULT_205),
                ParsecVote::RemoveElderNode(NODE_ELDER_130),
                ParsecVote::NewSectionInfo(SECTION_INFO_1),
            ]
        );
    }

    #[derive(Debug, PartialEq, Default, Clone)]
    struct AssertState {
        action_our_votes: Vec<ParsecVote>,
        action_our_rpcs: Vec<Rpc>,
        action_our_nodes: Vec<NodeChange>,
        action_our_events: Vec<LocalEvent>,
        action_our_section: SectionInfo,
        dst_routine: DstRoutineState,
        src_routine: SrcRoutineState,
        check_and_process_elder_change_routine: CheckAndProcessElderChangeState,
    }

    #[derive(Debug, PartialEq, Default, Clone)]
    struct AssertJoiningState {
        action_our_votes: Vec<ParsecVote>,
        action_our_rpcs: Vec<Rpc>,
        action_our_nodes: Vec<NodeChange>,
        action_our_events: Vec<LocalEvent>,
        action_our_section: SectionInfo,
        join_routine: JoiningRelocateCandidateState,
    }

    fn process_events(mut state: State, events: &[Event]) -> State {
        for event in events.iter().cloned() {
            state = match state.try_next(event) {
                Some(next_state) => next_state,
                None => state.failure_event(event),
            };

            if state.failure.is_some() {
                break;
            }
        }

        state
    }

    fn process_joining_events(mut state: JoiningState, events: &[Event]) -> JoiningState {
        for event in events.iter().cloned() {
            state = match state.try_next(event) {
                Some(next_state) => next_state,
                None => state.failure_event(event),
            };

            if state.failure.is_some() {
                break;
            }
        }

        state
    }

    fn run_test(
        test_name: &str,
        start_state: &State,
        events: &[Event],
        expected_state: &AssertState,
    ) {
        let final_state = process_events(start_state.clone(), &events);
        let action = final_state.action.inner();

        let final_state = (
            AssertState {
                action_our_rpcs: action.our_rpc,
                action_our_votes: action.our_votes,
                action_our_nodes: action.our_nodes,
                action_our_events: action.our_events,
                action_our_section: action.our_section,
                dst_routine: final_state.dst_routine,
                src_routine: final_state.src_routine,
                check_and_process_elder_change_routine: final_state
                    .check_and_process_elder_change_routine,
            },
            final_state.failure,
        );
        let expected_state = (expected_state.clone(), None);

        assert_eq!(expected_state, final_state, "{}", test_name);
    }

    fn run_joining_test(
        test_name: &str,
        start_state: &JoiningState,
        events: &[Event],
        expected_state: &AssertJoiningState,
    ) {
        let final_state = process_joining_events(start_state.clone(), &events);
        let action = final_state.action.inner();

        let final_state = (
            AssertJoiningState {
                action_our_rpcs: action.our_rpc,
                action_our_votes: action.our_votes,
                action_our_nodes: action.our_nodes,
                action_our_events: action.our_events,
                action_our_section: action.our_section,
                join_routine: final_state.join_routine,
            },
            final_state.failure,
        );
        let expected_state = (expected_state.clone(), None);

        assert_eq!(expected_state, final_state, "{}", test_name);
    }

    fn arrange_initial_state(state: &State, events: &[Event]) -> State {
        let state = process_events(state.clone(), events);
        state.action.remove_processed_state();
        state
    }

    fn arrange_initial_joining_state(state: &JoiningState, events: &[Event]) -> JoiningState {
        let state = process_joining_events(state.clone(), events);
        state.action.remove_processed_state();
        state
    }

    fn intial_state_young_elders() -> State {
        State {
            action: Action::new(INNER_ACTION_YOUNG_ELDERS.clone()),
            ..Default::default()
        }
    }

    fn intial_state_old_elders() -> State {
        State {
            action: Action::new(INNER_ACTION_OLD_ELDERS.clone()),
            ..Default::default()
        }
    }

    fn intial_joining_state_with_dst_200() -> JoiningState {
        JoiningState {
            action: Action::new(INNER_ACTION_WITH_DST_SECTION_200.clone()),
            ..Default::default()
        }
    }

    fn routine_state_accept_as_candidate(
        accept_as_candidate: AcceptAsCandidateState,
    ) -> DstRoutineState {
        DstRoutineState {
            is_processing_candidate: true,
            sub_routine_accept_as_candidate: Some(accept_as_candidate),
            ..Default::default()
        }
    }

    //////////////////
    /// Dst
    //////////////////

    #[test]
    fn test_rpc_expect_candidate() {
        run_test(
            "Get RPC ExpectCandidate",
            &intial_state_old_elders(),
            &[Rpc::ExpectCandidate(CANDIDATE_1).to_event()],
            &AssertState {
                action_our_votes: vec![ParsecVote::ExpectCandidate(CANDIDATE_1)],
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate() {
        run_test(
            "Get Parsec ExpectCandidate",
            &intial_state_old_elders(),
            &[ParsecVote::ExpectCandidate(CANDIDATE_1).to_event()],
            &AssertState {
                action_our_nodes: vec![ADD_PROOFING_NODE_1],
                action_our_rpcs: vec![Rpc::RelocateResponse(CANDIDATE_1, OUR_INITIAL_SECTION_INFO)],
                dst_routine: routine_state_accept_as_candidate(AcceptAsCandidateState {
                    candidate: CANDIDATE_1,
                    got_candidate_info: false,
                    voted_online: false,
                }),
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_candidate_info() {
        let initial_state = arrange_initial_state(
            &intial_state_old_elders(),
            &[ParsecVote::ExpectCandidate(CANDIDATE_1).to_event()],
        );

        run_test(
            "Get Parsec ExpectCandidate then Purge",
            &initial_state,
            &vec![CANDIDATE_INFO_VALID_RPC_1.to_event()],
            &AssertState {
                action_our_rpcs: vec![Rpc::ResourceProof {
                    candidate: CANDIDATE_1,
                    source: OUR_NAME,
                    proof: OUR_PROOF_REQUEST,
                }],
                dst_routine: routine_state_accept_as_candidate(AcceptAsCandidateState {
                    candidate: CANDIDATE_1,
                    got_candidate_info: true,
                    voted_online: false,
                }),
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_candidate_info_twice() {
        let initial_state = arrange_initial_state(
            &intial_state_old_elders(),
            &[
                ParsecVote::ExpectCandidate(CANDIDATE_1).to_event(),
                CANDIDATE_INFO_VALID_RPC_1.to_event(),
            ],
        );

        run_test(
            "Get Parsec ExpectCandidate then Purge",
            &initial_state,
            &vec![CANDIDATE_INFO_VALID_RPC_1.to_event()],
            &AssertState {
                dst_routine: routine_state_accept_as_candidate(AcceptAsCandidateState {
                    candidate: CANDIDATE_1,
                    got_candidate_info: true,
                    voted_online: false,
                }),
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_invalid_candidate_info() {
        let initial_state = arrange_initial_state(
            &intial_state_old_elders(),
            &[ParsecVote::ExpectCandidate(CANDIDATE_1).to_event()],
        );

        run_test(
            "Get Parsec ExpectCandidate then Purge",
            &initial_state,
            &vec![Rpc::CandidateInfo {
                candidate: CANDIDATE_1,
                destination: OUR_NAME,
                valid: false,
            }
            .to_event()],
            &AssertState {
                action_our_votes: vec![ParsecVote::PurgeCandidate(CANDIDATE_1)],
                dst_routine: routine_state_accept_as_candidate(AcceptAsCandidateState {
                    candidate: CANDIDATE_1,
                    got_candidate_info: false,
                    voted_online: false,
                }),
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_time_out() {
        let initial_state = arrange_initial_state(
            &intial_state_old_elders(),
            &[ParsecVote::ExpectCandidate(CANDIDATE_1).to_event()],
        );

        run_test(
            "Get Parsec ExpectCandidate then Purge",
            &initial_state,
            &vec![LocalEvent::TimeoutAccept.to_event()],
            &AssertState {
                action_our_votes: vec![ParsecVote::PurgeCandidate(CANDIDATE_1)],
                dst_routine: routine_state_accept_as_candidate(AcceptAsCandidateState {
                    candidate: CANDIDATE_1,
                    got_candidate_info: false,
                    voted_online: false,
                }),
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_wrong_candidate_info() {
        let initial_state = arrange_initial_state(
            &intial_state_old_elders(),
            &[ParsecVote::ExpectCandidate(CANDIDATE_1).to_event()],
        );

        run_test(
            "Get Parsec ExpectCandidate then Purge",
            &initial_state,
            &vec![Rpc::CandidateInfo {
                candidate: CANDIDATE_2,
                destination: OUR_NAME,
                valid: true,
            }
            .to_event()],
            &AssertState {
                dst_routine: routine_state_accept_as_candidate(AcceptAsCandidateState {
                    candidate: CANDIDATE_1,
                    got_candidate_info: false,
                    voted_online: false,
                }),
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_candidate_info_then_part_proof() {
        let initial_state = arrange_initial_state(
            &intial_state_old_elders(),
            &[
                ParsecVote::ExpectCandidate(CANDIDATE_1).to_event(),
                CANDIDATE_INFO_VALID_RPC_1.to_event(),
            ],
        );

        run_test(
            "Get Parsec ExpectCandidate then Purge",
            &initial_state,
            &[Rpc::ResourceProofResponse {
                candidate: CANDIDATE_1,
                destination: OUR_NAME,
                proof: Proof::ValidPart,
            }
            .to_event()],
            &AssertState {
                action_our_rpcs: vec![Rpc::ResourceProofReceipt {
                    candidate: CANDIDATE_1,
                    source: OUR_NAME,
                }],
                dst_routine: routine_state_accept_as_candidate(AcceptAsCandidateState {
                    candidate: CANDIDATE_1,
                    got_candidate_info: true,
                    voted_online: false,
                }),
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_candidate_info_then_end_proof() {
        let initial_state = arrange_initial_state(
            &intial_state_old_elders(),
            &[
                ParsecVote::ExpectCandidate(CANDIDATE_1).to_event(),
                CANDIDATE_INFO_VALID_RPC_1.to_event(),
            ],
        );

        run_test(
            "Get Parsec ExpectCandidate then Purge",
            &initial_state,
            &[Rpc::ResourceProofResponse {
                candidate: CANDIDATE_1,
                destination: OUR_NAME,
                proof: Proof::ValidEnd,
            }
            .to_event()],
            &AssertState {
                action_our_votes: vec![ParsecVote::Online(CANDIDATE_1)],
                dst_routine: routine_state_accept_as_candidate(AcceptAsCandidateState {
                    candidate: CANDIDATE_1,
                    got_candidate_info: true,
                    voted_online: true,
                }),
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_candidate_info_then_end_proof_twice() {
        let initial_state = arrange_initial_state(
            &intial_state_old_elders(),
            &[
                ParsecVote::ExpectCandidate(CANDIDATE_1).to_event(),
                CANDIDATE_INFO_VALID_RPC_1.to_event(),
                Rpc::ResourceProofResponse {
                    candidate: CANDIDATE_1,
                    destination: OUR_NAME,
                    proof: Proof::ValidEnd,
                }
                .to_event(),
            ],
        );

        run_test(
            "Get Parsec ExpectCandidate then Purge",
            &initial_state,
            &[Rpc::ResourceProofResponse {
                candidate: CANDIDATE_1,
                destination: OUR_NAME,
                proof: Proof::ValidEnd,
            }
            .to_event()],
            &AssertState {
                dst_routine: routine_state_accept_as_candidate(AcceptAsCandidateState {
                    candidate: CANDIDATE_1,
                    got_candidate_info: true,
                    voted_online: true,
                }),
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_candidate_info_then_invalid_proof() {
        let initial_state = arrange_initial_state(
            &intial_state_old_elders(),
            &[
                ParsecVote::ExpectCandidate(CANDIDATE_1).to_event(),
                CANDIDATE_INFO_VALID_RPC_1.to_event(),
            ],
        );

        run_test(
            "Get Parsec ExpectCandidate then Purge",
            &initial_state,
            &[Rpc::ResourceProofResponse {
                candidate: CANDIDATE_1,
                destination: OUR_NAME,
                proof: Proof::Invalid,
            }
            .to_event()],
            &AssertState {
                dst_routine: routine_state_accept_as_candidate(AcceptAsCandidateState {
                    candidate: CANDIDATE_1,
                    got_candidate_info: true,
                    voted_online: false,
                }),
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_candidate_info_then_end_proof_wrong_candidate() {
        let initial_state = arrange_initial_state(
            &intial_state_old_elders(),
            &[
                ParsecVote::ExpectCandidate(CANDIDATE_1).to_event(),
                CANDIDATE_INFO_VALID_RPC_1.to_event(),
            ],
        );

        run_test(
            "Get Parsec ExpectCandidate then Purge",
            &initial_state,
            &[Rpc::ResourceProofResponse {
                candidate: CANDIDATE_2,
                destination: OUR_NAME,
                proof: Proof::ValidEnd,
            }
            .to_event()],
            &AssertState {
                dst_routine: routine_state_accept_as_candidate(AcceptAsCandidateState {
                    candidate: CANDIDATE_1,
                    got_candidate_info: true,
                    voted_online: false,
                }),
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_purge_and_online_for_wrong_candidate() {
        let initial_state = arrange_initial_state(
            &intial_state_young_elders(),
            &[ParsecVote::ExpectCandidate(CANDIDATE_1).to_event()],
        );

        run_test(
            "Get Parsec ExpectCandidate then Purge",
            &initial_state,
            &vec![
                ParsecVote::Online(CANDIDATE_2).to_event(),
                ParsecVote::PurgeCandidate(CANDIDATE_2).to_event(),
            ],
            &AssertState {
                dst_routine: routine_state_accept_as_candidate(AcceptAsCandidateState {
                    candidate: CANDIDATE_1,
                    got_candidate_info: false,
                    voted_online: false,
                }),
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_online_no_elder_change() {
        let initial_state = arrange_initial_state(
            &intial_state_old_elders(),
            &[ParsecVote::ExpectCandidate(CANDIDATE_1).to_event()],
        );

        run_test(
            "Get Parsec ExpectCandidate then Online (No Elder Change)",
            &initial_state,
            &[ParsecVote::Online(CANDIDATE_1).to_event()],
            &AssertState {
                action_our_rpcs: vec![Rpc::NodeApproval(CANDIDATE_1, OUR_GENESIS_INFO)],
                action_our_nodes: vec![SET_ONLINE_NODE_1],
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_online_elder_change() {
        let initial_state = arrange_initial_state(
            &intial_state_young_elders(),
            &[ParsecVote::ExpectCandidate(CANDIDATE_1).to_event()],
        );

        run_test(
            "Get Parsec ExpectCandidate then Online (Elder Change)",
            &initial_state,
            &vec![
                ParsecVote::Online(CANDIDATE_1).to_event(),
                ParsecVote::CheckElder.to_event(),
            ],
            &AssertState {
                action_our_rpcs: vec![Rpc::NodeApproval(CANDIDATE_1, OUR_GENESIS_INFO)],
                action_our_votes: SWAP_ELDER_109_NODE_1_SECTION_INFO_1.1.clone(),
                action_our_nodes: vec![SET_ONLINE_NODE_1],
                check_and_process_elder_change_routine: CheckAndProcessElderChangeState {
                    change_elder: Some(SWAP_ELDER_109_NODE_1_SECTION_INFO_1.0.clone()),
                    wait_votes: SWAP_ELDER_109_NODE_1_SECTION_INFO_1.1.clone(),
                },
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_online_elder_change_get_wrong_votes() {
        let initial_state = arrange_initial_state(
            &intial_state_young_elders(),
            &[
                ParsecVote::ExpectCandidate(CANDIDATE_1).to_event(),
                ParsecVote::Online(CANDIDATE_1).to_event(),
                ParsecVote::CheckElder.to_event(),
            ],
        );

        run_test(
            "Get Parsec ExpectCandidate then Online (Elder Change) RemoveElderNode for wrong elder,\
            AddElderNode for wrong node, NewSectionInfo for wrong section",
            &initial_state,
            &vec![
                ParsecVote::RemoveElderNode(NODE_1).to_event(),
                ParsecVote::AddElderNode(NODE_ELDER_109).to_event(),
                ParsecVote::NewSectionInfo(SECTION_INFO_2).to_event(),
            ],
            &AssertState {
                check_and_process_elder_change_routine: CheckAndProcessElderChangeState {
                    change_elder: Some(SWAP_ELDER_109_NODE_1_SECTION_INFO_1.0.clone()),
                    wait_votes: SWAP_ELDER_109_NODE_1_SECTION_INFO_1.1.clone(),
                },
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_online_elder_change_remove_elder() {
        let initial_state = arrange_initial_state(
            &intial_state_young_elders(),
            &[
                ParsecVote::ExpectCandidate(CANDIDATE_1).to_event(),
                ParsecVote::Online(CANDIDATE_1).to_event(),
                ParsecVote::CheckElder.to_event(),
            ],
        );

        run_test(
            "Get Parsec ExpectCandidate then Online (Elder Change) then RemoveElderNode",
            &initial_state,
            &vec![ParsecVote::RemoveElderNode(NODE_ELDER_109).to_event()],
            &AssertState {
                check_and_process_elder_change_routine: CheckAndProcessElderChangeState {
                    change_elder: Some(SWAP_ELDER_109_NODE_1_SECTION_INFO_1.0.clone()),
                    wait_votes: vec![
                        ParsecVote::AddElderNode(NODE_1),
                        ParsecVote::NewSectionInfo(SECTION_INFO_1),
                    ],
                },
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_online_elder_change_complete_elder() {
        let initial_state = arrange_initial_state(
            &intial_state_young_elders(),
            &[
                ParsecVote::ExpectCandidate(CANDIDATE_1).to_event(),
                ParsecVote::Online(CANDIDATE_1).to_event(),
                ParsecVote::RemoveElderNode(NODE_ELDER_109).to_event(),
            ],
        );

        run_test(
            "Get Parsec ExpectCandidate then Online (Elder Change) then \
             RemoveElderNode, AddElderNode, NewSectionInfo",
            &initial_state,
            &[
                ParsecVote::AddElderNode(NODE_1).to_event(),
                ParsecVote::NewSectionInfo(SECTION_INFO_1).to_event(),
            ],
            &AssertState::default(),
        );
    }

    #[test]
    fn test_parsec_expect_candidate_when_candidate_completed_with_elder_change() {
        let initial_state = arrange_initial_state(
            &intial_state_young_elders(),
            &[
                ParsecVote::ExpectCandidate(CANDIDATE_1).to_event(),
                ParsecVote::Online(CANDIDATE_1).to_event(),
                ParsecVote::RemoveElderNode(NODE_ELDER_109).to_event(),
                ParsecVote::AddElderNode(NODE_1).to_event(),
                ParsecVote::NewSectionInfo(SECTION_INFO_1).to_event(),
            ],
        );

        run_test(
            "Get Parsec ExpectCandidate after first candidate completed \
             with elder change",
            &initial_state,
            &[ParsecVote::ExpectCandidate(CANDIDATE_2).to_event()],
            &&AssertState {
                action_our_nodes: vec![ADD_PROOFING_NODE_2],
                action_our_rpcs: vec![Rpc::RelocateResponse(CANDIDATE_2, OUR_INITIAL_SECTION_INFO)],
                dst_routine: routine_state_accept_as_candidate(AcceptAsCandidateState {
                    candidate: CANDIDATE_2,
                    got_candidate_info: false,
                    voted_online: false,
                }),
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_then_purge() {
        let initial_state = arrange_initial_state(
            &intial_state_young_elders(),
            &[ParsecVote::ExpectCandidate(CANDIDATE_1).to_event()],
        );

        run_test(
            "Get Parsec ExpectCandidate then Purge",
            &initial_state,
            &vec![ParsecVote::PurgeCandidate(CANDIDATE_1).to_event()],
            &AssertState {
                action_our_nodes: vec![REMOVE_NODE_1],
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_twice() {
        let initial_state = arrange_initial_state(
            &intial_state_young_elders(),
            &[ParsecVote::ExpectCandidate(CANDIDATE_1).to_event()],
        );

        run_test(
            &"Get Parsec 2 ExpectCandidate",
            &initial_state,
            &vec![ParsecVote::ExpectCandidate(CANDIDATE_2).to_event()],
            &AssertState {
                action_our_rpcs: vec![Rpc::RefuseCandidate(CANDIDATE_2)],
                dst_routine: routine_state_accept_as_candidate(AcceptAsCandidateState {
                    candidate: CANDIDATE_1,
                    got_candidate_info: false,
                    voted_online: false,
                }),
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_expect_candidate_with_shorter_known_section() {
        let initial_state = State {
            action: Action::new(InnerAction {
                shortest_prefix: Some(OTHER_SECTION_1),
                ..INNER_ACTION_OLD_ELDERS.clone()
            }),
            ..State::default()
        };

        run_test(
            &"Get Parsec ExpectCandidate with a shorter known section",
            &initial_state,
            &vec![ParsecVote::ExpectCandidate(CANDIDATE_1).to_event()],
            &AssertState {
                action_our_rpcs: vec![Rpc::ResendExpectCandidate(OTHER_SECTION_1, CANDIDATE_1)],
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_unexpected_purge_online() {
        run_test(
            "Get unexpected Parsec consensus Online and PurgeCandidate. \
             Candidate may have trigger both vote: only consider the first",
            &intial_state_old_elders(),
            &[
                ParsecVote::Online(CANDIDATE_1).to_event(),
                ParsecVote::PurgeCandidate(CANDIDATE_1).to_event(),
            ],
            &AssertState::default(),
        );
    }

    #[test]
    fn test_rpc_unexpected_candidate_info_resource_proof_response() {
        run_test(
            "Get unexpected RPC CandidateInfo and ResourceProofResponse. \
             Candidate RPC may arrive after candidate was pured or accepted",
            &intial_state_old_elders(),
            &[
                Rpc::CandidateInfo {
                    candidate: CANDIDATE_1,
                    destination: OUR_NAME,
                    valid: true,
                }
                .to_event(),
                Rpc::ResourceProofResponse {
                    candidate: CANDIDATE_1,
                    destination: OUR_NAME,
                    proof: Proof::ValidEnd,
                }
                .to_event(),
            ],
            &AssertState::default(),
        );
    }

    //////////////////
    /// Scr
    //////////////////
    #[test]
    fn test_local_event_relocation_trigger() {
        run_test(
            "Get RPC ExpectCandidate",
            &intial_state_old_elders(),
            &[LocalEvent::RelocationTrigger.to_event()],
            &AssertState {
                action_our_votes: vec![ParsecVote::RelocationTrigger],
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_relocation_trigger() {
        let initial_state = State {
            action: Action::new(InnerAction {
                node_to_relocate: Some(YOUNG_ADULT_205),
                ..INNER_ACTION_OLD_ELDERS.clone()
            }),
            ..State::default()
        };

        run_test(
            "Get Parsec ExpectCandidate",
            &initial_state,
            &[ParsecVote::RelocationTrigger.to_event()],
            &AssertState {
                action_our_rpcs: vec![Rpc::ExpectCandidate(CANDIDATE_205.clone())],
                action_our_nodes: vec![NodeChange::Relocating(YOUNG_ADULT_205.clone())],
                src_routine: SrcRoutineState {
                    relocating_candidate: Some(CANDIDATE_205.clone()),
                    sub_routine_try_relocating: Some(TryRelocatingState {
                        candidate: CANDIDATE_205.clone(),
                    }),
                    ..Default::default()
                },
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_relocate_trigger_elder_change() {
        let initial_state = State {
            action: Action::new(InnerAction {
                node_to_relocate: Some(NODE_ELDER_130),
                ..INNER_ACTION_OLD_ELDERS.clone()
            }),
            ..State::default()
        };

        run_test(
            "Get Parsec ExpectCandidate then Online (Elder Change)",
            &initial_state,
            &vec![
                ParsecVote::RelocationTrigger.to_event(),
                ParsecVote::CheckElder.to_event(),
            ],
            &AssertState {
                action_our_nodes: vec![NodeChange::Relocating(NODE_ELDER_130.clone())],
                action_our_votes: SWAP_ELDER_130_YOUNG_205_SECTION_INFO_1.1.clone(),
                check_and_process_elder_change_routine: CheckAndProcessElderChangeState {
                    change_elder: Some(SWAP_ELDER_130_YOUNG_205_SECTION_INFO_1.0.clone()),
                    wait_votes: SWAP_ELDER_130_YOUNG_205_SECTION_INFO_1.1.clone(),
                },
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_relocate_trigger_elder_change_complete() {
        let initial_state = arrange_initial_state(
            &State {
                action: Action::new(InnerAction {
                    node_to_relocate: Some(NODE_ELDER_130),
                    ..INNER_ACTION_OLD_ELDERS.clone()
                }),
                ..State::default()
            },
            &[
                ParsecVote::RelocationTrigger.to_event(),
                ParsecVote::CheckElder.to_event(),
            ],
        );

        run_test(
            "Get Parsec ExpectCandidate then Online (Elder Change)",
            &initial_state,
            &vec![
                ParsecVote::RemoveElderNode(NODE_ELDER_130).to_event(),
                ParsecVote::AddElderNode(YOUNG_ADULT_205).to_event(),
                ParsecVote::NewSectionInfo(SECTION_INFO_1).to_event(),
                ParsecVote::RelocationTrigger.to_event(),
            ],
            &AssertState {
                action_our_section: SECTION_INFO_1,
                action_our_nodes: vec![
                    NodeChange::Elder(YOUNG_ADULT_205, true),
                    NodeChange::Elder(NODE_ELDER_130, false),
                ],
                action_our_rpcs: vec![Rpc::ExpectCandidate(CANDIDATE_130.clone())],
                action_our_events: vec![LocalEvent::TimeoutCheckElder],
                src_routine: SrcRoutineState {
                    relocating_candidate: Some(Candidate(NODE_ELDER_130.0)),
                    sub_routine_try_relocating: Some(TryRelocatingState {
                        candidate: CANDIDATE_130.clone(),
                    }),
                    ..Default::default()
                },
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_relocation_trigger_refuse_candidate_rpc() {
        let initial_state = arrange_initial_state(
            &State {
                action: Action::new(InnerAction {
                    node_to_relocate: Some(YOUNG_ADULT_205),
                    ..INNER_ACTION_OLD_ELDERS.clone()
                }),
                ..State::default()
            },
            &[ParsecVote::RelocationTrigger.to_event()],
        );

        run_test(
            "Get Parsec ExpectCandidate",
            &initial_state,
            &[Rpc::RefuseCandidate(CANDIDATE_205.clone()).to_event()],
            &AssertState {
                action_our_votes: vec![ParsecVote::RefuseCandidate(CANDIDATE_205.clone())],
                src_routine: SrcRoutineState {
                    relocating_candidate: Some(CANDIDATE_205.clone()),
                    sub_routine_try_relocating: Some(TryRelocatingState {
                        candidate: CANDIDATE_205.clone(),
                    }),
                    ..Default::default()
                },
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_relocation_trigger_relocate_response_rpc() {
        let initial_state = arrange_initial_state(
            &State {
                action: Action::new(InnerAction {
                    node_to_relocate: Some(YOUNG_ADULT_205),
                    ..INNER_ACTION_OLD_ELDERS.clone()
                }),
                ..State::default()
            },
            &[ParsecVote::RelocationTrigger.to_event()],
        );

        run_test(
            "Get Parsec ExpectCandidate",
            &initial_state,
            &[Rpc::RelocateResponse(CANDIDATE_205, DST_SECTION_INFO_200).to_event()],
            &AssertState {
                action_our_votes: vec![ParsecVote::RelocateResponse(
                    CANDIDATE_205.clone(),
                    DST_SECTION_INFO_200,
                )],
                src_routine: SrcRoutineState {
                    relocating_candidate: Some(CANDIDATE_205.clone()),
                    sub_routine_try_relocating: Some(TryRelocatingState {
                        candidate: CANDIDATE_205.clone(),
                    }),
                    ..Default::default()
                },
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_relocation_trigger_accept() {
        let initial_state = arrange_initial_state(
            &State {
                action: Action::new(InnerAction {
                    node_to_relocate: Some(YOUNG_ADULT_205),
                    ..INNER_ACTION_OLD_ELDERS.clone()
                }),
                ..State::default()
            },
            &[ParsecVote::RelocationTrigger.to_event()],
        );

        run_test(
            "Get Parsec ExpectCandidate",
            &initial_state,
            &[ParsecVote::RelocateResponse(CANDIDATE_205, DST_SECTION_INFO_200).to_event()],
            &AssertState {
                action_our_rpcs: vec![Rpc::RelocatedInfo(
                    Candidate(YOUNG_ADULT_205.0.clone()),
                    DST_SECTION_INFO_200,
                )],
                action_our_nodes: vec![NodeChange::Remove(YOUNG_ADULT_205.clone())],
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_relocation_trigger_refuse() {
        let initial_state = arrange_initial_state(
            &State {
                action: Action::new(InnerAction {
                    node_to_relocate: Some(YOUNG_ADULT_205),
                    ..INNER_ACTION_OLD_ELDERS.clone()
                }),
                ..State::default()
            },
            &[ParsecVote::RelocationTrigger.to_event()],
        );

        run_test(
            "Get Parsec ExpectCandidate",
            &initial_state,
            &[ParsecVote::RefuseCandidate(CANDIDATE_205).to_event()],
            &AssertState::default(),
        );
    }

    #[test]
    fn test_parsec_relocation_trigger_refuse_trigger_again() {
        let initial_state = arrange_initial_state(
            &State {
                action: Action::new(InnerAction {
                    node_to_relocate: Some(YOUNG_ADULT_205),
                    ..INNER_ACTION_OLD_ELDERS.clone()
                }),
                ..State::default()
            },
            &[
                ParsecVote::RelocationTrigger.to_event(),
                ParsecVote::RefuseCandidate(CANDIDATE_205).to_event(),
            ],
        );

        run_test(
            "Get Parsec ExpectCandidate",
            &initial_state,
            &[ParsecVote::RelocationTrigger.to_event()],
            &AssertState {
                action_our_rpcs: vec![Rpc::ExpectCandidate(CANDIDATE_205.clone())],
                src_routine: SrcRoutineState {
                    relocating_candidate: Some(CANDIDATE_205.clone()),
                    sub_routine_try_relocating: Some(TryRelocatingState {
                        candidate: CANDIDATE_205.clone(),
                    }),
                    ..Default::default()
                },
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_parsec_relocation_trigger_elder_change_refuse_trigger_again() {
        let initial_state = arrange_initial_state(
            &State {
                action: Action::new(InnerAction {
                    node_to_relocate: Some(NODE_ELDER_130),
                    ..INNER_ACTION_OLD_ELDERS.clone()
                }),
                ..State::default()
            },
            &[
                ParsecVote::RelocationTrigger.to_event(),
                ParsecVote::CheckElder.to_event(),
                ParsecVote::RemoveElderNode(NODE_ELDER_130).to_event(),
                ParsecVote::AddElderNode(YOUNG_ADULT_205).to_event(),
                ParsecVote::NewSectionInfo(SECTION_INFO_1).to_event(),
                ParsecVote::RelocationTrigger.to_event(),
                ParsecVote::RefuseCandidate(CANDIDATE_130).to_event(),
            ],
        );

        run_test(
            "Get Parsec ExpectCandidate",
            &initial_state,
            &[ParsecVote::RelocationTrigger.to_event()],
            &AssertState {
                action_our_section: SECTION_INFO_1,
                action_our_rpcs: vec![Rpc::ExpectCandidate(CANDIDATE_130.clone())],
                src_routine: SrcRoutineState {
                    relocating_candidate: Some(CANDIDATE_130.clone()),
                    sub_routine_try_relocating: Some(TryRelocatingState {
                        candidate: CANDIDATE_130.clone(),
                    }),
                    ..Default::default()
                },
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_unexpected_refuse_candidate() {
        run_test(
            "Get RPC ExpectCandidate",
            &intial_state_old_elders(),
            &[Rpc::RefuseCandidate(CANDIDATE_205.clone()).to_event()],
            &AssertState {
                action_our_votes: vec![ParsecVote::RefuseCandidate(CANDIDATE_205.clone())],
                ..AssertState::default()
            },
        );
    }

    #[test]
    fn test_unexpected_relocate_response() {
        run_test(
            "Get RPC ExpectCandidate",
            &intial_state_old_elders(),
            &[Rpc::RelocateResponse(CANDIDATE_205.clone(), DST_SECTION_INFO_200).to_event()],
            &AssertState {
                action_our_votes: vec![ParsecVote::RelocateResponse(
                    CANDIDATE_205.clone(),
                    DST_SECTION_INFO_200,
                )],
                ..AssertState::default()
            },
        );
    }

    //////////////////
    /// Joining Relocate Node
    //////////////////

    #[test]
    fn test_joining_start() {
        run_joining_test(
            "",
            &intial_joining_state_with_dst_200().start(&DST_SECTION_INFO_200),
            &[],
            &AssertJoiningState {
                action_our_rpcs: vec![
                    Rpc::ConnectionInfoRequest {
                        source: OUR_NAME,
                        destination: NAME_109,
                        connection_info: OUR_NAME.0,
                    },
                    Rpc::ConnectionInfoRequest {
                        source: OUR_NAME,
                        destination: NAME_110,
                        connection_info: OUR_NAME.0,
                    },
                    Rpc::ConnectionInfoRequest {
                        source: OUR_NAME,
                        destination: NAME_111,
                        connection_info: OUR_NAME.0,
                    },
                ],
                action_our_events: vec![
                    LocalEvent::JoiningTimeoutResendCandidateInfo,
                    LocalEvent::JoiningTimeoutRefused,
                ],
                join_routine: JoiningRelocateCandidateState {
                    has_resource_proofs: to_collect![
                        (NAME_109, (false, None)),
                        (NAME_110, (false, None)),
                        (NAME_111, (false, None))
                    ],
                    ..JoiningRelocateCandidateState::default()
                },
                ..AssertJoiningState::default()
            },
        );
    }

    #[test]
    fn test_joining_receive_two_connection_info() {
        let initial_state = arrange_initial_joining_state(
            &intial_joining_state_with_dst_200().start(&DST_SECTION_INFO_200),
            &[],
        );

        run_joining_test(
            "",
            &initial_state,
            &[
                Rpc::ConnectionInfoResponse {
                    source: NAME_110,
                    destination: OUR_NAME,
                    connection_info: NAME_110.0,
                }
                .to_event(),
                Rpc::ConnectionInfoResponse {
                    source: NAME_111,
                    destination: OUR_NAME,
                    connection_info: NAME_111.0,
                }
                .to_event(),
            ],
            &AssertJoiningState {
                action_our_rpcs: vec![
                    Rpc::CandidateInfo {
                        candidate: OUR_NODE_CANDIDATE,
                        destination: NAME_110,
                        valid: true,
                    },
                    Rpc::CandidateInfo {
                        candidate: OUR_NODE_CANDIDATE,
                        destination: NAME_111,
                        valid: true,
                    },
                ],
                join_routine: JoiningRelocateCandidateState {
                    has_resource_proofs: to_collect![
                        (NAME_109, (false, None)),
                        (NAME_110, (false, None)),
                        (NAME_111, (false, None))
                    ],
                    ..JoiningRelocateCandidateState::default()
                },
                ..AssertJoiningState::default()
            },
        );
    }

    #[test]
    fn test_joining_receive_one_resource_proof() {
        let initial_state = arrange_initial_joining_state(
            &intial_joining_state_with_dst_200().start(&DST_SECTION_INFO_200),
            &[
                Rpc::ConnectionInfoResponse {
                    source: NAME_110,
                    destination: OUR_NAME,
                    connection_info: NAME_110.0,
                }
                .to_event(),
                Rpc::ConnectionInfoResponse {
                    source: NAME_111,
                    destination: OUR_NAME,
                    connection_info: NAME_111.0,
                }
                .to_event(),
            ],
        );

        run_joining_test(
            "",
            &initial_state,
            &[Rpc::ResourceProof {
                candidate: OUR_NODE_CANDIDATE,
                source: NAME_111,
                proof: ProofRequest { value: NAME_111.0 },
            }
            .to_event()],
            &AssertJoiningState {
                action_our_events: vec![LocalEvent::ComputeResourceProofForElder(
                    NAME_111,
                    ProofSource(2),
                )],
                join_routine: JoiningRelocateCandidateState {
                    has_resource_proofs: to_collect![
                        (NAME_109, (false, None)),
                        (NAME_110, (false, None)),
                        (NAME_111, (true, None))
                    ],
                    ..JoiningRelocateCandidateState::default()
                },
                ..AssertJoiningState::default()
            },
        );
    }

    #[test]
    fn test_joining_computed_one_proof_one_proof() {
        let initial_state = arrange_initial_joining_state(
            &intial_joining_state_with_dst_200().start(&DST_SECTION_INFO_200),
            &[
                Rpc::ConnectionInfoResponse {
                    source: NAME_111,
                    destination: OUR_NAME,
                    connection_info: NAME_111.0,
                }
                .to_event(),
                Rpc::ResourceProof {
                    candidate: OUR_NODE_CANDIDATE,
                    source: NAME_111,
                    proof: ProofRequest { value: NAME_111.0 },
                }
                .to_event(),
            ],
        );

        run_joining_test(
            "",
            &initial_state,
            &[LocalEvent::ComputeResourceProofForElder(NAME_111, ProofSource(2)).to_event()],
            &AssertJoiningState {
                action_our_rpcs: vec![Rpc::ResourceProofResponse {
                    candidate: OUR_NODE_CANDIDATE,
                    destination: NAME_111,
                    proof: Proof::ValidPart,
                }],
                join_routine: JoiningRelocateCandidateState {
                    has_resource_proofs: to_collect![
                        (NAME_109, (false, None)),
                        (NAME_110, (false, None)),
                        (NAME_111, (true, Some(ProofSource(1))))
                    ],
                    ..JoiningRelocateCandidateState::default()
                },
                ..AssertJoiningState::default()
            },
        );
    }

    #[test]
    fn test_joining_got_one_proof_receipt() {
        let initial_state = arrange_initial_joining_state(
            &intial_joining_state_with_dst_200().start(&DST_SECTION_INFO_200),
            &[
                Rpc::ConnectionInfoResponse {
                    source: NAME_111,
                    destination: OUR_NAME,
                    connection_info: NAME_111.0,
                }
                .to_event(),
                Rpc::ResourceProof {
                    candidate: OUR_NODE_CANDIDATE,
                    source: NAME_111,
                    proof: ProofRequest { value: NAME_111.0 },
                }
                .to_event(),
                LocalEvent::ComputeResourceProofForElder(NAME_111, ProofSource(2)).to_event(),
            ],
        );

        run_joining_test(
            "",
            &initial_state,
            &[Rpc::ResourceProofReceipt {
                candidate: OUR_NODE_CANDIDATE,
                source: NAME_111,
            }
            .to_event()],
            &AssertJoiningState {
                action_our_rpcs: vec![Rpc::ResourceProofResponse {
                    candidate: OUR_NODE_CANDIDATE,
                    destination: NAME_111,
                    proof: Proof::ValidEnd,
                }],
                join_routine: JoiningRelocateCandidateState {
                    has_resource_proofs: to_collect![
                        (NAME_109, (false, None)),
                        (NAME_110, (false, None)),
                        (NAME_111, (true, Some(ProofSource(0))))
                    ],
                    ..JoiningRelocateCandidateState::default()
                },
                ..AssertJoiningState::default()
            },
        );
    }

    #[test]
    fn test_joining_resend_timeout_after_one_proof() {
        let initial_state = arrange_initial_joining_state(
            &intial_joining_state_with_dst_200().start(&DST_SECTION_INFO_200),
            &[
                Rpc::ConnectionInfoResponse {
                    source: NAME_110,
                    destination: OUR_NAME,
                    connection_info: NAME_110.0,
                }
                .to_event(),
                Rpc::ConnectionInfoResponse {
                    source: NAME_111,
                    destination: OUR_NAME,
                    connection_info: NAME_111.0,
                }
                .to_event(),
                Rpc::ResourceProof {
                    candidate: OUR_NODE_CANDIDATE,
                    source: NAME_111,
                    proof: ProofRequest { value: NAME_111.0 },
                }
                .to_event(),
            ],
        );

        run_joining_test(
            "",
            &initial_state,
            &[LocalEvent::JoiningTimeoutResendCandidateInfo.to_event()],
            &AssertJoiningState {
                action_our_rpcs: vec![
                    Rpc::ConnectionInfoRequest {
                        source: OUR_NAME,
                        destination: NAME_109,
                        connection_info: OUR_NAME.0,
                    },
                    Rpc::ConnectionInfoRequest {
                        source: OUR_NAME,
                        destination: NAME_110,
                        connection_info: OUR_NAME.0,
                    },
                ],
                action_our_events: vec![LocalEvent::JoiningTimeoutResendCandidateInfo],
                join_routine: JoiningRelocateCandidateState {
                    has_resource_proofs: to_collect![
                        (NAME_109, (false, None)),
                        (NAME_110, (false, None)),
                        (NAME_111, (true, None))
                    ],
                    ..JoiningRelocateCandidateState::default()
                },
                ..AssertJoiningState::default()
            },
        );
    }

    #[test]
    fn test_joining_approved() {
        let initial_state = arrange_initial_joining_state(
            &intial_joining_state_with_dst_200().start(&DST_SECTION_INFO_200),
            &[],
        );

        run_joining_test(
            "",
            &initial_state,
            &[
                Rpc::NodeApproval(OUR_NODE_CANDIDATE, GenesisPfxInfo(DST_SECTION_INFO_200))
                    .to_event(),
            ],
            &AssertJoiningState {
                join_routine: JoiningRelocateCandidateState {
                    routine_complete: Some(GenesisPfxInfo(DST_SECTION_INFO_200)),
                    ..JoiningRelocateCandidateState::default()
                },
                ..AssertJoiningState::default()
            },
        );
    }
}
