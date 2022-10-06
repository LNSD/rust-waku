use crate::proto::proto;
use crate::waku_message::WakuMessage;

impl From<proto::waku::message::v1::WakuMessage> for WakuMessage {
    fn from(rpc: proto::waku::message::v1::WakuMessage) -> Self {
        WakuMessage {
            content_topic: rpc.content_topic.clone(),
            timestamp: rpc.timestamp.clone(),
            version: rpc.version,
            payload: rpc.payload.clone(),
            ephemeral: rpc.ephemeral.unwrap_or(false),
        }
    }
}
