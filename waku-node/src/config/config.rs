use std::net::IpAddr;

use libp2p::identity::{Keypair, secp256k1};
use libp2p::Multiaddr;

use crate::config::waku_relay_config::WakuRelayConfig;

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub keypair: Keypair,
    pub tcp_ipaddr: IpAddr,
    pub tcp_port: u16,
    pub static_nodes: Vec<Multiaddr>,
    pub keepalive: bool,
    pub relay: Option<WakuRelayConfig>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            keypair: Keypair::generate_secp256k1(),
            tcp_ipaddr: "0.0.0.0".parse().expect("valid ip address format"),
            tcp_port: 0,
            static_nodes: Vec::new(),
            keepalive: false,
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

    pub fn build(&self) -> NodeConfig {
        self.config.clone()
    }

    pub fn keypair(&mut self, keypair: Keypair) -> &mut Self {
        self.config.keypair = keypair;
        self
    }

    // TODO: Move to the nwaku's wakunode2 equivalent
    pub fn keypair_from_secp256k1(&mut self, bytes: &mut [u8]) -> anyhow::Result<&mut Self> {
        // let keys: anyhow::Result<Keypair> = {
        //     let mut key_raw = hex::decode(key).map_err();
        //     let secret = secp256k1::SecretKey::from_bytes(&mut key_raw)?;
        //     Ok(Keypair::Secp256k1(secp256k1::Keypair::from(secret)))
        // };

        let keypair = {
            let mut key_raw = bytes.as_mut();
            let secret_key = secp256k1::SecretKey::from_bytes(&mut key_raw)?;
            secp256k1::Keypair::from(secret_key).into()
        };

        self.config.keypair = keypair;
        Ok(self)
    }

    pub fn tcp(&mut self, address: IpAddr, port: u16) -> &mut Self {
        self.config.tcp_ipaddr = address;
        self.config.tcp_port = port;
        self
    }

    pub fn static_nodes(&mut self, nodes: Vec<Multiaddr>) -> &mut Self {
        self.config.static_nodes = nodes;
        self
    }

    pub fn keepalive(&mut self, enable: bool) -> &mut Self {
        self.config.keepalive = enable;
        self
    }

    pub fn with_waku_relay(&mut self, config: WakuRelayConfig) -> &mut Self {
        self.config.relay = Some(config);
        self
    }
}
