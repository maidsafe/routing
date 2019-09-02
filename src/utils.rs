// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use crate::{routing_table::Xorable, xor_name::XorName, Prefix};
use itertools::Itertools;
use tiny_keccak::sha3_256;
use std::{
    collections::BTreeSet,
    fmt::{self, Display, Formatter},
    iter,
    ops::RangeInclusive,
    time::Duration,
};

/// Display a "number" to the given number of decimal places
pub trait DisplayDuration {
    /// Construct a formattable object
    fn display_secs(&self) -> DisplayDurObj;
}

impl DisplayDuration for Duration {
    fn display_secs(&self) -> DisplayDurObj {
        DisplayDurObj { dur: *self }
    }
}

/// Display a number to the given number of decimal places
pub struct DisplayDurObj {
    dur: Duration,
}

impl Display for DisplayDurObj {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut secs = self.dur.as_secs();
        if self.dur.subsec_nanos() >= 500_000_000 {
            secs += 1;
        }
        write!(f, "{} seconds", secs)
    }
}

/// Identified or node/client for logging purposes.
#[derive(Clone)]
pub struct LogIdent(String);

impl LogIdent {
    pub fn new<T: Display + ?Sized>(node: &T) -> Self {
        LogIdent(format!("{}", node))
    }
}

impl Display for LogIdent {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Target Xor interval
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub struct XorTargetInterval(pub XorName, pub XorName);

impl XorTargetInterval {
    /// Create a XorTargetInterval from the equivalent RangeInclusive
    pub fn new(range: RangeInclusive<XorName>) -> Self {
        let (start, end) = range.into_inner();
        Self(start, end)
    }

    /// check if the inclusive range contains the value
    pub fn contains(&self, value: &XorName) -> bool {
        RangeInclusive::new(self.0, self.1).contains(value)
    }
}

impl Into<RangeInclusive<XorName>> for XorTargetInterval {
    fn into(self) -> RangeInclusive<XorName> {
        RangeInclusive::new(self.0, self.1)
    }
}

/// Compute the target destination for a joining node with the given name.
///
/// This is used by each member of a joining node's section to choose a location for the node to
/// move to. On the one hand, sufficiently many of them need to agree on the new name to reach
/// quorum size, on the other hand, the joining node shall not be able to predict it so that it
/// cannot choose where to be relocated to.
///
/// To meet these requirements, the target is computed from the two closest nodes and the joining
/// node's current name: It is the SHA3 hash of:
///
/// [`current_name`, 1st closest node id, 2nd closest node id]
///
/// In the case where only one close node is provided (in initial network setup scenario):
///
/// [`current_name`, 1st closest node id]
pub fn calculate_relocation_dst(mut close_nodes: Vec<XorName>, current_name: &XorName) -> XorName {
    close_nodes.sort_by(|a, b| current_name.cmp_distance(a, b));
    let combined: Vec<u8> = iter::once(current_name)
        .chain(close_nodes.iter().take(2))
        .flat_map(|close_node| close_node.0.iter())
        .cloned()
        .collect();
    XorName(sha3_256(&combined))
}

/// Calculate the interval for a node joining our section to generate a key for.
pub fn calculate_relocation_interval(
    prefix: &Prefix<XorName>,
    section: &BTreeSet<XorName>,
) -> XorTargetInterval {
    let (lower_bound, upper_bound) = (prefix.lower_bound(), prefix.upper_bound());

    let (start, end) = iter::once(&lower_bound)
        .chain(section)
        .chain(iter::once(&upper_bound))
        .tuple_windows()
        .max_by(|&(x1, y1), &(x2, y2)| {
            let diff1 = y1 - x1;
            let diff2 = y2 - x2;
            diff1.cmp(&diff2)
        })
        .unwrap_or((&lower_bound, &upper_bound));

    let third_of_distance = (*end - *start) / 3;
    let new_end = *end - third_of_distance;
    XorTargetInterval(new_end - third_of_distance, new_end)
}

#[cfg(any(test, feature = "mock_base"))]
pub fn rand_index(exclusive_max: usize) -> usize {
    use maidsafe_utilities::SeededRng;
    use rand::Rng;

    let mut rng = SeededRng::thread_rng();
    rng.gen::<usize>() % exclusive_max
}

#[cfg(all(not(test), not(feature = "mock_base")))]
pub fn rand_index(exclusive_max: usize) -> usize {
    ::rand::random::<usize>() % exclusive_max
}

#[cfg(test)]
mod tests {
    use super::DisplayDuration;
    use crate::routing_table::Xorable;
    use crate::xor_name::XorName;
    use rand;
    use tiny_keccak::sha3_256;
    use std::time::Duration;

    #[test]
    fn duration_formatting() {
        assert_eq!(
            format!("{}", Duration::new(653_105, 499_000_000).display_secs()),
            "653105 seconds"
        );
        assert_eq!(
            format!("{}", Duration::new(653_105, 500_000_000).display_secs()),
            "653106 seconds"
        );
        assert_eq!(
            format!("{}", Duration::new(0, 900_000_000).display_secs()),
            "1 seconds"
        );
    }

    #[test]
    fn calculate_relocation_dst() {
        let original_name: XorName = rand::random();

        // one entry
        let mut close_nodes_one_entry: Vec<XorName> = Vec::new();
        close_nodes_one_entry.push(rand::random());
        let actual_relocated_name_one_entry =
            super::calculate_relocation_dst(close_nodes_one_entry.clone(), &original_name);
        assert_ne!(original_name, actual_relocated_name_one_entry);

        let mut combined_one_node_vec: Vec<XorName> = Vec::new();
        combined_one_node_vec.push(original_name);
        combined_one_node_vec.push(close_nodes_one_entry[0]);

        let mut combined_one_node: Vec<u8> = Vec::new();
        for node_id in combined_one_node_vec {
            for i in &node_id.0 {
                combined_one_node.push(*i);
            }
        }

        let expected_relocated_name_one_node = XorName(sha3_256(&combined_one_node));

        assert_eq!(
            actual_relocated_name_one_entry,
            expected_relocated_name_one_node
        );

        // populated closed nodes
        let mut close_nodes: Vec<XorName> = Vec::new();
        for _ in 0..5 {
            close_nodes.push(rand::random());
        }
        let actual_relocated_name =
            super::calculate_relocation_dst(close_nodes.clone(), &original_name);
        assert_ne!(original_name, actual_relocated_name);
        close_nodes.sort_by(|a, b| original_name.cmp_distance(a, b));
        let first_closest = close_nodes[0];
        let second_closest = close_nodes[1];
        let mut combined: Vec<u8> = Vec::new();

        for i in &original_name.0 {
            combined.push(*i);
        }
        for i in &first_closest.0 {
            combined.push(*i);
        }
        for i in &second_closest.0 {
            combined.push(*i);
        }

        let expected_relocated_name = XorName(sha3_256(&combined));
        assert_eq!(expected_relocated_name, actual_relocated_name);

        let mut invalid_combined: Vec<u8> = Vec::new();
        for i in &first_closest.0 {
            invalid_combined.push(*i);
        }
        for i in &second_closest.0 {
            invalid_combined.push(*i);
        }
        for i in &original_name.0 {
            invalid_combined.push(*i);
        }
        let invalid_relocated_name = XorName(sha3_256(&invalid_combined));
        assert_ne!(invalid_relocated_name, actual_relocated_name);
    }
}
