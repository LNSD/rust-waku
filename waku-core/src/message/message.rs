use std::fmt::{Debug, Formatter};

use bytes::Bytes;

use crate::content_topic::ContentTopic;

#[derive(Clone, Eq, PartialEq)]
pub struct WakuMessage {
    pub payload: Bytes,
    pub content_topic: ContentTopic,
    pub meta: Option<Bytes>,
    pub ephemeral: bool,
}

impl Debug for WakuMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let payload_fmt = match self.payload.get(0..32) {
            Some(slice) => format!("{}…", hex::encode(slice)),
            None => hex::encode(&self.payload[..]),
        };
        let meta_fmt = &self.meta.clone().map_or("None".to_string(), hex::encode);

        f.debug_struct("WakuMessage")
            .field("content_topic", &self.content_topic)
            .field("meta", &meta_fmt)
            .field("payload", &payload_fmt)
            .field("ephemeral", &self.ephemeral)
            .finish()
    }
}
