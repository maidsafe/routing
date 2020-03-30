// Copyright 2019 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use super::{
    add_connected_nodes_until_one_away_from_split, create_connected_nodes_until_split,
    current_sections, nodes_with_prefix, poll_and_resend, poll_and_resend_with_options,
    verify_invariant_for_all_nodes, PollOptions, TestNode, LOWERED_ELDER_SIZE,
};
use rand::{
    distributions::{Distribution, Standard},
    seq::SliceRandom,
    Rng,
};
use routing::{
    mock::Environment, FullId, NetworkParams, Prefix, PublicId, RelocationOverrides,
    TransportConfig, XorName,
};
use std::{iter, slice};

// These params are selected such that there can be a section size which allows relocation and at the same time
// allows churn to happen which doesn't trigger split or allow churn to not increase age.
const NETWORK_PARAMS: NetworkParams = NetworkParams {
    elder_size: LOWERED_ELDER_SIZE,
    safe_section_size: LOWERED_ELDER_SIZE + 4,
};

#[test]
fn relocate_without_split() {
    let env = Environment::new(NETWORK_PARAMS);
    let mut overrides = RelocationOverrides::new();

    let mut rng = env.new_rng();
    let mut nodes = create_connected_nodes_until_split(&env, vec![1, 1]);
    verify_invariant_for_all_nodes(&env, &mut nodes);

    let prefixes: Vec<_> = current_sections(&nodes).collect();

    // `nodes[0]` is the oldest node, so it's the first one to be relocated when its age increases.
    // So let's pick the prefix it is in as the source prefix.
    let source_prefix = *find_matching_prefix(&prefixes, &nodes[0].name());
    let target_prefix = *choose_other_prefix(&mut rng, &prefixes, &source_prefix);

    let destination = target_prefix.substituted_in(rng.gen());
    overrides.set(source_prefix, destination);

    // Create enough churn events so that the age of the oldest node increases which causes it to
    // be relocated.
    let oldest_age_counter = node_age_counter(&nodes, 0);
    let num_churns = oldest_age_counter.next_power_of_two() - oldest_age_counter;
    section_churn_allowing_relocate(num_churns, &env, &mut nodes, &source_prefix);
    poll_and_resend(&mut nodes);

    assert!(
        target_prefix.matches(&nodes[0].name()),
        "Verify the Node {}, got relocated to prefix {:?}",
        nodes[0].name(),
        target_prefix
    );
}

#[test]
fn relocate_causing_split() {
    // Note: this test doesn't always trigger split in the target section. This is because when the
    // target section receives the bootstrap request from the relocating node, it still has its
    // pre-split prefix which it gives to the node. So the node then generates random name matching
    // that prefix which will fall into the split-triggering subsection only ~50% of the time.
    //
    // We might consider trying to figure a way to force the relocation into the correct
    // sub-interval, but the test is still useful as is for soak testing.

    // Relocate node into a section which is one node shy of splitting.
    let env = Environment::new(NETWORK_PARAMS);
    let mut overrides = RelocationOverrides::new();

    let mut rng = env.new_rng();
    let mut nodes = create_connected_nodes_until_split(&env, vec![1, 1]);

    let oldest_age_counter = node_age_counter(&nodes, 0);

    let prefixes: Vec<_> = current_sections(&nodes).collect();
    let source_prefix = *find_matching_prefix(&prefixes, &nodes[0].name());
    let target_prefix = *choose_other_prefix(&mut rng, &prefixes, &source_prefix);

    overrides.suppress(target_prefix);

    let trigger_prefixes = add_connected_nodes_until_one_away_from_split(
        &env,
        &mut nodes,
        slice::from_ref(&target_prefix),
    );

    let destination = trigger_prefixes[0].substituted_in(rng.gen());
    overrides.set(source_prefix, destination);

    // Trigger relocation.
    let num_churns = oldest_age_counter.next_power_of_two() - oldest_age_counter;
    section_churn_allowing_relocate(num_churns, &env, &mut nodes, &source_prefix);
    poll_and_resend(&mut nodes);

    assert!(
        target_prefix.matches(&nodes[0].name()),
        "Verify the Node {}, got relocated to prefix {:?}",
        nodes[0].name(),
        target_prefix
    );

    // Check whether the destination section split.
    let split = nodes_with_prefix(&nodes, &target_prefix)
        .all(|node| node.our_prefix().is_extension_of(&target_prefix));
    debug!(
        "The target section {:?} {} split",
        target_prefix,
        if split { "did" } else { "did not" },
    );
}

// This test is ignored because it currently fails in the following case:
// A node is relocated to the target section, successfully bootstraps and is about to send
// `JoinRequest`. At the same time, the target section splits. One half of the former section
// matches the new name of the relocated node, but does not match the relocate destination. The
// other half is the other way around. Both thus reject the `JoinRequest` and the node relocation
// fails.
// TODO: find a way to address this issue.
#[ignore]
#[test]
fn relocate_during_split() {
    // Relocate node into a section which is undergoing split.
    let env = Environment::new(NETWORK_PARAMS);
    let mut overrides = RelocationOverrides::new();

    let mut rng = env.new_rng();
    let mut nodes = create_connected_nodes_until_split(&env, vec![1, 1]);
    let oldest_age_counter = node_age_counter(&nodes, 0);

    let prefixes: Vec<_> = current_sections(&nodes).collect();
    let source_prefix = *unwrap!(prefixes.choose(&mut rng));
    let target_prefix = *choose_other_prefix(&mut rng, &prefixes, &source_prefix);

    let _ = add_connected_nodes_until_one_away_from_split(
        &env,
        &mut nodes,
        slice::from_ref(&target_prefix),
    );

    let destination = target_prefix.substituted_in(rng.gen());
    overrides.set(source_prefix, destination);

    // Create churn so we are one churn away from relocation.
    let num_churns = oldest_age_counter.next_power_of_two() - oldest_age_counter - 1;
    section_churn_allowing_relocate(num_churns, &env, &mut nodes, &source_prefix);

    // Add new node, but do not poll yet.
    add_node_to_prefix(&env, &mut nodes, &target_prefix);

    // One more churn to trigger the relocation.
    section_churn_allowing_relocate(1, &env, &mut nodes, &source_prefix);

    // Poll now, so the add and the relocation happen simultaneously.
    poll_and_resend_with_options(
        &mut nodes,
        PollOptions::default()
            .continue_if(move |nodes| !node_relocated(nodes, 0, &source_prefix, &target_prefix)),
    )
}

// Age counter of the node at the given index.
fn node_age_counter(nodes: &[TestNode], index: usize) -> usize {
    let name = nodes[index].name();
    let mut values: Vec<_> = nodes
        .iter()
        .filter_map(|node| node.inner.member_age_counter(&name))
        .collect();
    values.sort();
    values.dedup();

    match values.len() {
        1 => values[0] as usize,
        0 => panic!("{} is not a member known to any node.", name),
        _ => panic!("Not all nodes agree on the age counter value of {}.", name),
    }
}

fn find_matching_prefix<'a>(
    prefixes: &'a [Prefix<XorName>],
    name: &XorName,
) -> &'a Prefix<XorName> {
    unwrap!(prefixes.iter().find(|prefix| prefix.matches(name)))
}

fn choose_other_prefix<'a, R: Rng>(
    rng: &mut R,
    prefixes: &'a [Prefix<XorName>],
    except: &Prefix<XorName>,
) -> &'a Prefix<XorName> {
    assert!(prefixes.iter().any(|prefix| prefix != except));

    unwrap!(iter::repeat(())
        .filter_map(|_| prefixes.choose(rng))
        .find(|prefix| *prefix != except))
}

fn add_node_to_prefix(env: &Environment, nodes: &mut Vec<TestNode>, prefix: &Prefix<XorName>) {
    let mut rng = env.new_rng();

    let bootstrap_index = unwrap!(iter::repeat(())
        .map(|_| rng.gen_range(0, nodes.len()))
        .find(|index| nodes[*index].inner.is_elder()));

    let config = TransportConfig::node().with_hard_coded_contact(nodes[bootstrap_index].endpoint());
    let full_id = FullId::within_range(&mut rng, &prefix.range_inclusive());
    nodes.push(
        TestNode::builder(env)
            .transport_config(config)
            .full_id(full_id)
            .create(),
    )
}

fn remove_node_from_prefix(nodes: &mut Vec<TestNode>, prefix: &Prefix<XorName>) -> TestNode {
    // Lookup from the end, so we remove the youngest node in the section.
    let index = nodes.len()
        - unwrap!(nodes
            .iter()
            .rev()
            .position(|node| prefix.matches(&node.name())))
        - 1;
    nodes.remove(index)
}

// Make the given section churn the given number of times, while maintaining the section size in
// the interval that allow demoting/relocating a node at each step.
fn section_churn_allowing_relocate(
    count: usize,
    env: &Environment,
    nodes: &mut Vec<TestNode>,
    prefix: &Prefix<XorName>,
) {
    // Keep the section size such that relocations can happen but splits can't.
    // We need NETWORK_PARAMS.elder_size + 1 excluding relocating node for it to be demoted.
    let min_size = (NETWORK_PARAMS.elder_size + 1) + 1;

    // Ensure we are increasing age at each churn event.
    let max_size = NETWORK_PARAMS.safe_section_size - 1;

    section_churn(count, &env, nodes, &prefix, min_size, max_size)
}

// Make the given section churn the given number of times, while maintaining the section size in
// the given interval.
fn section_churn(
    count: usize,
    env: &Environment,
    nodes: &mut Vec<TestNode>,
    prefix: &Prefix<XorName>,
    min_section_size: usize,
    max_section_size: usize,
) {
    info!(
        "Start section_churn for num churn: {}, prefix {:?}",
        count, prefix
    );

    assert!(min_section_size < max_section_size);

    let mut rng = env.new_rng();

    for _ in 0..count {
        let section_size = nodes_with_prefix(nodes, prefix).count();
        let churn = if section_size <= min_section_size {
            Churn::Add
        } else if section_size >= max_section_size {
            Churn::Remove
        } else {
            rng.gen()
        };

        info!("section_churn churn: {:?}", churn);
        match churn {
            Churn::Add => {
                add_node_to_prefix(env, nodes, prefix);
                poll_and_resend_with_options(
                    nodes,
                    PollOptions::default()
                        .continue_if(|nodes| !node_joined(nodes, nodes.len() - 1)),
                );
            }
            Churn::Remove => {
                let id = remove_node_from_prefix(nodes, prefix).id();
                poll_and_resend_with_options(
                    nodes,
                    PollOptions::default().continue_if(move |nodes| !node_left(nodes, &id)),
                );
            }
        }
    }

    info!("section_churn complete");
}

// Returns whether all nodes from its section recognize the node at the given index as joined.
fn node_joined(nodes: &[TestNode], node_index: usize) -> bool {
    let id = nodes[node_index].id();

    nodes
        .iter()
        .filter(|node| node.inner.is_elder())
        .filter(|node| {
            node.inner
                .our_prefix()
                .map(|prefix| prefix.matches(id.name()))
                .unwrap_or(false)
        })
        .all(|node| node.inner.is_peer_our_member(&id))
}

// Returns whether all nodes recognize the node with the given id as left.
fn node_left(nodes: &[TestNode], id: &PublicId) -> bool {
    nodes
        .iter()
        .filter(|node| node.inner.is_elder())
        .all(|node| !node.inner.is_peer_our_member(id))
}

// Returns whether the relocation of node at `node_index` from `source_prefix` to `target_prefix`
// is complete.
fn node_relocated(
    nodes: &[TestNode],
    node_index: usize,
    source_prefix: &Prefix<XorName>,
    target_prefix: &Prefix<XorName>,
) -> bool {
    let node_name = nodes[node_index].name();
    for node in nodes {
        let prefixes = node.inner.prefixes();

        let in_source = prefixes
            .iter()
            .filter(|prefix| prefix.is_compatible(source_prefix))
            .any(|prefix| {
                // TODO: check all members, not just elders.
                node.inner.section_elders(prefix).contains(&node_name)
            });
        if in_source {
            return false;
        }

        let in_target = prefixes
            .iter()
            .filter(|prefix| prefix.is_compatible(target_prefix))
            .any(|prefix| {
                // TODO: check all members, not just elders.
                node.inner.section_elders(prefix).contains(&node_name)
            });
        if !in_target {
            return false;
        }
    }

    true
}

#[derive(Debug, Eq, PartialEq)]
enum Churn {
    Add,
    Remove,
}

impl Distribution<Churn> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Churn {
        if rng.gen() {
            Churn::Add
        } else {
            Churn::Remove
        }
    }
}
