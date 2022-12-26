use anyhow::anyhow;
use futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use log::{debug, error, info, trace};
use tokio::sync::mpsc;

use crate::behaviour;
use crate::event_loop::command::Command;
use crate::event_loop::event::Event;

pub struct EventLoop {
    switch: libp2p::Swarm<behaviour::Behaviour>,
    command_source: mpsc::Receiver<Command>,
    event_sink: mpsc::Sender<Event>,
}

impl EventLoop {
    pub fn new(
        switch: libp2p::Swarm<behaviour::Behaviour>,
        command_source: mpsc::Receiver<Command>,
        event_sink: mpsc::Sender<Event>,
    ) -> Self {
        Self {
            switch,
            command_source,
            event_sink,
        }
    }

    pub async fn dispatch(mut self) {
        loop {
            tokio::select! {
                command = self.command_source.recv() => match command {
                    Some(cmd) => { self.handle_command(cmd).await; },
                    None => { debug!("got empty command. terminating node event loop"); return },
                },
                event = self.switch.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } => info!("switch listening on: {address:?}"),
                    SwarmEvent::Behaviour(behaviour::Event::WakuRelay(event)) => {
                        self.handle_waku_relay_event(event).await;
                    },
                    SwarmEvent::Behaviour(event) => debug!("{event:?}"),
                    _ => {}
                },
            }
        }
    }

    async fn handle_command(&mut self, cmd: Command) {
        match cmd {
            Command::SwitchListenOn { address, sender } => {
                trace!("handle command: {}", "switch_listen_on");

                match self.switch.listen_on(address) {
                    Ok(_) => sender.send(Ok(())),
                    Err(e) => sender.send(Err(e.into())),
                }
                .unwrap_or_else(|e| {
                    error!(
                        "send '{}' command response failed: {:?}.",
                        "switch_listen_on", e
                    );
                });
            }
            Command::SwitchDial { address, sender } => {
                trace!("handle command: {}", "switch_dial");

                match self.switch.dial(address) {
                    Ok(_) => sender.send(Ok(())),
                    Err(e) => sender.send(Err(e.into())),
                }
                .unwrap_or_else(|e| {
                    error!("send '{}' command response failed: {:?}.", "switch_dial", e);
                });
            }

            Command::RelaySubscribe {
                pubsub_topic,
                sender,
            } => {
                trace!("handle command: {}", "relay_subscribe");

                if !self.switch.behaviour().waku_relay.is_enabled() {
                    sender
                        .send(Err(anyhow!("relay protocol disabled")))
                        .unwrap_or_else(|e| {
                            error!(
                                "send '{}' command response failed: {:?}.",
                                "relay_subscribe", e
                            );
                        });
                    return;
                }

                match self
                    .switch
                    .behaviour_mut()
                    .waku_relay
                    .as_mut()
                    .unwrap()
                    .subscribe(&pubsub_topic)
                {
                    Ok(_) => sender.send(Ok(())),
                    Err(e) => sender.send(Err(e.into())),
                }
                .unwrap_or_else(|e| {
                    error!(
                        "send '{}' command response failed: {:?}.",
                        "relay_subscribe", e
                    );
                });
            }
            Command::RelayUnsubscribe {
                pubsub_topic,
                sender,
            } => {
                trace!("handle command: {}", "relay_unsubscribe");

                if !self.switch.behaviour().waku_relay.is_enabled() {
                    sender
                        .send(Err(anyhow!("relay protocol disabled")))
                        .unwrap_or_else(|e| {
                            error!(
                                "send '{}' command response failed: {:?}.",
                                "relay_unsubscribe", e
                            );
                        });
                    return;
                }

                match self
                    .switch
                    .behaviour_mut()
                    .waku_relay
                    .as_mut()
                    .unwrap()
                    .unsubscribe(&pubsub_topic)
                {
                    Ok(_) => sender.send(Ok(())),
                    Err(e) => sender.send(Err(e.into())),
                }
                .unwrap_or_else(|e| {
                    error!(
                        "send '{}' command response failed: {:?}.",
                        "relay_unsubscribe", e
                    );
                });
            }
            Command::RelayPublish {
                pubsub_topic,
                message,
                sender,
            } => {
                trace!("handle command: {}", "relay_publish");

                if !self.switch.behaviour().waku_relay.is_enabled() {
                    sender
                        .send(Err(anyhow!("relay protocol disabled")))
                        .unwrap_or_else(|e| {
                            error!(
                                "send '{}' command response failed: {:?}.",
                                "relay_publish", e
                            );
                        });
                    return;
                }

                match self
                    .switch
                    .behaviour_mut()
                    .waku_relay
                    .as_mut()
                    .unwrap()
                    .publish(&pubsub_topic, message)
                {
                    Ok(_) => sender.send(Ok(())),
                    Err(e) => sender.send(Err(e.into())),
                }
                .unwrap_or_else(|e| {
                    error!(
                        "send '{}' command response failed: {:?}.",
                        "relay_publish", e
                    );
                });
            }
        }
    }

    async fn handle_waku_relay_event(&mut self, event: waku_relay::Event) {
        match event {
            waku_relay::Event::Message {
                pubsub_topic,
                message,
            } => {
                trace!("handle event: {}", "waku_relay_message");

                self.event_sink
                    .send(Event::WakuRelayMessage {
                        pubsub_topic,
                        message,
                    })
                    .await
                    .unwrap_or_else(|e| {
                        error!("send '{}' event failed: {:?}.", "waku_relay_message", e);
                    });
            }
            _ => {}
        }
    }
}
