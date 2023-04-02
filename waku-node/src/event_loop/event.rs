use strum_macros::Display;

use waku_core::message::WakuMessage;
use waku_core::pubsub_topic::PubsubTopic;

#[derive(Debug, Display)]
pub enum Event {
    WakuRelayMessage {
        pubsub_topic: PubsubTopic,
        message: WakuMessage,
    },
}
