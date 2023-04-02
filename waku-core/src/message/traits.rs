use crate::message::WakuMessage;
use crate::proto::waku::message::v1::WakuMessage as WakuMessageProto;

impl From<WakuMessageProto> for WakuMessage {
    fn from(proto: WakuMessageProto) -> Self {
        Self {
            payload: proto.payload,
            content_topic: proto.content_topic.into(),
            version: proto.version.unwrap_or(0),
            timestamp: proto.timestamp,
            ephemeral: proto.ephemeral.unwrap_or(false),
        }
    }
}

impl Into<WakuMessageProto> for WakuMessage {
    fn into(self) -> WakuMessageProto {
        WakuMessageProto {
            payload: self.payload.clone(),
            content_topic: self.content_topic.into(),
            version: Some(self.version),
            timestamp: self.timestamp,
            ephemeral: Some(self.ephemeral),
        }
    }
}
