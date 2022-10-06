mod proto;
mod waku_message;

pub use self::proto::waku::message::v1::*;
pub use self::waku_message::*;

pub const MAX_WAKU_MESSAGE_SIZE: usize = 1024 * 1024; // In bytes. Corresponds to PubSub default
