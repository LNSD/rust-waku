use libp2p::{Multiaddr, PeerId};
use log::debug;
use tokio::sync::{mpsc, oneshot};

use crate::behaviour::Behaviour;
use crate::behaviour::Config as BehaviourConfig;
use crate::config::NodeConfig;
use crate::event_loop::{Command, Event, EventLoop};
use crate::transport::create_transport;

pub struct Node {
    config: NodeConfig,
    peer_id: PeerId,
    command_sender: mpsc::Sender<Command>,
    event_receiver: mpsc::Receiver<Event>,
}

impl Node {
    pub fn new(config: NodeConfig) -> anyhow::Result<Self> {
        let peer_id = PeerId::from(&config.keypair.public());

        let switch = {
            let transport = create_transport(&config.keypair)?;
            let behaviour = Behaviour::new(BehaviourConfig {
                local_public_key: config.keypair.public(),
            });
            libp2p::Swarm::with_tokio_executor(transport, behaviour, peer_id)
        };

        let (command_sender, command_receiver) = mpsc::channel(32);
        let (event_sender, event_receiver) = mpsc::channel(32);
        let mut ev_loop = EventLoop::new(switch, command_receiver, event_sender);

        debug!("start node event loop");
        tokio::spawn(ev_loop.dispatch());

        Ok(Self {
            config,
            peer_id,
            command_sender,
            event_receiver,
        })
    }

    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    pub async fn switch_listen_on(&self, address: Multiaddr) -> anyhow::Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.command_sender
            .send(Command::switch_listen_on(address, resp_tx))
            .await?;

        resp_rx.await?
    }

    pub async fn switch_dial(&self, address: Multiaddr) -> anyhow::Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.command_sender
            .send(Command::switch_dial(address, resp_tx))
            .await?;

        resp_rx.await?
    }
}
