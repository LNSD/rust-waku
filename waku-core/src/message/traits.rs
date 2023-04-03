use crate::message::proto::waku::message::v1::WakuMessage as WakuMessageProto;
use crate::message::WakuMessage;

impl From<WakuMessageProto> for WakuMessage {
    fn from(proto: WakuMessageProto) -> Self {
        Self {
            payload: proto.payload,
            content_topic: proto.content_topic.into(),
            meta: proto.meta,
            ephemeral: proto.ephemeral.unwrap_or(false),
        }
    }
}

impl From<WakuMessage> for WakuMessageProto {
    fn from(message: WakuMessage) -> Self {
        WakuMessageProto {
            payload: message.payload,
            content_topic: message.content_topic.to_string(),
            version: None,   // Deprecated
            timestamp: None, // Deprecated
            meta: message.meta,
            ephemeral: Some(message.ephemeral),
        }
    }
}
