mod pagination;
mod proto;
mod request;
mod response;

// TODO: Move this to the right crate
const MAX_PAGE_SIZE: usize = 100;

// TODO: Move this to the right crate
const MAX_WAKU_MESSAGE_SIZE: usize = 1024 * 1024; // In bytes. Corresponds to PubSub default

pub const MAX_PROTOBUF_SIZE: usize = MAX_PAGE_SIZE * MAX_WAKU_MESSAGE_SIZE + 64 * 1024; // We add a 64kB safety buffer for protocol overhead

pub use proto::waku::message::v1::*;
pub use proto::waku::store::v2beta4::*;
