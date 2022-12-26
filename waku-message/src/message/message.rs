use std::fmt::{Debug, Formatter};

use bytes::Bytes;

use crate::content_topic::ContentTopic;

#[derive(Clone, Eq, PartialEq)]
pub struct WakuMessage {
    pub payload: Bytes,
    pub content_topic: ContentTopic,
    pub version: u32,
    pub timestamp: Option<i64>,
    pub ephemeral: bool,
}

impl Debug for WakuMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let payload_slice = self.payload.get(0..32).unwrap_or(&self.payload[..]);
        f.debug_struct("WakuMessage")
            .field("content_topic", &self.content_topic)
            .field("version", &self.version)
            .field("timestamp", &self.timestamp)
            .field("ephemeral", &self.ephemeral)
            .field(
                "payload",
                &format_args!("Bytes(0x{}…)", hex::encode(payload_slice)),
            )
            .finish()
    }
}
