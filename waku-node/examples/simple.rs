use anyhow::anyhow;
use log::LevelFilter;

use waku_node::Node;
use waku_node::NodeConfigBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::formatted_builder()
        .filter_level(LevelFilter::Info)
        .format_timestamp_millis()
        .init();

    // Init -----
    let mut key_raw =
        hex::decode("2a6ecd4041f9903e6d57fd5841bc89ee40606e78e2be4202fa7d32485b41cb8c")?;
    let config = NodeConfigBuilder::new()
        .keypair_from_secp256k1(&mut key_raw)?
        .build();

    let node = Node::new(config.clone())?;

    // Setup -----
    let addr = format!("/ip4/{}/tcp/{}", &config.tcp_ipaddr, &config.tcp_port);
    if let Ok(listen_addr) = addr.parse() {
        node.switch_listen_on(listen_addr).await?;
    } else {
        return Err(anyhow!("invalid listen multiaddr format: {addr:?}"));
    }

    loop {}
}
