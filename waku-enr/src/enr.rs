use enr::CombinedKey;
use multiaddr::Multiaddr;

use crate::capabilities::WakuEnrCapabilities;
use crate::multiaddrs;

pub type Enr = enr::Enr<CombinedKey>;
pub type EnrBuilder = enr::EnrBuilder<CombinedKey>;

/// The ENR field specifying the node multiaddrs.
pub const WAKU2_MULTIADDR_ENR_KEY: &str = "multiaddrs";
/// The ENR field specifying the node Waku v2 capabilities.
pub const WAKU2_CAPABILITIES_ENR_KEY: &str = "waku2";

/// Extension trait for Waku v2 ENRs
pub trait Waku2Enr {
    /// The multiaddrs field associated with the ENR.
    fn multiaddrs(&self) -> Option<Vec<Multiaddr>>;

    /// The waku node capabilities bitfield associated with the ENR.
    fn waku2(&self) -> Option<WakuEnrCapabilities>;
}

impl Waku2Enr for Enr {
    fn multiaddrs(&self) -> Option<Vec<Multiaddr>> {
        if let Some(multiaddrs_bytes) = self.get(WAKU2_MULTIADDR_ENR_KEY) {
            return multiaddrs::decode(multiaddrs_bytes).ok();
        }
        None
    }

    fn waku2(&self) -> Option<WakuEnrCapabilities> {
        if let Some(bitfield) = self.get(WAKU2_CAPABILITIES_ENR_KEY) {
            return match bitfield.len() {
                1 => WakuEnrCapabilities::from_bits(bitfield[0]),
                _ => None,
            };
        }
        None
    }
}

pub trait WakuEnrBuilder {
    fn multiaddrs(&mut self, multiaddrs: Vec<Multiaddr>) -> &mut Self;

    fn waku2(&mut self, capabilities: WakuEnrCapabilities) -> &mut Self;
}

impl WakuEnrBuilder for EnrBuilder {
    /// Adds a Waku `multiaddr` field to the EnrBuilder.
    fn multiaddrs(&mut self, addrs: Vec<Multiaddr>) -> &mut Self {
        let multiaddrs = multiaddrs::encode(&addrs);
        self.add_value(WAKU2_MULTIADDR_ENR_KEY, &multiaddrs);
        self
    }

    /// Adds a Waku `waku2` capabilities bitfield to the EnrBuilder.
    fn waku2(&mut self, cap: WakuEnrCapabilities) -> &mut Self {
        self.add_value(WAKU2_CAPABILITIES_ENR_KEY, &[cap.bits()]);
        self
    }
}
