// Copyright 2020 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

mod utils;

use anyhow::{Error, Result};
use ed25519_dalek::Keypair;
use futures::future;
use sn_routing::{
    event::{Connected, Event},
    EventStream, Routing, ELDER_SIZE,
};
use tokio::time;
use utils::*;
use xor_name::XorName;

#[tokio::test]
async fn test_genesis_node() -> Result<()> {
    let keypair = Keypair::generate(&mut rand::thread_rng());
    let pub_key = keypair.public;
    let (node, mut event_stream) = RoutingBuilder::new(None)
        .first()
        .keypair(keypair)
        .create()
        .await?;

    assert_eq!(pub_key, node.public_key().await);

    assert_next_event!(event_stream, Event::Connected(Connected::First));
    assert_next_event!(event_stream, Event::PromotedToElder);

    assert!(node.is_elder().await);

    Ok(())
}

#[tokio::test]
async fn test_node_bootstrapping() -> Result<()> {
    let (genesis_node, mut event_stream) = RoutingBuilder::new(None).first().create().await?;

    // spawn genesis node events listener
    let genesis_handler = tokio::spawn(async move {
        assert_next_event!(event_stream, Event::Connected(Connected::First));
        assert_next_event!(event_stream, Event::PromotedToElder);
        assert_next_event!(event_stream, Event::InfantJoined { age: 4, name: _ });
        // TODO: Should we expect EldersChanged event too ??
        // assert_next_event!(event_stream, Event::EldersChanged { .. })?;
        Ok::<(), Error>(())
    });

    // bootstrap a second node with genesis
    let genesis_contact = genesis_node.our_connection_info()?;
    let (node1, mut event_stream) = RoutingBuilder::new(None)
        .with_contact(genesis_contact)
        .create()
        .await?;

    assert_next_event!(event_stream, Event::Connected(Connected::First));

    // just await for genesis node to finish receiving all events
    genesis_handler.await??;

    let elder_size = 2;
    verify_invariants_for_node(&genesis_node, elder_size).await?;
    verify_invariants_for_node(&node1, elder_size).await?;

    Ok(())
}

#[tokio::test]
async fn test_section_bootstrapping() -> Result<()> {
    let (genesis_node, mut event_stream) = RoutingBuilder::new(None).first().create().await?;

    // spawn genesis node events listener
    let genesis_handler = tokio::spawn(async move {
        // expect events for all nodes
        let mut joined_nodes = Vec::default();
        while let Some(event) = event_stream.next().await {
            match event {
                Event::InfantJoined { age, name } => {
                    assert_eq!(age, 4);
                    joined_nodes.push(name);
                }
                _other => {}
            }

            if joined_nodes.len() == ELDER_SIZE {
                break;
            }
        }

        Ok::<Vec<XorName>, Error>(joined_nodes)
    });

    // bootstrap several nodes with genesis to form a section
    let genesis_contact = genesis_node.our_connection_info()?;
    let mut nodes_joining_tasks = Vec::with_capacity(ELDER_SIZE);
    for _ in 0..ELDER_SIZE {
        nodes_joining_tasks.push(async {
            let (node, mut event_stream) = RoutingBuilder::new(None)
                .with_contact(genesis_contact)
                .create()
                .await?;

            assert_next_event!(event_stream, Event::Connected(Connected::First));

            Ok::<Routing, Error>(node)
        });
    }

    let nodes = future::join_all(nodes_joining_tasks).await;

    // just await for genesis node to finish receiving all events
    let joined_nodes = genesis_handler.await??;

    for result in nodes {
        let node = result?;
        let name = node.name().await;

        // assert names of nodes joined match
        let found = joined_nodes.iter().find(|n| **n == name);
        assert!(found.is_some());

        verify_invariants_for_node(&node, ELDER_SIZE).await?;
    }

    Ok(())
}

// Test that the first `ELDER_SIZE` nodes in the network are promoted to elders.
#[tokio::test]
async fn test_startup_elders() -> Result<()> {
    // FIXME: using only 3 nodes for now because with 4 or more the test takes too long (but still
    // succeeds). Needs further investigation.
    let network_size = 3;
    let mut nodes = create_connected_nodes(network_size).await?;

    async fn expect_promote_event(stream: &mut EventStream) {
        while let Some(event) = stream.next().await {
            if let Event::PromotedToElder = event {
                return;
            }
        }

        panic!("event stream closed before receiving Event::PromotedToElder");
    }

    let _ = time::timeout(
        TIMEOUT,
        future::join_all(nodes.iter_mut().map(|(node, stream)| async move {
            if node.is_elder().await {
                return;
            }

            expect_promote_event(stream).await
        })),
    )
    .await?;

    Ok(())
}
