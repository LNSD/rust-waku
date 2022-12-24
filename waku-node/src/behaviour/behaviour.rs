use libp2p::{identify, ping};
use libp2p::identity::PublicKey;
use libp2p::swarm::behaviour::toggle;
use libp2p::swarm::NetworkBehaviour;

use crate::WakuRelayConfig;

pub struct Config {
    pub local_public_key: PublicKey,
    pub relay: Option<WakuRelayConfig>,
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "crate::behaviour::event::Event")]
pub struct Behaviour {
    pub ping: ping::Behaviour,
    pub identify: identify::Behaviour,
    pub waku_relay: toggle::Toggle<waku_relay::Behaviour>,
}

impl Behaviour {
    pub fn new(config: Config) -> Self {
        let identify = identify::Behaviour::new(
            identify::Config::new("/ipfs/id/1.0.0".to_owned(), config.local_public_key)
                .with_agent_version(format!("rust-waku/{}", env!("CARGO_PKG_VERSION"))),
        );
        let waku_relay = toggle::Toggle::from(config.relay.map(|_| Default::default()));

        Self {
            ping: Default::default(),
            identify,
            waku_relay,
        }
    }
}
