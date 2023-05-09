use libp2p::identity::Keypair;

use crate::config::waku_relay_config::WakuRelayConfig;

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub keypair: Keypair,
    pub keepalive: bool,
    pub ping: bool,
    pub relay: Option<WakuRelayConfig>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            keypair: Keypair::generate_secp256k1(),
            keepalive: false,
            ping: false,
            relay: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct NodeConfigBuilder {
    config: NodeConfig,
}

impl NodeConfigBuilder {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn build(self) -> NodeConfig {
        self.config
    }

    pub fn keypair(mut self, keypair: Keypair) -> Self {
        self.config.keypair = keypair;
        self
    }

    pub fn with_keepalive(mut self, enable: bool) -> Self {
        self.config.keepalive = enable;
        self
    }

    pub fn with_ping(mut self, enable: bool) -> Self {
        self.config.ping = enable;
        self
    }

    pub fn with_waku_relay(mut self, config: WakuRelayConfig) -> Self {
        self.config.relay = Some(config);
        self
    }
}
