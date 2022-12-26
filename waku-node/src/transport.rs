use std::time::Duration;

use libp2p::{core, dns, mplex, noise, PeerId, tcp, Transport, yamux};
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport;
use libp2p::identity::Keypair;

/// Type alias for libp2p transport
pub type P2PTransport = (PeerId, StreamMuxerBox);
/// Type alias for boxed libp2p transport
pub type BoxedP2PTransport = transport::Boxed<P2PTransport>;

// create the libp2p transport for the node
pub fn default_transport(keypair: &Keypair) -> std::io::Result<BoxedP2PTransport> {
    let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
        .into_authentic(keypair)
        .expect("signing libp2p-noise static dh keypair failed");
    let noise_config = noise::NoiseConfig::xx(noise_keys);

    let transport = {
        let dns_tcp = dns::TokioDnsConfig::system(tcp::tokio::Transport::new(
            tcp::Config::default().nodelay(true),
        ))?;

        dns_tcp
    };

    Ok(transport
        .upgrade(core::upgrade::Version::V1)
        .authenticate(noise_config.into_authenticated())
        .multiplex(core::upgrade::SelectUpgrade::new(
            yamux::YamuxConfig::default(),
            mplex::MplexConfig::default(),
        ))
        .timeout(Duration::from_secs(20))
        .boxed())
}

/// In memory transport
pub fn memory_transport(keypair: &Keypair) -> std::io::Result<BoxedP2PTransport> {
    let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
        .into_authentic(keypair)
        .expect("signing libp2p-noise static dh keypair failed");
    let noise_config = noise::NoiseConfig::xx(noise_keys);

    let transport = transport::MemoryTransport::default();

    Ok(transport
        .upgrade(core::upgrade::Version::V1)
        .authenticate(noise_config.into_authenticated())
        .multiplex(core::upgrade::SelectUpgrade::new(
            yamux::YamuxConfig::default(),
            mplex::MplexConfig::default(),
        ))
        .timeout(Duration::from_secs(20))
        .boxed())
}
