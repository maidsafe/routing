// Copyright 2020 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

mod utils;

use anyhow::{format_err, Result};
use bytes::Bytes;
use qp2p::QuicP2p;
use sn_data_types::Keypair;
use sn_messaging::{
    client::{Message, MessageId, MsgEnvelope, MsgSender, Query, TransferQuery},
    WireMsg,
};
use sn_routing::{Config, DstLocation, Error, Event, NodeElderChange, SrcLocation};
use std::net::{IpAddr, Ipv4Addr};
use utils::*;
use xor_name::XorName;

#[tokio::test]
async fn test_messages_client_node() -> Result<()> {
    let response = b"good bye!";

    let (node, mut event_stream) = create_node(Config {
        first: true,
        ..Default::default()
    })
    .await?;

    // create a client message
    let mut rng = rand::thread_rng();
    let keypair = Keypair::new_ed25519(&mut rng);
    let pk = keypair.public_key();
    let signature = keypair.sign(b"blabla");

    let random_xor = XorName::random();
    let id = MessageId(random_xor);
    let message = Message::Query {
        query: Query::Transfer(TransferQuery::GetBalance(pk)),
        id,
    };

    let msg_envelope = MsgEnvelope {
        message,
        origin: MsgSender::client(pk, signature)?,
        proxies: vec![],
    };
    let msg_envelope_clone = msg_envelope.clone();

    // spawn node events listener
    let node_handler = tokio::spawn(async move {
        while let Some(event) = event_stream.next().await {
            match event {
                Event::ClientMessageReceived { content, send, .. } => {
                    assert_eq!(*content, msg_envelope_clone);

                    // the second message received should be on a bi-stream
                    // and in such case we respond and end the loop.
                    if let Some(mut send_stream) = send {
                        send_stream
                            .send_user_msg(Bytes::from_static(response))
                            .await?;
                        break;
                    }
                }
                _other => {}
            }
        }
        Ok::<(), Error>(())
    });

    // create a client which sends a message to the node
    let node_addr = node.our_connection_info().await?;
    let mut config = sn_routing::TransportConfig {
        ip: Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))),
        ..Default::default()
    };
    config.ip = Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));

    let client = QuicP2p::with_config(Some(config), &[node_addr], false)?;
    let client_endpoint = client.new_endpoint()?;
    let (conn, _) = client_endpoint.connect_to(&node_addr).await?;

    let client_msg_bytes = WireMsg::serialize_client_msg(&msg_envelope)?;

    // we send it on a uni-stream but we don't await for a respnse here
    let _ = conn.send_uni(client_msg_bytes.clone()).await?;

    // and now we send it on a bi-stream where we'll await for a response
    let (_, mut recv) = conn.send_bi(client_msg_bytes).await?;

    // just await for node to respond to client
    node_handler.await??;
    let resp = recv.next().await?;
    assert_eq!(resp, Bytes::from_static(response));

    Ok(())
}

#[tokio::test]
async fn test_messages_between_nodes() -> Result<()> {
    let msg = b"hello!";
    let response = b"good bye!";

    let (node1, mut event_stream) = create_node(Config {
        first: true,
        ..Default::default()
    })
    .await?;
    let node1_contact = node1.our_connection_info().await?;
    let node1_name = node1.name().await;

    // spawn node events listener
    let node_handler = tokio::spawn(async move {
        while let Some(event) = event_stream.next().await {
            match event {
                Event::MessageReceived { content, src, .. } => {
                    assert_eq!(content, Bytes::from_static(msg));
                    return Ok(src.to_dst());
                }
                _other => {}
            }
        }
        Err(format_err!("message not received"))
    });

    // start a second node which sends a message to the first node
    let (node2, mut event_stream) = create_node(config_with_contact(node1_contact)).await?;

    assert_event!(event_stream, Event::EldersChanged { self_status_change: NodeElderChange::Promoted, .. });

    let node2_name = node2.name().await;

    node2
        .send_message(
            SrcLocation::Node(node2_name),
            DstLocation::Node(node1_name),
            Bytes::from_static(msg),
        )
        .await?;

    // just await for node1 to receive message from node2
    let dst = node_handler.await??;

    // send response from node1 to node2
    node1
        .send_message(
            SrcLocation::Node(node1_name),
            dst,
            Bytes::from_static(response),
        )
        .await?;

    // check we received the response message from node1
    while let Some(event) = event_stream.next().await {
        match event {
            Event::MessageReceived { content, src, .. } => {
                assert_eq!(content, Bytes::from_static(response));
                assert_eq!(src, SrcLocation::Node(node1_name));
                return Ok(());
            }
            _other => {}
        }
    }

    Err(format_err!("message not received"))
}
