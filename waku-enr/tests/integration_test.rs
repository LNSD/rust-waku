use std::net::{Ipv4Addr, Ipv6Addr};

use enr::{CombinedKey, EnrKey};
use multiaddr::Multiaddr;

use waku_enr;
use waku_enr::{Enr, EnrBuilder, Waku2Enr, WakuEnrBuilder, WakuEnrCapabilities};

///! https://rfc.vac.dev/spec/31/#many-connection-types
#[test]
fn test_build_waku_enr() {
    // Given
    let tcp: u16 = 10101;
    let udp: u16 = 20202;
    let tcp6: u16 = 30303;
    let udp6: u16 = 40404;
    let ip: Ipv4Addr = "1.2.3.4".parse().unwrap();
    let ip6: Ipv6Addr = "1234:5600:101:1::142".parse().unwrap();
    let capabilities: WakuEnrCapabilities = WakuEnrCapabilities::STORE | WakuEnrCapabilities::RELAY;
    let multiaddrs: Vec<Multiaddr> = vec![
        "/dns4/example.com/tcp/443/wss".parse().unwrap(),
        "/dns4/quic.example.com/tcp/443/quic".parse().unwrap(),
    ];

    // Signing key
    let key_secp256k1_base64 = "MaZivCR1kZsI2/1MuSw9mhnLQYqETWwjfcWpyiS20uw=";
    let mut key_secp256k1_bytes = base64::decode(key_secp256k1_base64).unwrap();
    let key = CombinedKey::secp256k1_from_bytes(&mut key_secp256k1_bytes).unwrap();

    // When
    let enr = EnrBuilder::new("v4")
        .tcp4(tcp)
        .udp4(udp)
        .tcp6(tcp6)
        .udp6(udp6)
        .ip4(ip)
        .ip6(ip6)
        .multiaddrs(multiaddrs)
        .waku2(capabilities)
        .build(&key);

    // Then
    assert!(enr.is_ok());
}

///! https://rfc.vac.dev/spec/31/#many-connection-types
#[test]
fn test_decode_waku_enr() {
    // Given
    // Expected values
    let expected_tcp: u16 = 10101;
    let expected_udp: u16 = 20202;
    let expected_tcp6: u16 = 30303;
    let expected_udp6: u16 = 40404;
    let expected_ip: Ipv4Addr = "1.2.3.4".parse().unwrap();
    let expected_ip6: Ipv6Addr = "1234:5600:101:1::142".parse().unwrap();
    let expected_capabilities: WakuEnrCapabilities =
        WakuEnrCapabilities::STORE | WakuEnrCapabilities::RELAY;
    let expected_multiaddrs: Vec<Multiaddr> = vec![
        "/dns4/example.com/tcp/443/wss".parse().unwrap(),
        "/dns4/quic.example.com/tcp/443/quic".parse().unwrap(),
    ];

    // Signing key
    let key_secp256k1_base64 = "MaZivCR1kZsI2/1MuSw9mhnLQYqETWwjfcWpyiS20uw=";
    let mut key_secp256k1_bytes = base64::decode(key_secp256k1_base64).unwrap();
    let expected_key = CombinedKey::secp256k1_from_bytes(&mut key_secp256k1_bytes).unwrap();

    // ENR
    let enr_base64 = "enr:-PC4QPdY95OvXxYSdzPnWTCEY3u0jr0t925ArgGDGJfsDemgMvl-PuXr23r9fJnJGncdx1yPYT7oB6OJoqsiUjSnF7sBgmlkgnY0gmlwhAECAwSDaXA2kBI0VgABAQABAAAAAAAAAUKKbXVsdGlhZGRyc60AEjYLZXhhbXBsZS5jb20GAbveAwAXNhBxdWljLmV4YW1wbGUuY29tBgG7zAOJc2VjcDI1NmsxoQL72vzMVCejPltbXNukOvJc8Mqj-IiawTVxiYY1WCRSX4N0Y3CCJ3WEdGNwNoJ2X4N1ZHCCTuqEdWRwNoKd1IV3YWt1MgM";

    // When
    let enr: Enr = enr_base64.parse().expect("valid enr string");

    let tcp = enr.tcp4();
    let udp = enr.udp4();
    let tcp6 = enr.tcp6();
    let udp6 = enr.udp6();
    let ip = enr.ip4();
    let ip6 = enr.ip6();
    let capabilities = enr.waku2();
    let multiaddrs = enr.multiaddrs();
    let public_key = enr.public_key();

    // Then
    assert_eq!(public_key, expected_key.public());
    assert!(matches!(tcp, Some(value) if value == expected_tcp));
    assert!(matches!(udp, Some(value) if value == expected_udp));
    assert!(matches!(tcp6, Some(value) if value == expected_tcp6));
    assert!(matches!(udp6, Some(value) if value == expected_udp6));
    assert!(matches!(ip, Some(value) if value == expected_ip));
    assert!(matches!(ip6, Some(value) if value == expected_ip6));
    assert!(matches!(capabilities, Some(value) if value == expected_capabilities));
    assert!(matches!(multiaddrs, Some(value) if value == expected_multiaddrs));
}
