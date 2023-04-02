use libp2p::PeerId;

use waku_core::pubsub_topic::PubsubTopic;

#[derive(Debug, Clone, Default)]
pub struct WakuRelayConfig {
    pub pubsub_topics: Vec<PubsubTopic>,
    pub static_nodes: Vec<PeerId>,
}

#[derive(Default)]
pub struct WakuRelayConfigBuilder {
    config: WakuRelayConfig,
}

impl WakuRelayConfigBuilder {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn build(&self) -> WakuRelayConfig {
        self.config.clone()
    }

    pub fn pubsub_topics(&mut self, topics: Vec<PubsubTopic>) -> &mut Self {
        self.config.pubsub_topics = topics;
        self
    }

    pub fn static_nodes(&mut self, nodes: Vec<PeerId>) -> &mut Self {
        self.config.static_nodes = nodes;
        self
    }
}
