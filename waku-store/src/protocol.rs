use libp2p::core::upgrade::ProtocolName;

const PROTOCOL_NAME: &'static [u8] = b"/vac/waku/store/2.0.0-beta4";

#[derive(Debug, Clone)]
pub struct WakuStoreProtocol();

impl ProtocolName for WakuStoreProtocol {
    fn protocol_name(&self) -> &[u8] {
        PROTOCOL_NAME
    }
}
