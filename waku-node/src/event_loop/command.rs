use libp2p::Multiaddr;
use strum_macros::Display;
use tokio::sync::oneshot;

use waku_message::{PubsubTopic, WakuMessage};

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
    RelaySubscribe {
        pubsub_topic: PubsubTopic,
        sender: oneshot::Sender<anyhow::Result<()>>,
    },
    RelayUnsubscribe {
        pubsub_topic: PubsubTopic,
        sender: oneshot::Sender<anyhow::Result<()>>,
    },
    RelayPublish {
        pubsub_topic: PubsubTopic,
        message: WakuMessage,
        sender: oneshot::Sender<anyhow::Result<()>>,
    },
}

impl Command {
    pub fn switch_listen_on(
        address: Multiaddr,
        sender: oneshot::Sender<anyhow::Result<()>>,
    ) -> Self {
        Command::SwitchListenOn { address, sender }
    }

    pub fn switch_dial(address: Multiaddr, sender: oneshot::Sender<anyhow::Result<()>>) -> Self {
        Command::SwitchDial { address, sender }
    }

    pub fn relay_subscribe(
        topic: PubsubTopic,
        sender: oneshot::Sender<anyhow::Result<()>>,
    ) -> Self {
        Command::RelaySubscribe {
            pubsub_topic: topic,
            sender,
        }
    }
    pub fn relay_unsubscribe(
        topic: PubsubTopic,
        sender: oneshot::Sender<anyhow::Result<()>>,
    ) -> Self {
        Command::RelayUnsubscribe {
            pubsub_topic: topic,
            sender,
        }
    }
    pub fn relay_publish(
        topic: PubsubTopic,
        message: WakuMessage,
        sender: oneshot::Sender<anyhow::Result<()>>,
    ) -> Self {
        Command::RelayPublish {
            pubsub_topic: topic,
            message,
            sender,
        }
    }
}
