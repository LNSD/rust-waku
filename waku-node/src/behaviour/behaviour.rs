use libp2p::{identify, ping};
use libp2p::identity::PublicKey;
use libp2p::swarm::behaviour::toggle;
use libp2p::swarm::keep_alive;
use libp2p::swarm::NetworkBehaviour;

use crate::WakuRelayConfig;

pub struct Config {
    pub local_public_key: PublicKey,
    pub keep_alive: Option<bool>,
    pub ping: Option<bool>,
    pub relay: Option<WakuRelayConfig>,
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "crate::behaviour::event::Event")]
pub struct Behaviour {
    pub keep_alive: toggle::Toggle<keep_alive::Behaviour>,
    pub ping: toggle::Toggle<ping::Behaviour>,
    pub identify: identify::Behaviour,
    pub waku_relay: toggle::Toggle<waku_relay::Behaviour>,
}

impl Behaviour {
    pub fn new(config: Config) -> Self {
        let keep_alive = toggle::Toggle::from(config.keep_alive.map(|_| Default::default()));
        let ping = toggle::Toggle::from(config.ping.map(|_| Default::default()));
        let identify = identify::Behaviour::new(
            identify::Config::new("/ipfs/id/1.0.0".to_owned(), config.local_public_key)
                .with_agent_version(format!("rust-waku/{}", env!("CARGO_PKG_VERSION"))),
        );
        let waku_relay = toggle::Toggle::from(config.relay.map(|_| Default::default()));

        Self {
            keep_alive,
            ping,
            identify,
            waku_relay,
        }
    }
}
