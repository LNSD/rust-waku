use libp2p::PeerId;

#[derive(Debug, Clone, Default)]
pub struct WakuRelayConfig {
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

    pub fn static_nodes(&mut self, nodes: Vec<PeerId>) -> &mut Self {
        self.config.static_nodes = nodes;
        self
    }
}
