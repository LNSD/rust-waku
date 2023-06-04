use futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use libp2p::Swarm;

use waku_relay::gossipsub::Behaviour;

pub async fn wait_for_new_listen_addr(swarm: &mut Swarm<Behaviour>) {
    loop {
        let event = swarm.select_next_some().await;
        log::trace!("Event: {:?}", event);
        if let SwarmEvent::NewListenAddr { .. } = event {
            break;
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
