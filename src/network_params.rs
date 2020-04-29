// Copyright 2019 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use crate::{ELDER_SIZE, SAFE_SECTION_SIZE};

/// Network parameters: number of elders, safe section size
#[derive(Clone, Copy, Debug)]
pub struct NetworkParams {
    /// The number of elders per section
    pub elder_size: usize,
    /// Minimum number of nodes we consider safe in a section
    pub safe_section_size: usize,
}

impl Default for NetworkParams {
    fn default() -> Self {
        Self {
            elder_size: ELDER_SIZE,
            safe_section_size: SAFE_SECTION_SIZE,
        }
    }
}
