use libp2p::identity::{Keypair, secp256k1};
use libp2p::Multiaddr;
use log::{info, LevelFilter};

use waku_core::pubsub_topic::PubsubTopic;
use waku_node::{Node, WakuRelayConfigBuilder};
use waku_node::NodeConfigBuilder;

fn keypair_from_secp256k1<S: AsRef<[u8]>>(private_key: S) -> anyhow::Result<Keypair> {
    let raw_key = hex::decode(private_key)?;
    let secret_key = secp256k1::SecretKey::try_from_bytes(raw_key)?;
    Ok(secp256k1::Keypair::from(secret_key).into())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::formatted_builder()
        .filter_level(LevelFilter::Info)
        .format_timestamp_millis()
        .init();

    // Init -----
    let keypair =
        keypair_from_secp256k1("2a6ecd4041f9903e6d57fd5841bc89ee40606e78e2be4202fa7d32485b41cb8c")?;

    let nodes: Vec<Multiaddr> = vec![
        "/dns4/node-01.do-ams3.wakuv2.test.statusim.net/tcp/30303/p2p/16Uiu2HAmPLe7Mzm8TsYUubgCAW1aJoeFScxrLj8ppHFivPo97bUZ".parse()?,
    ];

    let pubsub_topics: Vec<PubsubTopic> = vec!["/waku/2/default-waku/proto".parse().unwrap()];
    let relay_config = WakuRelayConfigBuilder::new()
        .pubsub_topics(pubsub_topics)
        .build();

    let config = NodeConfigBuilder::new()
        .keypair(keypair)
        .with_keepalive(true)
        .with_ping(true)
        .with_waku_relay(relay_config)
        .build();

    let mut node = Node::new(config.clone())?;

    // Setup -----
    // Listen on a random port
    let addr = "/ip4/0.0.0.0/tcp/0".parse()?;
    node.switch_listen_on(&addr).await?;

    // Connect to bootstrap nodes
    for peer in &nodes {
        node.switch_dial(peer).await?;
    }

    // Subscribe to relay topics
    if let Some(conf) = config.relay {
        for topic in conf.pubsub_topics {
            node.relay_subscribe(&topic).await?;
        }
    }

    loop {
        if let Some(event) = node.recv_event().await {
            info!("{event:?}");
        }
    }
}
