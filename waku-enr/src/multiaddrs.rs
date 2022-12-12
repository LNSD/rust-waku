use anyhow::anyhow;
use multiaddr::Multiaddr;

pub fn encode(multiaddrs: &[Multiaddr]) -> Vec<u8> {
    let mut buffer: Vec<u8> = Vec::new();
    for addr in multiaddrs {
        let mut addr_bytes = addr.to_vec();
        let length_prefix: [u8; 2] = u16::to_be_bytes(addr_bytes.len() as u16);

        buffer.extend_from_slice(&length_prefix);
        buffer.append(&mut addr_bytes);
    }
    buffer
}

pub fn decode(data: &[u8]) -> anyhow::Result<Vec<Multiaddr>> {
    let mut buffer = Vec::from(data);
    let mut multiaddrs: Vec<Multiaddr> = Vec::new();

    while buffer.len() > 2 {
        let length_prefix: [u8; 2] = {
            let prefix_bytes = buffer.drain(..2).collect::<Vec<u8>>();
            prefix_bytes.try_into().expect("2 bytes slice")
        };
        let length = u16::from_be_bytes(length_prefix) as usize;
        if length > buffer.len() {
            return Err(anyhow!("not enough bytes"));
        }

        let addr_bytes = buffer.drain(..length).collect::<Vec<u8>>();
        let addr: Multiaddr = addr_bytes.try_into()?;
        multiaddrs.push(addr);
    }

    Ok(multiaddrs)
}

#[cfg(test)]
mod tests {
    use multiaddr::Multiaddr;

    use super::{decode, encode};

    #[test]
    fn test_multiaddrs_codec() {
        // Given
        let multiaddrs: Vec<Multiaddr> = vec![
            "/dns4/example.com/tcp/443/wss".parse().unwrap(),
            "/dns4/quic.example.com/tcp/443/quic".parse().unwrap(),
            "/ip4/7.7.7.7/tcp/3003/p2p/QmYyQSo1c1Ym7orWxLYvCrM2EmxFTANf8wXmmE7DWjhx5N/p2p-circuit/p2p/QmUWYRp3mkQUUVeyGVjcM1fC7kbVxmDieGpGsQzopXivyk"
                .parse()
                .unwrap(),
        ];

        // When
        let encoded = encode(&multiaddrs);
        let decoded = decode(&encoded);

        // Then
        assert!(matches!(decoded, Ok(addrs) if addrs == multiaddrs));
    }
}
