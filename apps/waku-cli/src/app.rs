use libp2p::bytes::Bytes;
use log::info;
use multiaddr::Multiaddr;
use ulid::Ulid;

use waku_core::content_topic::ContentTopic;
use waku_core::message::WakuMessage;
use waku_core::pubsub_topic::PubsubTopic;
use waku_node::{Event, Node, NodeConfigBuilder};

use crate::cli::{Cli, Commands, RelayCommand};

#[derive(Debug, Clone)]
pub struct AppConf {
    pub bootstrap_nodes: Vec<Multiaddr>,
    pub cmd_conf: AppCmdConf,
}

#[derive(Debug, Clone)]
pub enum AppCmdConf {
    RelaySubscribe {
        pubsub_topic: PubsubTopic,
        content_topic: ContentTopic,
    },
    RelayPublish {
        pubsub_topic: PubsubTopic,
        content_topic: ContentTopic,
        message: String,
    },
}

impl From<Cli> for AppConf {
    fn from(cli: Cli) -> Self {
        match cli {
            Cli {
                command: Commands::Relay(cmd),
            } => match cmd {
                RelayCommand::Publish {
                    peer,
                    pubsub_topic,
                    content_topic,
                    payload,
                } => Self {
                    bootstrap_nodes: vec![peer.parse().unwrap()],
                    cmd_conf: AppCmdConf::RelayPublish {
                        pubsub_topic: pubsub_topic.parse().unwrap(),
                        content_topic: content_topic.parse().unwrap(),
                        message: payload,
                    },
                },
                RelayCommand::Subscribe {
                    peer,
                    pubsub_topic,
                    content_topic,
                } => Self {
                    bootstrap_nodes: vec![peer.parse().unwrap()],
                    cmd_conf: AppCmdConf::RelaySubscribe {
                        pubsub_topic: pubsub_topic.parse().unwrap(),
                        content_topic: content_topic.parse().unwrap(),
                    },
                },
            },
        }
    }
}

pub struct App {
    conf: AppConf,
    node: Node,
}

impl App {
    fn new_relay_node() -> anyhow::Result<Node> {
        let node_conf = NodeConfigBuilder::default()
            .with_keepalive(true)
            .with_ping(true)
            .with_waku_relay(Default::default())
            .build();

        Node::new(node_conf)
    }

    pub fn new(conf: AppConf) -> anyhow::Result<Self> {
        let node = match conf.cmd_conf {
            AppCmdConf::RelayPublish { .. } | AppCmdConf::RelaySubscribe { .. } => {
                Self::new_relay_node()?
            }
        };

        Ok(Self { conf, node })
    }

    pub async fn setup(&mut self) -> anyhow::Result<()> {
        // Listen on a random TCP port
        let addr = "/ip4/0.0.0.0/tcp/0".parse().unwrap();
        self.node.switch_listen_on(&addr).await?;

        for peer in &self.conf.bootstrap_nodes {
            info!("Bootstrapping to {}", peer);
            self.node.switch_dial(peer).await?;
        }

        match &self.conf.cmd_conf {
            AppCmdConf::RelayPublish { pubsub_topic, .. } => {
                self.node.relay_subscribe(pubsub_topic).await?;
            }
            AppCmdConf::RelaySubscribe { pubsub_topic, .. } => {
                self.node.relay_subscribe(pubsub_topic).await?;
            }
        }

        Ok(())
    }

    pub async fn run(&mut self) -> anyhow::Result<Option<Event>> {
        match self.conf.cmd_conf.clone() {
            AppCmdConf::RelaySubscribe { content_topic, .. } => {
                if let Some(Event::WakuRelayMessage {
                                message,
                                pubsub_topic,
                            }) = self.node.recv_event().await
                {
                    if message.content_topic != content_topic {
                        return Ok(None);
                    }

                    return Ok(Some(Event::WakuRelayMessage {
                        message,
                        pubsub_topic,
                    }));
                }
                Ok(None)
            }
            AppCmdConf::RelayPublish {
                pubsub_topic,
                content_topic,
                message,
            } => {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

                let meta = Bytes::from(Ulid::new().0.to_be_bytes().to_vec());
                let payload = Bytes::from(message.clone());
                let message = WakuMessage {
                    content_topic: content_topic.clone(),
                    meta: Some(meta),
                    payload,
                    ephemeral: true,
                };

                info!("Publishing message: {message:?}");
                self.node.relay_publish(&pubsub_topic, message).await?;
                Ok(None)
            }
        }
    }
}
