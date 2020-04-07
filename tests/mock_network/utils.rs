// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use crossbeam_channel as mpmc;
use fake_clock::FakeClock;
use itertools::Itertools;
use rand::{
    distributions::{Distribution, Standard},
    Rng,
};
use routing::{
    event::{Connected, Event},
    mock::Environment,
    rng::MainRng,
    test_consts, DstLocation, FullId, Node, NodeConfig, PausedState, Prefix, PublicId,
    RelocationOverrides, SrcLocation, TransportConfig, XorName, Xorable,
};
use std::{
    cmp, collections::BTreeSet, convert::TryInto, iter, net::SocketAddr, ops::Range, time::Duration,
};

// Maximum number of times to try and poll in a loop.  This is several orders higher than the
// anticipated upper limit for any test, and if hit is likely to indicate an infinite loop.
const MAX_POLL_CALLS: usize = 2000;

// -----  Random number generation  -----

pub fn gen_range<T: Rng>(rng: &mut T, low: usize, high: usize) -> usize {
    rng.gen_range(low as u32, high as u32) as usize
}

pub fn gen_elder_index<R: Rng>(rng: &mut R, nodes: &[TestNode]) -> usize {
    loop {
        let index = gen_range(rng, 0, nodes.len());
        if nodes[index].inner.is_elder() {
            break index;
        }
    }
}

// -----  TestNode and builder  -----

pub struct TestNode {
    pub inner: Node,
    env: Environment,
    user_event_rx: mpmc::Receiver<Event>,
}

impl TestNode {
    pub fn builder(env: &Environment) -> TestNodeBuilder {
        TestNodeBuilder {
            config: NodeConfig::default(),
            env,
        }
    }

    pub fn resume(env: &Environment, state: PausedState) -> Self {
        let (inner, user_event_rx) = Node::resume(state);
        Self {
            inner,
            env: env.clone(),
            user_event_rx,
        }
    }

    pub fn endpoint(&mut self) -> SocketAddr {
        unwrap!(self.inner.our_connection_info(), "{}", self.name())
    }

    pub fn id(&self) -> &PublicId {
        self.inner.id()
    }

    pub fn name(&self) -> &XorName {
        self.inner.name()
    }

    pub fn close_names(&self) -> Vec<XorName> {
        unwrap!(self.inner.close_names(&self.name()), "{}", self.name())
    }

    pub fn our_prefix(&self) -> &Prefix<XorName> {
        unwrap!(self.inner.our_prefix(), "{}", self.name())
    }

    pub fn in_src_location(&self, src: &SrcLocation) -> bool {
        self.inner.in_src_location(src)
    }

    pub fn in_dst_location(&self, dst: &DstLocation) -> bool {
        self.inner.in_dst_location(dst)
    }

    pub fn poll(&mut self) -> bool {
        let mut result = false;

        // Exhaust all the events/actions from the channels but return true only if at least one of
        // those events/actions are considered as handled (that is there is at least one
        // non-timeout).
        loop {
            let mut sel = mpmc::Select::new();
            self.inner.register(&mut sel);

            if let Ok(op_index) = sel.try_ready() {
                if self
                    .inner
                    .handle_selected_operation(op_index)
                    .unwrap_or(false)
                {
                    result = true;
                }
            } else {
                break;
            }
        }

        result
    }

    pub fn try_recv_event(&self) -> Option<Event> {
        self.user_event_rx.try_recv().ok()
    }
}

pub fn count_sections(nodes: &[TestNode]) -> usize {
    current_sections(nodes).count()
}

pub fn current_sections<'a>(nodes: &'a [TestNode]) -> impl Iterator<Item = Prefix<XorName>> + 'a {
    nodes.iter().flat_map(|n| n.inner.prefixes()).unique()
}

pub struct TestNodeBuilder<'a> {
    config: NodeConfig,
    env: &'a Environment,
}

impl<'a> TestNodeBuilder<'a> {
    pub fn first(mut self) -> Self {
        self.config.first = true;
        self
    }

    pub fn transport_config(mut self, config: TransportConfig) -> Self {
        self.config.transport_config = config;
        self
    }

    pub fn full_id(mut self, full_id: FullId) -> Self {
        self.config.full_id = Some(full_id);
        self
    }

    pub fn create(mut self) -> TestNode {
        self.config.network_params = self.env.network_params();
        self.config.rng = self.env.new_rng();

        let (inner, user_event_rx, _client_rx) = Node::new(self.config);

        TestNode {
            inner,
            env: self.env.clone(),
            user_event_rx,
        }
    }
}

// -----  poll_all, create_connected_...  -----

/// Process all events. Returns whether there were any events.
pub fn poll_all(env: &Environment, nodes: &mut [TestNode]) -> bool {
    let mut result = false;

    for _ in 0..MAX_POLL_CALLS {
        env.poll();

        let mut handled_message = false;

        for node in nodes.iter_mut() {
            handled_message = node.poll() || handled_message;
        }

        if !handled_message {
            return result;
        }

        result = true;
    }

    panic!("poll_all has been called {} times.", MAX_POLL_CALLS);
}

/// Polls the network until the given predicate returns `true`.
pub fn poll_until<F>(env: &Environment, nodes: &mut [TestNode], mut predicate: F)
where
    F: FnMut(&[TestNode]) -> bool,
{
    // Duration to advance the time after each iteration.
    let time_step = test_consts::GOSSIP_PERIOD + Duration::from_millis(1);

    for _ in 0..MAX_POLL_CALLS {
        if poll_all(env, nodes) {
            advance_time(time_step);
            continue;
        }

        if !predicate(nodes) {
            advance_time(time_step);
            continue;
        }

        return;
    }

    panic!("poll_until has been called {} times.", MAX_POLL_CALLS);
}

/// Polls and processes all events, until there are no unacknowledged messages left.
pub fn poll_and_resend(nodes: &mut [TestNode]) {
    let env = nodes[0].env.clone();

    let node_busy = |node: &TestNode| node.inner.has_unpolled_observations();

    // When all nodes become idle, run a couple more iterations, advancing the time a bit after
    // each one. This should allow the nodes to process failed or bounced messages.
    let max_final_iterations = 19;
    let mut final_iterations = 0;

    poll_until(&env, nodes, |nodes| {
        if nodes.iter().any(node_busy) {
            return false;
        }

        if final_iterations < max_final_iterations {
            final_iterations += 1;
            return false;
        }

        true
    })
}

fn advance_time(duration: Duration) {
    FakeClock::advance_time(duration.as_millis().try_into().expect("time step too long"));
}

// Returns whether all nodes from its section recognize the node at the given index as joined.
pub fn node_joined(nodes: &[TestNode], index: usize) -> bool {
    if !nodes[index].inner.is_approved() {
        trace!(
            "Node {} is not yet member according to itself",
            nodes[index].name()
        );
        return false;
    }

    let id = nodes[index].id();

    nodes
        .iter()
        .filter(|node| node.inner.is_elder())
        .filter(|node| {
            node.inner
                .our_prefix()
                .map(|prefix| prefix.matches(id.name()))
                .unwrap_or(false)
        })
        .all(|node| {
            if node.inner.is_peer_our_member(&id) {
                true
            } else {
                trace!(
                    "Node {} is not yet member according to {}",
                    id.name(),
                    node.name()
                );
                false
            }
        })
}

pub fn all_nodes_joined(nodes: &[TestNode], indices: impl IntoIterator<Item = usize>) -> bool {
    indices.into_iter().all(|index| node_joined(nodes, index))
}

// Returns whether all nodes recognize the node with the given id as left.
pub fn node_left(nodes: &[TestNode], id: &PublicId) -> bool {
    nodes
        .iter()
        .filter(|node| node.inner.is_elder())
        .all(|node| {
            // Note: need both checks because even if a node has been consensused as offline, it
            // can still be considered as elder until the new `SectionInfo`.
            if node.inner.is_peer_our_member(id) {
                trace!("Node {} is still member according to {}", id, node.name());
                return false;
            }

            if node.inner.is_peer_elder(id) {
                trace!("Node {} is still elder according to {}", id, node.name());
                return false;
            }

            true
        })
}

// Returns whether the section with the given prefix did split.
pub fn section_split(nodes: &[TestNode], prefix: &Prefix<XorName>) -> bool {
    let sub_prefix0 = prefix.pushed(false);
    let sub_prefix1 = prefix.pushed(true);

    let mut pending = nodes
        .iter()
        .filter(|node| {
            if sub_prefix0.matches(node.name()) && *node.our_prefix() != sub_prefix0 {
                return true;
            }

            if sub_prefix1.matches(node.name()) && *node.our_prefix() != sub_prefix1 {
                return true;
            }

            if node.inner.prefixes().contains(prefix) {
                return true;
            }

            false
        })
        .map(|node| node.name())
        .peekable();

    if pending.peek().is_none() {
        true
    } else {
        debug!("Pending split: {}", pending.format(", "));
        false
    }
}

pub fn create_connected_nodes(env: &Environment, size: usize) -> Vec<TestNode> {
    let mut nodes = Vec::new();

    // Create the seed node.
    nodes.push(TestNode::builder(env).first().create());
    let _ = nodes[0].poll();
    let endpoint = nodes[0].endpoint();
    info!("Seed node: {}", nodes[0].name());

    // Create other nodes using the seed node endpoint as bootstrap contact.
    for _ in 1..size {
        let config = TransportConfig::node().with_hard_coded_contact(endpoint);
        nodes.push(TestNode::builder(env).transport_config(config).create());

        poll_until(env, &mut nodes, |nodes| node_joined(nodes, nodes.len() - 1));
        verify_invariants_for_nodes(&env, &nodes);
    }

    for node in &mut nodes {
        expect_next_event!(node, Event::Connected(Connected::First));

        while let Some(event) = node.try_recv_event() {
            match event {
                Event::SectionSplit(..)
                | Event::RestartRequired
                | Event::Connected(Connected::Relocate)
                | Event::Promoted
                | Event::Demoted => (),
                event => panic!("Got unexpected event: {:?}", event),
            }
        }
    }

    nodes
}

pub fn create_connected_nodes_until_split(
    env: &Environment,
    prefix_lengths: &[usize],
) -> Vec<TestNode> {
    let mut rng = env.new_rng();

    // The prefixes we want to create.
    let final_prefixes = gen_prefixes(&mut rng, prefix_lengths);

    // The sequence of prefixes to split in order to reach `final_prefixes`.
    let mut split_sequence = final_prefixes
        .iter()
        .flat_map(|prefix| prefix.ancestors())
        .sorted_by(|lhs, rhs| lhs.cmp_breadth_first(rhs));
    split_sequence.dedup();

    let mut nodes = Vec::new();

    for prefix_to_split in split_sequence {
        trigger_split(env, &mut nodes, &prefix_to_split)
    }

    // Gather all the actual prefixes and check they are as expected.
    let actual_prefixes: BTreeSet<_> = nodes
        .iter()
        .flat_map(|node| node.inner.prefixes())
        .collect();
    assert_eq!(actual_prefixes, final_prefixes.iter().copied().collect());

    let actual_prefix_lengths: Vec<_> = actual_prefixes.iter().map(Prefix::bit_count).sorted();
    assert_eq!(&actual_prefix_lengths[..], prefix_lengths);

    trace!("Created testnet comprising {:?}", actual_prefixes);

    nodes
}

// Add connected nodes to the given prefix until adding one extra node into the
// returned sub-prefix would trigger a split.
pub fn add_connected_nodes_until_one_away_from_split(
    env: &Environment,
    nodes: &mut Vec<TestNode>,
    prefix_to_nearly_split: &Prefix<XorName>,
) -> Prefix<XorName> {
    let sub_prefix_last_bit = env.new_rng().gen();
    let sub_prefix = prefix_to_nearly_split.pushed(sub_prefix_last_bit);
    let (count0, count1) = if sub_prefix_last_bit {
        (env.safe_section_size(), env.safe_section_size() - 1)
    } else {
        (env.safe_section_size() - 1, env.safe_section_size())
    };

    add_mature_nodes(env, nodes, prefix_to_nearly_split, count0, count1);

    sub_prefix
}

/// Split the section by adding and/or removing nodes to/from it.
pub fn trigger_split(env: &Environment, nodes: &mut Vec<TestNode>, prefix: &Prefix<XorName>) {
    // To trigger split, we need the section to contain at least `safe_section_size` *mature* nodes
    // from each sub-prefix.
    add_mature_nodes(
        env,
        nodes,
        prefix,
        env.safe_section_size(),
        env.safe_section_size(),
    );

    // Verify the split actually happened.
    poll_until(env, nodes, |nodes| section_split(nodes, prefix));
    info!("Split finished");
}

/// Add/remove nodes to the given section until it has exactly `count0` mature nodes from the
/// 0-ending subprefix and `count1` mature nodes from the 1-ending subprefix.
/// Note: if `count0` and `count1` are both at least `safe_section_size`, this causes the section
/// to split.
pub fn add_mature_nodes(
    env: &Environment,
    nodes: &mut Vec<TestNode>,
    prefix: &Prefix<XorName>,
    count0: usize,
    count1: usize,
) {
    // New nodes start as infants at age counter 16. We need to increase their age counters to 32
    // in order for them to become adults. To do that, we need to add or remove 16 other mature
    // nodes. Adding mature node can be done only by relocating it from another section, so for
    // simplicity we will be only removing nodes here.

    // TODO: consider using relocations from other sections (if there are any) too.

    assert!(
        env.elder_size() > 3,
        "elder_size is {} which is less than 4 - the minimum needed to reach consensus on elder removal",
        env.elder_size()
    );

    let sub_prefix0 = prefix.pushed(false);
    let sub_prefix1 = prefix.pushed(true);

    // Number of times to increment the age counters so all nodes are mature. That is, the number
    // of mature nodes to remove.
    let remove_count = 16;

    // Count already existing nodes in the prefix.
    let current_count = nodes_with_prefix(nodes, &prefix).count();
    assert!(
        current_count <= remove_count,
        "section must have less than {} nodes (has {}) in order to trigger split (this is a \
         test-only limitation)",
        remove_count,
        current_count,
    );

    let mut rng = env.new_rng();

    let mut overrides = RelocationOverrides::new();
    overrides.suppress(*prefix);

    // The order the nodes are added in is important because it influences which nodes will be
    // promoted to replace previously removed elders and thus themselves being removed too. We want
    // to first add the nodes that will be removed and then the nodes that will remain.

    // First add the nodes that will be removed later. These can go into any sub-prefix.
    let temp_count = remove_count.saturating_sub(current_count);
    let first_index = nodes.len();
    info!("Adding {} temporary nodes", temp_count);
    for _ in 0..temp_count {
        add_node_to_section(env, nodes, &prefix);
    }

    poll_until(env, nodes, |nodes| {
        all_nodes_joined(nodes, first_index..nodes.len())
    });

    // Of the remaining nodes, `count0` goes to the 0-ending sub-prefix and `count` to the
    // 1-ending. Add them in random order to avoid accidentally relying on them being in any
    // particular order.
    info!("Adding {} final nodes", count0 + count1);
    let mut remaining0 = count0;
    let mut remaining1 = count1;
    let first_index = nodes.len();

    loop {
        let bit = if remaining0 > 0 && remaining1 > 0 {
            rng.gen()
        } else if remaining1 > 0 {
            true
        } else if remaining0 > 0 {
            false
        } else {
            break;
        };

        if bit {
            add_node_to_section(env, nodes, &sub_prefix1);
            remaining1 -= 1;
        } else {
            add_node_to_section(env, nodes, &sub_prefix0);
            remaining0 -= 1;
        }
    }

    poll_until(env, nodes, |nodes| {
        all_nodes_joined(nodes, first_index..nodes.len())
    });

    // Remove 16 mature nodes to trigger 16 age increments.
    info!("Removing {} mature nodes", remove_count);
    for _ in 0..remove_count {
        // Note: removing only elders for simplicity. Also making sure we don't remove any of the
        // last `count0 + count1` nodes.
        let removed_id =
            remove_elder_from_section_in_range(nodes, &prefix, 0..nodes.len() - count0 - count1);
        poll_until(env, nodes, |nodes| node_left(nodes, &removed_id));
    }

    // Count the number of nodes in each sub-prefix and verify they are as expected.
    let actual_count0 = nodes_with_prefix(nodes, &sub_prefix0).count();
    assert_eq!(actual_count0, count0);

    let actual_count1 = nodes_with_prefix(nodes, &sub_prefix1).count();
    assert_eq!(actual_count1, count1);
}

// -----  Small misc functions  -----

/// Sorts the given nodes by their distance to `name`. Note that this will call the `name()`
/// function on them which causes polling, so it calls `poll_all` to make sure that all other
/// events have been processed before sorting.
pub fn sort_nodes_by_distance_to(env: &Environment, nodes: &mut [TestNode], name: &XorName) {
    let _ = poll_all(env, nodes); // Poll
    nodes.sort_by(|node0, node1| name.cmp_distance(node0.name(), node1.name()));
}

/// Iterator over all nodes that belong to the given prefix.
pub fn nodes_with_prefix<'a>(
    nodes: &'a [TestNode],
    prefix: &'a Prefix<XorName>,
) -> impl Iterator<Item = &'a TestNode> {
    nodes.iter().filter(move |node| prefix.matches(node.name()))
}

/// Mutable iterator over all nodes that belong to the given prefix.
pub fn nodes_with_prefix_mut<'a>(
    nodes: &'a mut [TestNode],
    prefix: &'a Prefix<XorName>,
) -> impl Iterator<Item = &'a mut TestNode> {
    nodes
        .iter_mut()
        .filter(move |node| prefix.matches(node.name()))
}

/// Iterator over all nodes that belong to the given prefix + their indices
pub fn indexed_nodes_with_prefix<'a>(
    nodes: &'a [TestNode],
    prefix: &'a Prefix<XorName>,
) -> impl Iterator<Item = (usize, &'a TestNode)> {
    nodes
        .iter()
        .enumerate()
        .filter(move |(_, node)| prefix.matches(node.name()))
}

pub fn verify_invariants_for_node(env: &Environment, node: &TestNode) {
    let our_prefix = node.our_prefix();
    let our_name = node.name();
    let our_section_elders = node.inner.section_elders(our_prefix);

    assert!(
        our_prefix.matches(our_name),
        "{} Our prefix doesn't match our name: {:?}, {:?}",
        node.name(),
        our_prefix,
        our_name,
    );

    if !our_prefix.is_empty() {
        assert!(
            our_section_elders.len() >= env.elder_size(),
            "{} Our section {:?} is below the minimum size!",
            node.name(),
            our_prefix,
        );
    }

    if let Some(name) = our_section_elders
        .iter()
        .find(|name| !our_prefix.matches(name))
    {
        panic!(
            "{} A name in our section doesn't match its prefix! {:?}, {:?}",
            node.name(),
            name,
            our_prefix,
        );
    }

    let neighbour_prefixes = node.inner.neighbour_prefixes();
    if !node.inner.is_elder() {
        assert!(
            neighbour_prefixes.is_empty(),
            "{} is not elder so should not have neighbour infos, but has: {:?}",
            node.name(),
            neighbour_prefixes,
        );
        return;
    }

    if let Some(compatible_prefix) = neighbour_prefixes
        .iter()
        .find(|prefix| prefix.is_compatible(our_prefix))
    {
        panic!(
            "{} Our prefix is compatible with one of the neighbour prefixes:us: {:?} / neighbour: \
             {:?}, neighbour_prefixes: {:?}",
            node.name(),
            our_prefix,
            compatible_prefix,
            neighbour_prefixes,
        );
    }

    if let Some(prefix) = neighbour_prefixes
        .iter()
        .find(|prefix| node.inner.section_elders(prefix).len() < env.elder_size())
    {
        panic!(
            "{} A section is below the minimum size: size({:?}) = {}; For ({:?}: {:?}), \
             neighbour_prefixes: {:?}",
            node.name(),
            prefix,
            node.inner.section_elders(prefix).len(),
            our_name,
            our_prefix,
            neighbour_prefixes,
        );
    }

    for prefix in &neighbour_prefixes {
        if let Some(name) = node
            .inner
            .section_elders(prefix)
            .iter()
            .find(|name| !prefix.matches(name))
        {
            panic!(
                "{} A name in a section doesn't match its prefix! {:?}, {:?}",
                node.name(),
                name,
                prefix,
            );
        }
    }

    let all_are_neighbours = node
        .inner
        .neighbour_prefixes()
        .iter()
        .all(|prefix| our_prefix.is_neighbour(prefix));
    if !all_are_neighbours {
        panic!(
            "{} Some sections in the chain aren't neighbours of our section: {:?}",
            node.name(),
            iter::once(*our_prefix)
                .chain(neighbour_prefixes)
                .collect::<Vec<_>>()
        );
    }

    let all_neighbours_covered = {
        (0..our_prefix.bit_count()).all(|i| {
            our_prefix
                .with_flipped_bit(i)
                .is_covered_by(&neighbour_prefixes)
        })
    };
    if !all_neighbours_covered {
        panic!(
            "{} Some neighbours aren't fully covered by the chain: {:?}",
            node.name(),
            iter::once(*our_prefix)
                .chain(neighbour_prefixes)
                .collect::<Vec<_>>()
        );
    }
}

pub fn verify_invariants_for_nodes(env: &Environment, nodes: &[TestNode]) {
    for node in nodes {
        verify_invariants_for_node(env, node);
    }
}

// Generate a vector of random T of the given length.
pub fn gen_vec<R: Rng, T>(rng: &mut R, size: usize) -> Vec<T>
where
    Standard: Distribution<T>,
{
    rng.sample_iter(&Standard).take(size).collect()
}

// Generate a vector of random bytes of the given length.
pub fn gen_bytes<R: Rng>(rng: &mut R, size: usize) -> Vec<u8> {
    gen_vec(rng, size)
}

// Create new node in the given section.
pub fn add_node_to_section(env: &Environment, nodes: &mut Vec<TestNode>, prefix: &Prefix<XorName>) {
    let mut rng = env.new_rng();
    let full_id = FullId::within_range(&mut rng, &prefix.range_inclusive());

    let node = if nodes.is_empty() {
        TestNode::builder(env).first().full_id(full_id).create()
    } else {
        let config = TransportConfig::node().with_hard_coded_contact(nodes[0].endpoint());
        TestNode::builder(env)
            .transport_config(config)
            .full_id(full_id)
            .create()
    };

    info!("Add node {} to {:?}", node.name(), prefix);
    nodes.push(node);
}

// Removes one elder node from the given prefix but only from nodes in the given index range.
// Returns the id of the removed node.
fn remove_elder_from_section_in_range(
    nodes: &mut Vec<TestNode>,
    prefix: &Prefix<XorName>,
    index_range: Range<usize>,
) -> PublicId {
    let index = indexed_nodes_with_prefix(&nodes[index_range], prefix)
        .find(|(_, node)| node.inner.is_elder())
        .map(|(index, _)| index)
        .unwrap();

    info!("Remove node {} from {:?}", nodes[index].name(), prefix);
    *nodes.remove(index).id()
}

// Generate random prefixes with the given lengths.
fn gen_prefixes(rng: &mut MainRng, prefix_lengths: &[usize]) -> Vec<Prefix<XorName>> {
    validate_prefix_lenghts(&prefix_lengths);

    let _ = prefix_lengths.iter().fold(0, |previous, &current| {
        assert!(
            previous <= current,
            "Slice {:?} should be sorted.",
            prefix_lengths
        );
        current
    });

    let mut prefixes = vec![Prefix::new(prefix_lengths[0], rng.gen())];
    while prefixes.len() < prefix_lengths.len() {
        let new_prefix = Prefix::new(prefix_lengths[prefixes.len()], rng.gen());
        if prefixes
            .iter()
            .all(|prefix| !prefix.is_compatible(&new_prefix))
        {
            prefixes.push(new_prefix);
        }
    }
    prefixes
}

// Validate the prefixes generated with the given lengths. That is:
// - there are at least two prefixes
// - no prefix is longer than 8 bits
// - the prefixes cover the whole xor-name space
// - the prefixes don't overlap
fn validate_prefix_lenghts(prefix_lengths: &[usize]) {
    assert!(
        prefix_lengths.len() > 1,
        "There should be at least two specified prefix lengths"
    );
    let sum = prefix_lengths.iter().fold(0, |accumulated, &bit_count| {
        assert!(
            bit_count <= 8,
            "The specified prefix lengths {:?} must each be no more than 8",
            prefix_lengths
        );
        accumulated + (1 << (8 - bit_count))
    });

    match sum.cmp(&256) {
        cmp::Ordering::Less => {
            panic!(
                "The specified prefix lengths {:?} would not cover the entire address space",
                prefix_lengths
            );
        }
        cmp::Ordering::Greater => {
            panic!(
                "The specified prefix lengths {:?} would require overlapping sections",
                prefix_lengths
            );
        }
        cmp::Ordering::Equal => (),
    }
}

mod tests {
    use super::*;

    #[test]
    fn validate_prefix_lenghts_valid() {
        validate_prefix_lenghts(&[1, 1]);
        validate_prefix_lenghts(&[1, 2, 3, 4, 5, 6, 7, 8, 8]);
        validate_prefix_lenghts(&[8; 256]);
    }

    #[test]
    #[should_panic(expected = "There should be at least two specified prefix lengths")]
    fn validate_prefix_lenghts_no_split() {
        validate_prefix_lenghts(&[0]);
    }

    #[test]
    #[should_panic(expected = "would require overlapping sections")]
    fn validate_prefix_lenghts_overlapping_sections() {
        validate_prefix_lenghts(&[1, 2, 2, 2]);
    }

    #[test]
    #[should_panic(expected = "would not cover the entire address space")]
    fn validate_prefix_lenghts_missing_sections() {
        validate_prefix_lenghts(&[1, 2]);
    }

    #[test]
    #[should_panic(expected = "must each be no more than 8")]
    fn validate_prefix_lenghts_too_many_sections() {
        validate_prefix_lenghts(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 9]);
    }
}
