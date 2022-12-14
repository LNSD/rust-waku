//! Waku v2 ENR (EIP-778) collection of functions and an extension trait.
//! RFC 31/WAKU2-ENR: https://rfc.vac.dev/spec/31/

pub use crate::capabilities::*;
pub use crate::enr_ext::*;
pub use enr;

mod capabilities;
mod enr_ext;
mod multiaddrs;
