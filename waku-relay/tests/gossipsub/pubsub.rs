use std::time::Duration;

use assert_matches::assert_matches;
use bytes::Bytes;
use futures::StreamExt;
use libp2p::identity::{Keypair, PeerId};
use libp2p::swarm::{SwarmBuilder, SwarmEvent};
use libp2p::{Multiaddr, Swarm};
use tokio::time::timeout;
use void::Void;

use waku_relay::gossipsub::{
    Behaviour, Config, ConfigBuilder, Event, IdentTopic, Message, MessageAuthenticity,
    ValidationMode,
};

use crate::testlib;
use crate::testlib::any_memory_addr;

fn new_test_node(
    keypair: &Keypair,
    privacy: MessageAuthenticity,
    config: Config,
) -> Swarm<Behaviour> {
    let peer_id = PeerId::from(keypair.public());
    let transport = testlib::test_transport(keypair).expect("create the transport");
    let behaviour = Behaviour::new(privacy, config).expect("valid gossipsub configuration");
    SwarmBuilder::with_tokio_executor(transport, behaviour, peer_id).build()
}

async fn poll_mesh(
    duration: Duration,
    swarm1: &mut Swarm<Behaviour>,
    swarm2: &mut Swarm<Behaviour>,
) {
    timeout(
        duration,
        futures::future::join(testlib::swarm::poll(swarm1), testlib::swarm::poll(swarm2)),
    )
    .await
    .expect_err("timeout to be reached");
}

async fn wait_for_start_listening(
    publisher: &mut Swarm<Behaviour>,
    subscriber: &mut Swarm<Behaviour>,
) -> (Multiaddr, Multiaddr) {
    tokio::join!(
        testlib::swarm::wait_for_new_listen_addr(publisher),
        testlib::swarm::wait_for_new_listen_addr(subscriber)
    )
}

async fn wait_for_connection_establishment(
    dialer: &mut Swarm<Behaviour>,
    receiver: &mut Swarm<Behaviour>,
) {
    tokio::join!(
        testlib::swarm::wait_for_connection_established(dialer),
        testlib::swarm::wait_for_connection_established(receiver)
    );
}

async fn wait_for_message(swarm: &mut Swarm<Behaviour>) -> Vec<SwarmEvent<Event, Void>> {
    let mut events = Vec::new();

    loop {
        let event = swarm.select_next_some().await;
        events.push(event);

        if matches!(
            events.last(),
            Some(SwarmEvent::Behaviour(Event::Message { .. }))
        ) {
            break;
        }
    }

    events
}

async fn wait_mesh_message_propagation(
    duration: Duration,
    swarm1: &mut Swarm<Behaviour>,
    swarm2: &mut Swarm<Behaviour>,
) -> Vec<SwarmEvent<Event, Void>> {
    tokio::select! {
        _ = timeout(duration, testlib::swarm::poll(swarm1)) => panic!("timeout reached"),
        res = wait_for_message(swarm2) => res,
    }
}

#[tokio::test]
async fn strict_signing_and_validation() {
    testlib::init_logger();

    //// Given
    let pubsub_topic = IdentTopic::new("/waku/2/it-waku/test");
    let message_payload = Bytes::from_static(b"test-payload");

    let publisher_key = testlib::secp256k1_keypair(
        "dc404f7ed2d3cdb65b536e8d561255c84658e83775ee790ff46bf4d77690b0fe",
    );
    let subscriber_key = testlib::secp256k1_keypair(
        "9c0cd57a01ee12338915b42bf6232a386e467dcdbe172facd94e4623ffc9096c",
    );

    let pubsub_config = ConfigBuilder::default()
        .validation_mode(ValidationMode::Strict)
        .build()
        .expect("valid gossipsub configuration");

    //// Setup
    let mut publisher = new_test_node(
        &publisher_key,
        MessageAuthenticity::Signed(publisher_key.clone()),
        pubsub_config.clone(),
    );
    publisher
        .listen_on(any_memory_addr())
        .expect("listen on address");

    let mut subscriber = new_test_node(
        &subscriber_key,
        MessageAuthenticity::Signed(subscriber_key.clone()),
        pubsub_config.clone(),
    );
    subscriber
        .listen_on(any_memory_addr())
        .expect("listen on address");

    let (publisher_addr, _subscriber_addr) = timeout(
        Duration::from_secs(5),
        wait_for_start_listening(&mut publisher, &mut subscriber),
    )
    .await
    .expect("listening to start");

    // Subscribe to the topic
    publisher
        .behaviour_mut()
        .subscribe(&pubsub_topic)
        .expect("subscribe to topic");
    subscriber
        .behaviour_mut()
        .subscribe(&pubsub_topic)
        .expect("subscribe to topic");

    // Dial the publisher node
    subscriber.dial(publisher_addr).expect("dial to succeed");
    timeout(
        Duration::from_secs(5),
        wait_for_connection_establishment(&mut subscriber, &mut publisher),
    )
    .await
    .expect("subscriber to connect to publisher");

    // Wait for pub-sub network to establish
    poll_mesh(Duration::from_millis(100), &mut publisher, &mut subscriber).await;

    //// When
    publisher
        .behaviour_mut()
        .publish(pubsub_topic.clone(), message_payload.clone())
        .expect("publish the message");

    let sub_events =
        wait_mesh_message_propagation(Duration::from_millis(250), &mut publisher, &mut subscriber)
            .await;

    //// Then
    let last_event = sub_events.last().expect("at least one event");
    assert_matches!(last_event, SwarmEvent::Behaviour(Event::Message { message: Message { topic, data, source, sequence_number } , .. }) => {
        assert!(sequence_number.is_some());
        assert_matches!(source, Some(peer_id) => {
            assert_eq!(peer_id, publisher.local_peer_id());
        });
        assert_eq!(topic.to_string(), pubsub_topic.to_string());
        assert_eq!(data[..], message_payload[..]);
    });
}

#[tokio::test]
async fn random_author_signing_permissive_validation() {
    testlib::init_logger();

    //// Given
    let pubsub_topic = IdentTopic::new("/waku/2/it-waku/test");
    let message_payload = Bytes::from_static(b"test-payload");

    let publisher_key = testlib::secp256k1_keypair(
        "dc404f7ed2d3cdb65b536e8d561255c84658e83775ee790ff46bf4d77690b0fe",
    );
    let subscriber_key = testlib::secp256k1_keypair(
        "9c0cd57a01ee12338915b42bf6232a386e467dcdbe172facd94e4623ffc9096c",
    );

    let pubsub_config = ConfigBuilder::default()
        .validation_mode(ValidationMode::Permissive)
        .build()
        .expect("valid gossipsub configuration");

    //// Setup
    let mut publisher = new_test_node(
        &publisher_key,
        MessageAuthenticity::RandomAuthor,
        pubsub_config.clone(),
    );
    publisher
        .listen_on(any_memory_addr())
        .expect("listen on address");

    let mut subscriber = new_test_node(
        &subscriber_key,
        MessageAuthenticity::RandomAuthor,
        pubsub_config.clone(),
    );
    subscriber
        .listen_on(any_memory_addr())
        .expect("listen on address");

    let (publisher_addr, _subscriber_addr) = timeout(
        Duration::from_secs(5),
        wait_for_start_listening(&mut publisher, &mut subscriber),
    )
    .await
    .expect("listening to start");

    // Subscribe to the topic
    publisher
        .behaviour_mut()
        .subscribe(&pubsub_topic)
        .expect("subscribe to topic");
    subscriber
        .behaviour_mut()
        .subscribe(&pubsub_topic)
        .expect("subscribe to topic");

    // Dial the publisher node
    subscriber.dial(publisher_addr).expect("dial to succeed");
    timeout(
        Duration::from_secs(5),
        wait_for_connection_establishment(&mut subscriber, &mut publisher),
    )
    .await
    .expect("subscriber to connect to publisher");

    // Wait for pub-sub network to establish
    poll_mesh(Duration::from_millis(100), &mut publisher, &mut subscriber).await;

    //// When
    publisher
        .behaviour_mut()
        .publish(pubsub_topic.clone(), message_payload.clone())
        .expect("publish the message");

    let sub_events =
        wait_mesh_message_propagation(Duration::from_millis(250), &mut publisher, &mut subscriber)
            .await;

    //// Then
    let last_event = sub_events.last().expect("at least one event");
    assert_matches!(last_event, SwarmEvent::Behaviour(Event::Message { message: Message { topic, data, source, sequence_number } , .. }) => {
        assert!(source.is_some());
        assert!(sequence_number.is_some());
        assert_eq!(topic.to_string(), pubsub_topic.to_string());
        assert_eq!(data[..], message_payload[..]);
    });
}

#[tokio::test]
async fn anonymous_signing_and_validation() {
    testlib::init_logger();

    //// Given
    let pubsub_topic = IdentTopic::new("/waku/2/it-waku/test");
    let message_payload = Bytes::from_static(b"test-payload");

    let publisher_key = testlib::secp256k1_keypair(
        "dc404f7ed2d3cdb65b536e8d561255c84658e83775ee790ff46bf4d77690b0fe",
    );
    let subscriber_key = testlib::secp256k1_keypair(
        "9c0cd57a01ee12338915b42bf6232a386e467dcdbe172facd94e4623ffc9096c",
    );

    let pubsub_config = ConfigBuilder::default()
        .validation_mode(ValidationMode::Anonymous) // StrictNoSign
        .build()
        .expect("valid gossipsub configuration");

    //// Setup
    let mut publisher = new_test_node(
        &publisher_key,
        MessageAuthenticity::Anonymous,
        pubsub_config.clone(),
    );
    publisher
        .listen_on(any_memory_addr())
        .expect("listen on address");

    let mut subscriber = new_test_node(
        &subscriber_key,
        MessageAuthenticity::Anonymous,
        pubsub_config.clone(),
    );
    subscriber
        .listen_on(any_memory_addr())
        .expect("listen on address");

    let (publisher_addr, _subscriber_addr) = timeout(
        Duration::from_secs(5),
        wait_for_start_listening(&mut publisher, &mut subscriber),
    )
    .await
    .expect("listening to start");

    // Subscribe to the topic
    publisher
        .behaviour_mut()
        .subscribe(&pubsub_topic)
        .expect("subscribe to topic");
    subscriber
        .behaviour_mut()
        .subscribe(&pubsub_topic)
        .expect("subscribe to topic");

    // Dial the publisher node
    subscriber.dial(publisher_addr).expect("dial to succeed");
    timeout(
        Duration::from_secs(5),
        wait_for_connection_establishment(&mut subscriber, &mut publisher),
    )
    .await
    .expect("subscriber to connect to publisher");

    // Wait for pub-sub network to establish
    poll_mesh(Duration::from_millis(100), &mut publisher, &mut subscriber).await;

    //// When
    publisher
        .behaviour_mut()
        .publish(pubsub_topic.clone(), message_payload.clone())
        .expect("publish the message");

    let sub_events =
        wait_mesh_message_propagation(Duration::from_millis(250), &mut publisher, &mut subscriber)
            .await;

    //// Then
    let last_event = sub_events.last().expect("at least one event");
    assert_matches!(last_event, SwarmEvent::Behaviour(Event::Message { message: Message { topic, data, source, sequence_number } , .. }) => {
        assert!(sequence_number.is_none());
        assert!(source.is_none());
        assert_eq!(topic.to_string(), pubsub_topic.to_string());
        assert_eq!(data[..], message_payload[..]);
    });
}
