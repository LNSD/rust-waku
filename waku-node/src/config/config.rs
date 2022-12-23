use std::net::IpAddr;

use libp2p::identity::Keypair;

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub keypair: Keypair,
    pub tcp_ipaddr: IpAddr,
    pub tcp_port: u16,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            keypair: Keypair::generate_secp256k1(),
            tcp_ipaddr: "0.0.0.0".parse().expect("valid ip address format"),
            tcp_port: 0,
        }
    }
}
