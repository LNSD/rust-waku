use std::time::Duration;

use libp2p::{core, dns, noise, PeerId, tcp, Transport, yamux};
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport;
use libp2p::identity::Keypair;
use libp2p_mplex as mplex;

/// Type alias for libp2p transport
pub type P2PTransport = (PeerId, StreamMuxerBox);
/// Type alias for boxed libp2p transport
pub type BoxedP2PTransport = transport::Boxed<P2PTransport>;

// create the libp2p transport for the node
pub fn default_transport(keypair: &Keypair) -> std::io::Result<BoxedP2PTransport> {
    let transport = {
        dns::TokioDnsConfig::system(tcp::tokio::Transport::new(
            tcp::Config::default().nodelay(true),
        ))?
    };

    Ok(transport
        .upgrade(core::upgrade::Version::V1)
        .authenticate(noise::Config::new(keypair).unwrap())
        .multiplex(core::upgrade::SelectUpgrade::new(
            yamux::Config::default(),
            mplex::MplexConfig::default(),
        ))
        .timeout(Duration::from_secs(20))
        .boxed())
}

/// In memory transport
pub fn memory_transport(keypair: &Keypair) -> std::io::Result<BoxedP2PTransport> {
    let transport = transport::MemoryTransport::default();

    Ok(transport
        .upgrade(core::upgrade::Version::V1)
        .authenticate(noise::Config::new(keypair).unwrap())
        .multiplex(core::upgrade::SelectUpgrade::new(
            yamux::Config::default(),
            mplex::MplexConfig::default(),
        ))
        .timeout(Duration::from_secs(20))
        .boxed())
}
