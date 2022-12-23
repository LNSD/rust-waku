use std::time::Duration;

use libp2p::{core, dns, mplex, noise, PeerId, tcp, Transport, yamux};
use libp2p::identity::Keypair;

// create the libp2p transport for the node
pub fn create_transport(
    keypair: &Keypair,
) -> std::io::Result<core::transport::Boxed<(PeerId, core::muxing::StreamMuxerBox)>> {
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
