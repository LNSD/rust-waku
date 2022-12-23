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
        }
    }
}
