use libp2p::Multiaddr;
use strum_macros::Display;
use tokio::sync::oneshot;

#[derive(Debug, Display)]
pub enum Command {
    SwitchListenOn {
        address: Multiaddr,
        sender: oneshot::Sender<anyhow::Result<()>>,
    },
    SwitchDial {
        address: Multiaddr,
        sender: oneshot::Sender<anyhow::Result<()>>,
    },
}

impl Command {
    pub fn switch_listen_on(address: Multiaddr, sender: oneshot::Sender<anyhow::Result<()>>) -> Self {
        Command::SwitchListenOn { address, sender }
    }

    pub fn switch_dial(address: Multiaddr, sender: oneshot::Sender<anyhow::Result<()>>) -> Self {
        Command::SwitchDial { address, sender }
    }
}
