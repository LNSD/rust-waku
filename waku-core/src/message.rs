use byte_unit::MEBIBYTE;

pub use message::*;
pub use traits::*;

mod message;
pub mod proto;
mod traits;

pub const MAX_WAKU_MESSAGE_SIZE: usize = 1 * MEBIBYTE as usize;
