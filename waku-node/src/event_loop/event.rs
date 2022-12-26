use strum_macros::Display;

use waku_message::{PubsubTopic, WakuMessage};

#[derive(Debug, Display)]
pub enum Event {
    WakuRelayMessage {
        pubsub_topic: PubsubTopic,
        message: WakuMessage,
    },
}
