use libp2p::{Multiaddr, PeerId};
use libp2p::swarm::SwarmBuilder;
use log::debug;
use tokio::sync::{mpsc, oneshot};

use waku_core::message::WakuMessage;
use waku_core::pubsub_topic::PubsubTopic;

use crate::behaviour::Behaviour;
use crate::behaviour::Config as BehaviourConfig;
use crate::event_loop::{Command, Event, EventLoop};
use crate::NodeConfig;
use crate::transport::{BoxedP2PTransport, default_transport};

pub struct Node {
    pub config: NodeConfig,
    peer_id: PeerId,
    command_sender: mpsc::Sender<Command>,
    event_receiver: mpsc::Receiver<Event>,
}

impl Node {
    pub fn new_with_transport(
        config: NodeConfig,
        transport: BoxedP2PTransport,
    ) -> anyhow::Result<Self> {
        let peer_id = PeerId::from(&config.keypair.public());

        let switch = {
            let behaviour = Behaviour::new(BehaviourConfig {
                local_public_key: config.keypair.public(),
                keep_alive: config.keepalive.then_some(config.keepalive),
                relay: config.relay.clone(),
            });
            SwarmBuilder::with_tokio_executor(transport, behaviour, peer_id).build()
        };

        let (command_sender, command_receiver) = mpsc::channel(32);
        let (event_sender, event_receiver) = mpsc::channel(32);
        let ev_loop = EventLoop::new(switch, command_receiver, event_sender);

        debug!("start node event loop");
        tokio::spawn(ev_loop.dispatch());

        Ok(Self {
            config,
            peer_id,
            command_sender,
            event_receiver,
        })
    }

    pub fn new(config: NodeConfig) -> anyhow::Result<Self> {
        let transport = default_transport(&config.keypair)?;
        Self::new_with_transport(config, transport)
    }

    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    pub async fn recv_event(&mut self) -> Option<Event> {
        self.event_receiver.recv().await
    }

    pub async fn switch_listen_on(&self, address: &Multiaddr) -> anyhow::Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.command_sender
            .send(Command::switch_listen_on(address.clone(), resp_tx))
            .await?;

        resp_rx.await?
    }

    pub async fn switch_dial(&self, address: &Multiaddr) -> anyhow::Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.command_sender
            .send(Command::switch_dial(address.clone(), resp_tx))
            .await?;

        resp_rx.await?
    }

    pub async fn relay_subscribe(&self, topic: &PubsubTopic) -> anyhow::Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.command_sender
            .send(Command::relay_subscribe(topic.clone(), resp_tx))
            .await?;

        resp_rx.await?
    }

    pub async fn relay_unsubscribe(&self, topic: &PubsubTopic) -> anyhow::Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.command_sender
            .send(Command::relay_unsubscribe(topic.clone(), resp_tx))
            .await?;

        resp_rx.await?
    }

    pub async fn relay_publish(
        &self,
        topic: &PubsubTopic,
        message: WakuMessage,
    ) -> anyhow::Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.command_sender
            .send(Command::relay_publish(topic.clone(), message, resp_tx))
            .await?;

        resp_rx.await?
    }
}
