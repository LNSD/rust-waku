use libp2p::identity::{Keypair, secp256k1};
use log::info;
use multiaddr::Multiaddr;

use waku_core::pubsub_topic::PubsubTopic;
use waku_node::{Event, Node, NodeConfig, NodeConfigBuilder, WakuRelayConfigBuilder};

use crate::config::Wakunode2Conf;

#[derive(Debug, Clone)]
pub struct AppConf {
    pub node_conf: NodeConfig,
    pub listen_addresses: Vec<Multiaddr>,
    pub bootstrap_nodes: Vec<Multiaddr>,
    pub topics: Vec<PubsubTopic>,
}

fn try_to_multiaddr(addr: Vec<String>) -> anyhow::Result<Vec<Multiaddr>> {
    addr.into_iter()
        .map(|addr| addr.parse::<Multiaddr>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("Failed to parse multiaddr: {}", e))
}

fn to_pubsub_topic(topic: Vec<String>) -> Vec<PubsubTopic> {
    topic.into_iter().map(|t| t.into()).collect()
}

fn keypair_from_secp256k1(private_key: String) -> anyhow::Result<Keypair> {
    let raw_key = hex::decode(private_key)?;
    let keypair = {
        let secret_key = secp256k1::SecretKey::try_from_bytes(raw_key)?;
        secp256k1::Keypair::from(secret_key).into()
    };

    Ok(keypair)
}

impl TryFrom<Wakunode2Conf> for AppConf {
    type Error = anyhow::Error;

    fn try_from(c: Wakunode2Conf) -> Result<Self, Self::Error> {
        let node_conf = c.clone().try_into()?;
        Ok(Self {
            node_conf,
            listen_addresses: try_to_multiaddr(c.listen_addresses)?,
            bootstrap_nodes: try_to_multiaddr(c.bootstrap_nodes)?,
            topics: to_pubsub_topic(c.topics),
        })
    }
}

impl TryFrom<Wakunode2Conf> for NodeConfig {
    type Error = anyhow::Error;

    fn try_from(c: Wakunode2Conf) -> Result<Self, Self::Error> {
        let mut builder = NodeConfigBuilder::new()
            .keypair(keypair_from_secp256k1(c.private_key)?)
            .with_keepalive(c.keepalive);

        if c.relay {
            let topics = to_pubsub_topic(c.topics);
            let relay_config = WakuRelayConfigBuilder::new().pubsub_topics(topics).build();
            builder = builder.with_waku_relay(relay_config);
        }

        Ok(builder.build())
    }
}

pub struct App {
    conf: AppConf,
    node: Node,
}

impl App {
    pub fn new(conf: Wakunode2Conf) -> anyhow::Result<Self> {
        let app_conf: AppConf = conf.try_into()?;

        Ok(Self {
            conf: app_conf.clone(),
            node: Node::new(app_conf.node_conf)?,
        })
    }

    pub async fn setup(&mut self) -> anyhow::Result<()> {
        for addr in &self.conf.listen_addresses {
            self.node.switch_listen_on(addr).await?;
            info!("Listening on {addr}");
        }

        for peer in &self.conf.bootstrap_nodes {
            info!("Bootstrapping to {}", peer);
            self.node.switch_dial(peer).await?;
        }

        if self.conf.node_conf.relay.is_some() {
            for topic in &self.conf.topics {
                info!("Subscribing to {topic}");
                self.node.relay_subscribe(topic).await?;
            }
        }

        info!("Node is ready: {}", self.node.peer_id());
        Ok(())
    }

    pub async fn run(&mut self) -> Option<Event> {
        self.node.recv_event().await
    }
}
