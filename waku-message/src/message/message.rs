use bytes::Bytes;

use crate::content_topic::ContentTopic;

#[derive(Debug, Clone)]
pub struct WakuMessage {
    pub payload: Bytes,
    pub content_topic: ContentTopic,
    pub version: u32,
    pub timestamp: Option<i64>,
    pub ephemeral: bool,
}
