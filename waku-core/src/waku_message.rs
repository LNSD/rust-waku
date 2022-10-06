use bytes::Bytes;

#[derive(Debug, Clone)]
pub struct WakuMessage {
    pub content_topic: String,
    pub timestamp: Option<i64>,
    pub version: u32,
    pub payload: Bytes,
    pub ephemeral: bool,
    // TODO: add support for RLN proof
}