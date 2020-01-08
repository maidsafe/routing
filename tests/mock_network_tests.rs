// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

#![cfg(feature = "mock_base")]
// For explanation of lint checks, run `rustc -W help` or see
// https://github.com/maidsafe/QA/blob/master/Documentation/Rust%20Lint%20Checks.md
#![forbid(
    bad_style,
    exceeding_bitshifts,
    mutable_transmutes,
    no_mangle_const_items,
    unknown_crate_types,
    warnings
)]
#![deny(
    deprecated,
    improper_ctypes,
    missing_docs,
    non_shorthand_field_patterns,
    overflowing_literals,
    plugin_as_library,
    stable_features,
    unconditional_recursion,
    unknown_lints,
    unsafe_code,
    unused,
    unused_allocation,
    unused_attributes,
    unused_comparisons,
    unused_features,
    unused_parens,
    while_true
)]
#![warn(
    trivial_casts,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    unused_results
)]
#![allow(
    box_pointers,
    missing_copy_implementations,
    missing_debug_implementations,
    variant_size_differences,
    // FIXME: Re-enable `redundant_field_names`.
    clippy::redundant_field_names
)]

#[macro_use]
extern crate log;
#[allow(unused_extern_crates, clippy::useless_attribute)]
extern crate maidsafe_utilities;
#[macro_use]
extern crate unwrap;

// This module is a driver and defines macros. See `mock_network` modules for
// tests.

/// Expect that the next event raised by the node matches the given pattern.
/// Panics if no event, or an event that does not match the pattern is raised.
/// (ignores ticks).
macro_rules! expect_next_event {
    ($node:expr, $pattern:pat) => {
        loop {
            match $node.inner.try_next_ev() {
                Ok($pattern) => break,
                Ok(Event::TimerTicked) => (),
                other => panic!(
                    "Expected Ok({}) at {}, got {:?}",
                    stringify!($pattern),
                    $node.name(),
                    other
                ),
            }
        }
    };
}

/// Expects that any event raised by the node matches the given pattern
/// (with optional pattern guard). Ignores events that do not match the pattern.
/// Panics if the event channel is exhausted before matching event is found.
macro_rules! expect_any_event {
    ($node:expr, $pattern:pat) => {
        expect_any_event!($node, $pattern if true)
    };
    ($node:expr, $pattern:pat if $guard:expr) => {
        loop {
            match $node.inner.try_next_ev() {
                Ok($pattern) if $guard => break,
                Ok(_) => (),
                other => panic!(
                    "Expected Ok({}) at {}, got {:?}",
                    stringify!($pattern),
                    $node.name(),
                    other
                ),
            }
        }
    };
}

/// Expects that the node did not raise an even matching the given pattern, panics otherwise
/// (ignores ticks). If no pattern given, expects that no event (except ticks) were raised.
macro_rules! expect_no_event {
    ($node:expr) => {{
        match $node.inner.try_next_ev() {
            Ok(Event::TimerTicked) => (),
            Err(crossbeam_channel::TryRecvError::Empty) => (),
            other => panic!("Expected no event at {}, got {:?}", $node.name(), other),
        }
    }};

    ($node:expr, $pattern:pat) => {
        loop {
            match $node.inner.try_next_ev() {
                Ok(event @ $pattern) => panic!(
                    "Expected no event matching {} at {}, got {:?}",
                    stringify!($pattern),
                    $node.name(),
                    event
                ),
                Ok(_) => (),
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(error) => panic!("Unexpected error {:?}", error),
            }
        }
    };
}

mod mock_network;
