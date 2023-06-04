use std::time::Duration;

use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport::{Boxed, MemoryTransport, Transport};
use libp2p::core::upgrade::Version;
use libp2p::identity::{Keypair, PeerId};
use libp2p::{noise, yamux, Multiaddr};

/// Type alias for libp2p transport
pub type P2PTransport = (PeerId, StreamMuxerBox);
/// Type alias for boxed libp2p transport
pub type BoxedP2PTransport = Boxed<P2PTransport>;

/// Any memory address (for testing)
pub fn any_memory_addr() -> Multiaddr {
    "/memory/0".parse().unwrap()
}

/// In memory transport
pub fn test_transport(keypair: &Keypair) -> std::io::Result<BoxedP2PTransport> {
    let transport = MemoryTransport::default();

    Ok(transport
        .upgrade(Version::V1)
        .authenticate(noise::Config::new(keypair).unwrap())
        .multiplex(yamux::Config::default())
        .timeout(Duration::from_secs(20))
        .boxed())
}
