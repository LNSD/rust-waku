use std::net::IpAddr;

use libp2p::identity::{Keypair, secp256k1};

use crate::config::config::NodeConfig;

#[derive(Debug, Default)]
pub struct NodeConfigBuilder {
    config: NodeConfig,
}

impl NodeConfigBuilder {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn build(&mut self) -> NodeConfig {
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
            Keypair::Secp256k1(secp256k1::Keypair::from(secret_key))
        };

        self.config.keypair = keypair;
        Ok(self)
    }

    pub fn tcp(&mut self, address: IpAddr, port: u16) -> &mut Self {
        self.config.tcp_ipaddr = address;
        self.config.tcp_port = port;
        self
    }
}
