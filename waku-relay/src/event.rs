use libp2p::gossipsub::GossipsubEvent;
use libp2p::PeerId;
use prost::Message;
use strum_macros::Display;

use waku_message::{MAX_WAKU_MESSAGE_SIZE, PubsubTopic, WakuMessage};
use waku_message::proto::waku::message::v1::WakuMessage as WakuMessageProto;

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

impl From<GossipsubEvent> for Event {
    fn from(event: GossipsubEvent) -> Self {
        match event {
            GossipsubEvent::Subscribed { peer_id, topic } => Self::Subscribed {
                peer_id,
                pubsub_topic: PubsubTopic::new(topic.into_string()),
            },
            GossipsubEvent::Unsubscribed { peer_id, topic } => Self::Unsubscribed {
                peer_id,
                pubsub_topic: PubsubTopic::new(topic.into_string()),
            },
            GossipsubEvent::Message { message, .. } => {
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
            GossipsubEvent::GossipsubNotSupported { peer_id } => {
                Self::WakuRelayNotSupported { peer_id }
            }
        }
    }
}
