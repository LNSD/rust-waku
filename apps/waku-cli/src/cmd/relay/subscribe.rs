use std::str::FromStr;
use std::time::Duration;

use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::swarm::{SwarmBuilder, SwarmEvent};
use libp2p::PeerId;
use log::{debug, info};
use multiaddr::Multiaddr;
use tokio::time::timeout;

use waku_core::content_topic::ContentTopic;
use waku_core::pubsub_topic::PubsubTopic;
use waku_node::behaviour::{Behaviour as NodeBehaviour, Config as NodeBehaviourConfig};
use waku_node::default_transport;

#[derive(Debug, Clone, clap::Args)]
pub struct RelaySubscribeCmd {
    #[arg(long)]
    pub peer: String,
    #[arg(long)]
    pub pubsub_topic: String,
    #[arg(long)]
    pub content_topic: String,
}

pub async fn run_cmd(args: RelaySubscribeCmd) -> anyhow::Result<()> {
    // Parse command line arguments data
    let peer = args
        .peer
        .parse::<Multiaddr>()
        .map_err(|e| anyhow::anyhow!("Invalid peer address: {e}"))?;
    let pubsub_topic = args.pubsub_topic.parse::<PubsubTopic>().unwrap();
    let content_topic = args.content_topic.parse::<ContentTopic>().unwrap();

    // Build the waku node
    let keypair = Keypair::generate_secp256k1();
    let peer_id = PeerId::from(&keypair.public());

    let mut switch = {
        let transport = default_transport(&keypair)?;

        let conf = NodeBehaviourConfig {
            local_public_key: keypair.public(),
            keep_alive: None,
            ping: None,
            relay: Some(Default::default()),
        };
        let behaviour = NodeBehaviour::new(conf);

        SwarmBuilder::with_tokio_executor(transport, behaviour, peer_id).build()
    };

    // Start node
    info!("Peer ID: {}", peer_id);

    // Start switch
    let listen_addr = Multiaddr::from_str("/ip4/0.0.0.0/tcp/0").expect("Valid multiaddr");
    switch
        .listen_on(listen_addr.clone())
        .map_err(|e| anyhow::anyhow!("Failed to listen: {e}"))?;

    // Dial peer
    info!("Dialing peer: {}", peer);
    switch
        .dial(peer.clone())
        .map_err(|e| anyhow::anyhow!("Failed to dial peer: {e}"))?;

    // Await dial confirmation
    timeout(Duration::from_secs(5), async {
        loop {
            if let SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } = switch.select_next_some().await
            {
                let addr = endpoint.get_remote_address();
                if addr == &peer {
                    info!("Peer connection established: {addr} ({peer_id})");
                    return;
                }

                debug!("Connection established: {addr} ({peer_id})");
            }
        }
    })
    .await
    .map_err(|e| anyhow::anyhow!("Failed to dial peer (timeout): {e}"))?;

    // Join/Subscribe pubsub topic
    info!("Subscribing to pubsub topic: {pubsub_topic}");
    switch
        .behaviour_mut()
        .waku_relay
        .as_mut()
        .expect("Waku relay behaviour is enabled")
        .subscribe(&pubsub_topic)
        .map_err(|e| anyhow::anyhow!("Failed to join pubsub topic: {e}"))?;

    // Await subscription confirmation
    timeout(Duration::from_secs(5), async {
        loop {
            if let SwarmEvent::Behaviour(waku_node::behaviour::Event::WakuRelay(
                waku_relay::Event::Subscribed {
                    pubsub_topic: topic,
                    ..
                },
            )) = switch.select_next_some().await
            {
                if topic == pubsub_topic {
                    info!("Joined pubsub topic: {topic}");
                    return;
                }

                debug!("Joined pubsub topic: {topic}");
            }
        }
    })
    .await
    .map_err(|e| anyhow::anyhow!("Failed to subscribe to pubsub topic (timeout): {e}"))?;

    // Log messages matching the pubsub and content topics filter criteria
    loop {
        if let SwarmEvent::Behaviour(waku_node::behaviour::Event::WakuRelay(
            waku_relay::Event::Message {
                message,
                pubsub_topic: topic,
            },
        )) = switch.select_next_some().await
        {
            if pubsub_topic != topic {
                continue;
            }

            if message.content_topic != content_topic {
                continue;
            }

            info!("Message received on '{topic}': {message:?}");
        }
    }
}
