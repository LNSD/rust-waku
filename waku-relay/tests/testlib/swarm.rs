use futures::StreamExt;
use libp2p::identity::{secp256k1, Keypair};
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, Swarm};

use waku_relay::gossipsub::Behaviour;

pub async fn poll(swarm: &mut Swarm<Behaviour>) {
    loop {
        let event = swarm.select_next_some().await;
        log::trace!("Event: {:?}", event);
    }
}

pub fn secp256k1_keypair(key: &str) -> Keypair {
    let raw_key = hex::decode(key).expect("key to be valid");
    let secret_key = secp256k1::SecretKey::try_from_bytes(raw_key).unwrap();
    secp256k1::Keypair::from(secret_key).into()
}

pub async fn wait_for_new_listen_addr(swarm: &mut Swarm<Behaviour>) -> Multiaddr {
    loop {
        let event = swarm.select_next_some().await;
        log::trace!("Event: {:?}", event);
        if let SwarmEvent::NewListenAddr { address, .. } = event {
            return address;
        }
    }
}

pub async fn wait_for_incoming_connection(swarm: &mut Swarm<Behaviour>) {
    loop {
        let event = swarm.select_next_some().await;
        log::trace!("Event: {:?}", event);
        if matches!(event, SwarmEvent::IncomingConnection { .. }) {
            break;
        }
    }
}

pub async fn wait_for_dialing(swarm: &mut Swarm<Behaviour>) {
    loop {
        let event = swarm.select_next_some().await;
        log::trace!("Event: {:?}", event);
        if matches!(event, SwarmEvent::Dialing { .. }) {
            break;
        }
    }
}

pub async fn wait_for_connection_established(swarm: &mut Swarm<Behaviour>) {
    loop {
        let event = swarm.select_next_some().await;
        log::trace!("Event: {:?}", event);
        if matches!(event, SwarmEvent::ConnectionEstablished { .. }) {
            break;
        }
    }
}
