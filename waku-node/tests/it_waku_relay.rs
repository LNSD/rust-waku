use std::time::Duration;

use bytes::Bytes;
use libp2p::Multiaddr;
use tokio::time::sleep;

use waku_message::{PubsubTopic, WakuMessage};
use waku_node::{Event, Node, NodeConfigBuilder};

fn new_node(key: &str) -> Node {
    let mut key_raw = hex::decode(key).expect("key to be valid");

    let config = NodeConfigBuilder::new()
        .keypair_from_secp256k1(&mut key_raw)
        .unwrap()
        .keepalive(true)
        .with_waku_relay(Default::default())
        .build();

    Node::new(config).expect("node creation to succeed")
}

#[tokio::test]
async fn it_publish_and_subscribe() {
    //// Setup
    let publisher_key = "dc404f7ed2d3cdb65b536e8d561255c84658e83775ee790ff46bf4d77690b0fe";
    let publisher_addr: Multiaddr = "/ip4/127.0.0.1/tcp/23000".parse().unwrap();
    let publisher = new_node(publisher_key);
    publisher
        .switch_listen_on(&publisher_addr)
        .await
        .expect("listen on address");

    let subscriber_key = "9c0cd57a01ee12338915b42bf6232a386e467dcdbe172facd94e4623ffc9096c";
    let subscriber_addr: Multiaddr = "/ip4/127.0.0.1/tcp/23002".parse().unwrap();
    let mut subscriber = new_node(subscriber_key);
    subscriber
        .switch_listen_on(&subscriber_addr)
        .await
        .expect("listen on address");

    // Dial the publisher node
    subscriber
        .switch_dial(&publisher_addr)
        .await
        .expect("dial to succeed");

    // Subscribe to node
    let pubsub_topic: PubsubTopic = "/waku/2/it-waku/test".parse().unwrap();
    publisher
        .relay_subscribe(&pubsub_topic)
        .await
        .expect("subscribe to topic");
    subscriber
        .relay_subscribe(&pubsub_topic)
        .await
        .expect("subscribe to topic");

    // Wait for pub-sub network to establish
    sleep(Duration::from_millis(100)).await;

    //// Given
    let message = WakuMessage {
        payload: Bytes::from_static(b"TEST"),
        content_topic: "/test/v1/it/text".parse().unwrap(),
        version: 0,
        timestamp: None,
        ephemeral: false,
    };

    //// When
    publisher
        .relay_publish(&pubsub_topic, message.clone())
        .await
        .expect("publish the message");
    let event = subscriber.recv_event().await;

    //// Then
    assert!(matches!(event, Some(Event::WakuRelayMessage { .. })));
    if let Some(Event::WakuRelayMessage {
                    pubsub_topic: topic,
                    message: msg,
                }) = event
    {
        assert_eq!(topic, pubsub_topic);
        assert_eq!(msg, message);
    }
}
