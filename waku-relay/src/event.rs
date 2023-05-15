use libp2p::{gossipsub, PeerId};
use prost::Message;
use strum_macros::Display;

use waku_core::message::proto::waku::message::v1::WakuMessage as WakuMessageProto;
use waku_core::message::WakuMessage;
use waku_core::message::MAX_WAKU_MESSAGE_SIZE;
use waku_core::pubsub_topic::PubsubTopic;

#[derive(Debug, Display)]
pub enum Event {
    InvalidMessage,
    Subscribed {
        peer_id: PeerId,
        pubsub_topic: PubsubTopic,
    },
    Unsubscribed {
        peer_id: PeerId,
        pubsub_topic: PubsubTopic,
    },
    Message {
        pubsub_topic: PubsubTopic,
        message: WakuMessage,
    },
    WakuRelayNotSupported {
        peer_id: PeerId,
    },
}

impl From<gossipsub::Event> for Event {
    fn from(event: gossipsub::Event) -> Self {
        match event {
            gossipsub::Event::Subscribed { peer_id, topic } => Self::Subscribed {
                peer_id,
                pubsub_topic: PubsubTopic::new(topic.into_string()),
            },
            gossipsub::Event::Unsubscribed { peer_id, topic } => Self::Unsubscribed {
                peer_id,
                pubsub_topic: PubsubTopic::new(topic.into_string()),
            },
            gossipsub::Event::Message { message, .. } => {
                if message.data.len() > MAX_WAKU_MESSAGE_SIZE {
                    return Self::InvalidMessage;
                }

                let waku_message = if let Ok(msg) = WakuMessageProto::decode(&message.data[..]) {
                    msg.into()
                } else {
                    return Self::InvalidMessage;
                };

                Self::Message {
                    pubsub_topic: PubsubTopic::new(message.topic.to_string()),
                    message: waku_message,
                }
            }
            gossipsub::Event::GossipsubNotSupported { peer_id } => {
                Self::WakuRelayNotSupported { peer_id }
            }
        }
    }
}
