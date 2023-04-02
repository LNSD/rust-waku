use anyhow::anyhow;
use libp2p::Multiaddr;
use log::{info, LevelFilter};

use waku_core::pubsub_topic::PubsubTopic;
use waku_node::{Node, WakuRelayConfigBuilder};
use waku_node::NodeConfigBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::formatted_builder()
        .filter_level(LevelFilter::Info)
        .format_timestamp_millis()
        .init();

    // Init -----
    let mut key_raw =
        hex::decode("2a6ecd4041f9903e6d57fd5841bc89ee40606e78e2be4202fa7d32485b41cb8c")?;
    let nodes: Vec<Multiaddr> = vec![
        "/dns4/node-01.do-ams3.wakuv2.test.statusim.net/tcp/30303/p2p/16Uiu2HAmPLe7Mzm8TsYUubgCAW1aJoeFScxrLj8ppHFivPo97bUZ".parse()?,
    ];

    let pubsub_topics: Vec<PubsubTopic> = vec!["/waku/2/default-waku/proto".parse().unwrap()];
    let relay_config = WakuRelayConfigBuilder::new()
        .pubsub_topics(pubsub_topics)
        .build();

    let config = NodeConfigBuilder::new()
        .keypair_from_secp256k1(&mut key_raw)?
        .static_nodes(nodes)
        .keepalive(true)
        .with_waku_relay(relay_config)
        .build();

    let mut node = Node::new(config)?;

    // Setup -----
    let addr = format!(
        "/ip4/{}/tcp/{}",
        &node.config.tcp_ipaddr, &node.config.tcp_port
    );
    if let Ok(listen_addr) = addr.parse() {
        node.switch_listen_on(&listen_addr).await?;
    } else {
        return Err(anyhow!("invalid listen multiaddr format: {addr:?}"));
    }

    for peer in &node.config.static_nodes {
        node.switch_dial(peer).await?;
    }

    if let Some(conf) = node.config.relay.clone() {
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
