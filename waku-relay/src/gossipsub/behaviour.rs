// Copyright 2020 Sigma Prime Pty Ltd.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use std::cmp::max;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt;
use std::net::IpAddr;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::StreamExt;
use futures_ticker::Ticker;
use instant::Instant;
use libp2p::core::{multiaddr::Protocol::Ip4, multiaddr::Protocol::Ip6, Endpoint, Multiaddr};
use libp2p::identity::PeerId;
use libp2p::swarm::{
    behaviour::{AddressChange, ConnectionClosed, ConnectionEstablished, FromSwarm},
    dial_opts::DialOpts,
    ConnectionDenied, ConnectionId, NetworkBehaviour, NotifyHandler, PollParameters, THandler,
    THandlerInEvent, THandlerOutEvent, ToSwarm,
};
use log::{debug, error, trace, warn};
use prometheus_client::registry::Registry;
use prost::Message as _;
use rand::{seq::SliceRandom, thread_rng};

use crate::gossipsub::backoff::BackoffStorage;
use crate::gossipsub::config::{Config, MessageAuthenticity, ValidationMode};
use crate::gossipsub::connection_manager::ConnectionManager;
use crate::gossipsub::error::{
    MessageValidationError as ValidationError, PublishError, SubscriptionError,
};
use crate::gossipsub::event::Event;
use crate::gossipsub::handler::{Handler, HandlerEvent, HandlerIn};
use crate::gossipsub::heartbeat::Heartbeat;
use crate::gossipsub::mcache::{CachedMessage, MessageCache};
use crate::gossipsub::message_id::{FastMessageId, MessageId};
use crate::gossipsub::metrics::{
    Churn, Config as MetricsConfig, GossipsubMetrics, Inclusion, Metrics, NoopMetrics, Penalty,
};
use crate::gossipsub::peer_score::{
    GossipsubPeerScoreService, NoopPeerScoreService, PeerScore, PeerScoreService, RejectReason,
};
use crate::gossipsub::peer_score::{PeerScoreParams, PeerScoreThresholds, TopicScoreParams};
use crate::gossipsub::protocol::ProtocolUpgrade;
use crate::gossipsub::rpc::{fragment_rpc_message, validate_message_proto, MessageRpc, RpcProto};
use crate::gossipsub::seq_no::{
    LinearSequenceNumber, MessageSeqNumberGenerator, RandomSequenceNumber,
};
use crate::gossipsub::signing::{
    AnonymousMessageValidator, AuthorOnlySigner, Libp2pSigner, MessageSigner, MessageValidator,
    NoopMessageValidator, NoopSigner, PermissiveMessageValidator, RandomAuthorSigner,
    StrictMessageValidator,
};
use crate::gossipsub::subscription_filter::{AllowAllSubscriptionFilter, TopicSubscriptionFilter};
use crate::gossipsub::time_cache::{DuplicateCache, TimeCache};
use crate::gossipsub::topic::{Hasher, Topic, TopicHash};
use crate::gossipsub::transform::{DataTransform, IdentityTransform};
use crate::gossipsub::types::{
    ControlAction, Message, MessageAcceptance, PeerInfo, PeerKind, RawMessage, Rpc, Subscription,
    SubscriptionAction,
};

fn get_ip_addr(addr: &Multiaddr) -> Option<IpAddr> {
    addr.iter().find_map(|p| match p {
        Ip4(addr) => Some(IpAddr::V4(addr)),
        Ip6(addr) => Some(IpAddr::V6(addr)),
        _ => None,
    })
}

/// This is called when peers are added to any mesh. It checks if the peer existed
/// in any other mesh. If this is the first mesh they have joined, it queues a message to notify
/// the appropriate connection handler to maintain a connection.
fn peer_added_to_mesh(
    peer_id: PeerId,
    new_topics: Vec<&TopicHash>,
    mesh: &HashMap<TopicHash, BTreeSet<PeerId>>,
    known_topics: Option<&BTreeSet<TopicHash>>,
    events: &mut VecDeque<ToSwarm<Event, HandlerIn>>,
    connections: &ConnectionManager,
) {
    // Ensure there is an active connection
    let connection_id = {
        let conn = connections.connections(&peer_id).next();
        assert!(conn.is_some(), "Must have at least one connection");
        conn.unwrap()
    };

    if let Some(topics) = known_topics {
        for topic in topics {
            if !new_topics.contains(&topic) {
                if let Some(mesh_peers) = mesh.get(topic) {
                    if mesh_peers.contains(&peer_id) {
                        // the peer is already in a mesh for another topic
                        return;
                    }
                }
            }
        }
    }
    // This is the first mesh the peer has joined, inform the handler
    events.push_back(ToSwarm::NotifyHandler {
        peer_id,
        event: HandlerIn::JoinedMesh,
        handler: NotifyHandler::One(*connection_id),
    });
}

/// This is called when peers are removed from a mesh. It checks if the peer exists
/// in any other mesh. If this is the last mesh they have joined, we return true, in order to
/// notify the handler to no longer maintain a connection.
fn peer_removed_from_mesh(
    peer_id: PeerId,
    old_topic: &TopicHash,
    mesh: &HashMap<TopicHash, BTreeSet<PeerId>>,
    known_topics: Option<&BTreeSet<TopicHash>>,
    events: &mut VecDeque<ToSwarm<Event, HandlerIn>>,
    connections: &ConnectionManager,
) {
    // Ensure there is an active connection
    let connection_id = connections
        .connections(&peer_id)
        .next()
        .expect("There should be at least one connection to a peer.");

    if let Some(topics) = known_topics {
        for topic in topics {
            if topic != old_topic {
                if let Some(mesh_peers) = mesh.get(topic) {
                    if mesh_peers.contains(&peer_id) {
                        // the peer exists in another mesh still
                        return;
                    }
                }
            }
        }
    }
    // The peer is not in any other mesh, inform the handler
    events.push_back(ToSwarm::NotifyHandler {
        peer_id,
        event: HandlerIn::LeftMesh,
        handler: NotifyHandler::One(*connection_id),
    });
}

/// Helper function to get a subset of random gossipsub peers for a `topic_hash`
/// filtered by the function `f`. The number of peers to get equals the output of `n_map`
/// that gets as input the number of filtered peers.
fn get_random_peers_dynamic(
    topic_peers: &HashMap<TopicHash, BTreeSet<PeerId>>,
    connected_peers: &ConnectionManager,
    topic_hash: &TopicHash,
    // maps the number of total peers to the number of selected peers
    n_map: impl Fn(usize) -> usize,
    mut f: impl FnMut(&PeerId) -> bool,
) -> BTreeSet<PeerId> {
    let mut gossip_peers = match topic_peers.get(topic_hash) {
        // if they exist, filter the peers by `f`
        Some(peer_list) => peer_list
            .iter()
            .cloned()
            .filter(|p| f(p) && connected_peers.kind(p).is_some_and(|k| k.is_gossipsub()))
            .collect(),
        None => Vec::new(),
    };

    // if we have less than needed, return them
    let n = n_map(gossip_peers.len());
    if gossip_peers.len() <= n {
        debug!("RANDOM PEERS: Got {:?} peers", gossip_peers.len());
        return gossip_peers.into_iter().collect();
    }

    // we have more peers than needed, shuffle them and return n of them
    let mut rng = thread_rng();
    gossip_peers.partial_shuffle(&mut rng, n);

    debug!("RANDOM PEERS: Got {:?} peers", n);

    gossip_peers.into_iter().take(n).collect()
}

/// Helper function to get a set of `n` random gossipsub peers for a `topic_hash`
/// filtered by the function `f`.
fn get_random_peers(
    topic_peers: &HashMap<TopicHash, BTreeSet<PeerId>>,
    connected_peers: &ConnectionManager,
    topic_hash: &TopicHash,
    n: usize,
    f: impl FnMut(&PeerId) -> bool,
) -> BTreeSet<PeerId> {
    get_random_peers_dynamic(topic_peers, connected_peers, topic_hash, |_| n, f)
}

/// Validates the combination of signing, privacy and message validation to ensure the
/// configuration will not reject published messages.
fn validate_config(
    authenticity: &MessageAuthenticity,
    validation_mode: &ValidationMode,
) -> Result<(), &'static str> {
    match validation_mode {
        ValidationMode::Anonymous => {
            if authenticity.is_signing() {
                return Err("Cannot enable message signing with an Anonymous validation mode. Consider changing either the ValidationMode or MessageAuthenticity");
            }

            if !authenticity.is_anonymous() {
                return Err("Published messages contain an author but incoming messages with an author will be rejected. Consider adjusting the validation or privacy settings in the config");
            }
        }
        ValidationMode::Strict => {
            if !authenticity.is_signing() {
                return Err(
                    "Messages will be
                published unsigned and incoming unsigned messages will be rejected. Consider adjusting
                the validation or privacy settings in the config"
                );
            }
        }
        _ => {}
    }
    Ok(())
}

/// Network behaviour that handles the gossipsub protocol.
///
/// NOTE: Initialisation requires a [`MessageAuthenticity`] and [`Config`] instance. If
/// message signing is disabled, the [`ValidationMode`] in the config should be adjusted to an
/// appropriate level to accept unsigned messages.
///
/// The DataTransform trait allows applications to optionally add extra encoding/decoding
/// functionality to the underlying messages. This is intended for custom compression algorithms.
///
/// The TopicSubscriptionFilter allows applications to implement specific filters on topics to
/// prevent unwanted messages being propagated and evaluated.
pub struct Behaviour<D = IdentityTransform, F = AllowAllSubscriptionFilter> {
    /// Configuration providing gossipsub performance parameters.
    config: Config,

    /// Events that need to be yielded to the outside when polling.
    events: VecDeque<ToSwarm<Event, HandlerIn>>,

    /// Pools non-urgent control messages between heartbeats.
    control_pool: HashMap<PeerId, Vec<ControlAction>>,

    /// An LRU Time cache for storing seen messages (based on their ID). This cache prevents
    /// duplicates from being propagated to the application and on the network.
    duplicate_cache: DuplicateCache<MessageId>,

    /// A set of connected peers, indexed by their [`PeerId`] tracking both the [`PeerKind`] and
    /// the set of [`ConnectionId`]s.
    connected_peers: ConnectionManager,

    /// A map of all connected peers - A map of topic hash to a list of gossipsub peer Ids.
    topic_peers: HashMap<TopicHash, BTreeSet<PeerId>>,

    /// A map of all connected peers to their subscribed topics.
    peer_topics: HashMap<PeerId, BTreeSet<TopicHash>>,

    /// A set of all explicit peers. These are peers that remain connected and we unconditionally
    /// forward messages to, outside of the scoring system.
    explicit_peers: HashSet<PeerId>,

    /// A list of peers that have been blacklisted by the user.
    /// Messages are not sent to and are rejected from these peers.
    blacklisted_peers: HashSet<PeerId>,

    /// Overlay network of connected peers - Maps topics to connected gossipsub peers.
    mesh: HashMap<TopicHash, BTreeSet<PeerId>>,

    /// Map of topics to list of peers that we publish to, but don't subscribe to.
    fanout: HashMap<TopicHash, BTreeSet<PeerId>>,

    /// The last publish time for fanout topics.
    fanout_last_pub: HashMap<TopicHash, Instant>,

    ///Storage for backoffs
    backoffs: BackoffStorage,

    /// Message cache for the last few heartbeats.
    mcache: MessageCache,

    /// Heartbeat interval stream.
    heartbeat: Heartbeat,

    /// We remember all peers we found through peer exchange, since those peers are not considered
    /// as safe as randomly discovered outbound peers. This behaviour diverges from the go
    /// implementation to avoid possible love bombing attacks in PX. When disconnecting peers will
    /// be removed from this list which may result in a true outbound rediscovery.
    px_peers: HashSet<PeerId>,

    /// Stores optional peer score data together with thresholds, decay interval and gossip
    /// promises.
    peer_score: Box<dyn PeerScoreService + Send>,

    /// Counts the number of `IHAVE` received from each peer since the last heartbeat.
    count_received_ihave: HashMap<PeerId, usize>,

    /// Counts the number of `IWANT` that we sent the each peer since the last heartbeat.
    count_sent_iwant: HashMap<PeerId, usize>,

    /// Keeps track of IWANT messages that we are awaiting to send.
    /// This is used to prevent sending duplicate IWANT messages for the same message.
    pending_iwant_msgs: HashSet<MessageId>,

    /// Short term cache for published message ids. This is used for penalizing peers sending
    /// our own messages back if the messages are anonymous or use a random author.
    published_message_ids: DuplicateCache<MessageId>,

    /// Short term cache for fast message ids mapping them to the real message ids
    fast_message_id_cache: TimeCache<FastMessageId, MessageId>,

    /// The filter used to handle message subscriptions.
    subscription_filter: F,

    /// A general transformation function that can be applied to data received from the wire before
    /// calculating the message-id and sending to the application. This is designed to allow the
    /// user to implement arbitrary topic-based compression algorithms.
    data_transform: D,

    /// Keep track of a set of internal metrics relating to gossipsub.
    metrics: Box<dyn Metrics + Send>,

    /// A validator for incoming messages.
    message_validator: Box<dyn MessageValidator + Send>,

    /// A generator for message sequence numbers.
    message_seqno_generator: Option<Box<dyn MessageSeqNumberGenerator + Send>>,

    /// A signer for outgoing messages.
    message_signer: Box<dyn MessageSigner + Send>,
}

impl<D, F> Behaviour<D, F>
where
    D: DataTransform + Default,
    F: TopicSubscriptionFilter + Default,
{
    /// Creates a Gossipsub [`Behaviour`] struct given a set of parameters specified via a
    /// [`Config`]. This has no subscription filter and uses no compression.
    pub fn new(privacy: MessageAuthenticity, config: Config) -> Result<Self, &'static str> {
        Self::new_with_subscription_filter_and_transform(
            privacy,
            config,
            None,
            F::default(),
            D::default(),
        )
    }

    /// Creates a Gossipsub [`Behaviour`] struct given a set of parameters specified via a
    /// [`Config`]. This has no subscription filter and uses no compression.
    /// Metrics can be evaluated by passing a reference to a [`Registry`].
    pub fn new_with_metrics(
        privacy: MessageAuthenticity,
        config: Config,
        metrics_registry: &mut Registry,
        metrics_config: MetricsConfig,
    ) -> Result<Self, &'static str> {
        Self::new_with_subscription_filter_and_transform(
            privacy,
            config,
            Some((metrics_registry, metrics_config)),
            F::default(),
            D::default(),
        )
    }
}

impl<D, F> Behaviour<D, F>
where
    D: DataTransform + Default,
    F: TopicSubscriptionFilter,
{
    /// Creates a Gossipsub [`Behaviour`] struct given a set of parameters specified via a
    /// [`Config`] and a custom subscription filter.
    pub fn new_with_subscription_filter(
        privacy: MessageAuthenticity,
        config: Config,
        metrics: Option<(&mut Registry, MetricsConfig)>,
        subscription_filter: F,
    ) -> Result<Self, &'static str> {
        Self::new_with_subscription_filter_and_transform(
            privacy,
            config,
            metrics,
            subscription_filter,
            D::default(),
        )
    }
}

impl<D, F> Behaviour<D, F>
where
    D: DataTransform,
    F: TopicSubscriptionFilter + Default,
{
    /// Creates a Gossipsub [`Behaviour`] struct given a set of parameters specified via a
    /// [`Config`] and a custom data transform.
    pub fn new_with_transform(
        privacy: MessageAuthenticity,
        config: Config,
        metrics: Option<(&mut Registry, MetricsConfig)>,
        data_transform: D,
    ) -> Result<Self, &'static str> {
        Self::new_with_subscription_filter_and_transform(
            privacy,
            config,
            metrics,
            F::default(),
            data_transform,
        )
    }
}

impl<D, F> Behaviour<D, F>
where
    D: DataTransform,
    F: TopicSubscriptionFilter,
{
    /// Creates a Gossipsub [`Behaviour`] struct given a set of parameters specified via a
    /// [`Config`] and a custom subscription filter and data transform.
    pub fn new_with_subscription_filter_and_transform(
        privacy: MessageAuthenticity,
        config: Config,
        metrics: Option<(&mut Registry, MetricsConfig)>,
        subscription_filter: F,
        data_transform: D,
    ) -> Result<Self, &'static str> {
        // Set up the router given the configuration settings.

        // We do not allow configurations where a published message would also be rejected if it
        // were received locally.
        validate_config(&privacy, config.validation_mode())?;

        let metrics: Box<dyn Metrics + Send> = match metrics {
            None => Box::new(NoopMetrics::new()),
            Some((registry, cfg)) => Box::new(GossipsubMetrics::new(registry, cfg)),
        };

        let message_validator: Box<dyn MessageValidator + Send> = match &config.validation_mode() {
            ValidationMode::Strict => Box::new(StrictMessageValidator::new()),
            ValidationMode::Permissive => Box::new(PermissiveMessageValidator::new()),
            ValidationMode::Anonymous => Box::new(AnonymousMessageValidator::new()),
            ValidationMode::None => Box::new(NoopMessageValidator::new()),
        };

        let message_seqno_generator: Option<Box<dyn MessageSeqNumberGenerator + Send>> =
            match &privacy {
                MessageAuthenticity::Signed(_) => Some(Box::new(LinearSequenceNumber::new())),
                MessageAuthenticity::Author(_) | MessageAuthenticity::RandomAuthor => {
                    Some(Box::new(RandomSequenceNumber::new()))
                }
                MessageAuthenticity::Anonymous => None,
            };

        let message_signer: Box<dyn MessageSigner + Send> = match &privacy {
            MessageAuthenticity::Signed(keypair) => Box::new(Libp2pSigner::new(keypair)),
            MessageAuthenticity::Author(peer_id) => Box::new(AuthorOnlySigner::new(*peer_id)),
            MessageAuthenticity::RandomAuthor => Box::new(RandomAuthorSigner::new()),
            MessageAuthenticity::Anonymous => Box::new(NoopSigner::new()),
        };

        Ok(Behaviour {
            metrics,
            events: VecDeque::new(),
            control_pool: HashMap::new(),
            duplicate_cache: DuplicateCache::new(config.duplicate_cache_time()),
            fast_message_id_cache: TimeCache::new(config.duplicate_cache_time()),
            topic_peers: HashMap::new(),
            peer_topics: HashMap::new(),
            explicit_peers: HashSet::new(),
            blacklisted_peers: HashSet::new(),
            mesh: HashMap::new(),
            fanout: HashMap::new(),
            fanout_last_pub: HashMap::new(),
            backoffs: BackoffStorage::new(
                &config.prune_backoff(),
                config.heartbeat_interval(),
                config.backoff_slack(),
            ),
            mcache: MessageCache::new(config.history_gossip(), config.history_length()),
            heartbeat: Heartbeat::new(
                config.heartbeat_interval(),
                config.heartbeat_initial_delay(),
            ),
            px_peers: HashSet::new(),
            peer_score: Box::new(NoopPeerScoreService::new()),
            count_received_ihave: HashMap::new(),
            count_sent_iwant: HashMap::new(),
            pending_iwant_msgs: HashSet::new(),
            connected_peers: ConnectionManager::new(),
            published_message_ids: DuplicateCache::new(config.published_message_ids_cache_time()),
            config,
            subscription_filter,
            data_transform,
            message_validator,
            message_seqno_generator,
            message_signer,
        })
    }
}

impl<D, F> Behaviour<D, F>
where
    D: DataTransform + Send + 'static,
    F: TopicSubscriptionFilter + Send + 'static,
{
    /// Lists the hashes of the topics we are currently subscribed to.
    pub fn topics(&self) -> impl Iterator<Item = &TopicHash> {
        self.mesh.keys()
    }

    /// Lists all mesh peers for a certain topic hash.
    pub fn mesh_peers(&self, topic_hash: &TopicHash) -> impl Iterator<Item = &PeerId> {
        self.mesh.get(topic_hash).into_iter().flat_map(|x| x.iter())
    }

    pub fn all_mesh_peers(&self) -> impl Iterator<Item = &PeerId> {
        let mut res = BTreeSet::new();
        for peers in self.mesh.values() {
            res.extend(peers);
        }
        res.into_iter()
    }

    /// Lists all known peers and their associated subscribed topics.
    pub fn all_peers(&self) -> impl Iterator<Item = (&PeerId, Vec<&TopicHash>)> {
        self.peer_topics
            .iter()
            .map(|(peer_id, topic_set)| (peer_id, topic_set.iter().collect()))
    }

    /// Lists all known peers and their associated protocol.
    pub fn peer_protocol(&self) -> impl Iterator<Item = (&PeerId, &PeerKind)> {
        self.connected_peers.peer_protocol()
    }

    /// Returns the gossipsub score for a given peer, if one exists.
    pub fn peer_score(&self, peer_id: &PeerId) -> Option<f64> {
        self.peer_score.peer_score(peer_id)
    }

    /// Subscribe to a topic.
    ///
    /// Returns [`Ok(true)`] if the subscription worked. Returns [`Ok(false)`] if we were already
    /// subscribed.
    pub fn subscribe<H: Hasher>(&mut self, topic: &Topic<H>) -> Result<bool, SubscriptionError> {
        debug!("Subscribing to topic: {}", topic);

        let topic_hash = topic.hash();
        if !self.subscription_filter.can_subscribe(&topic_hash) {
            return Err(SubscriptionError::NotAllowed);
        }

        if self.mesh.get(&topic_hash).is_some() {
            debug!("Topic: {} is already in the mesh.", topic);
            return Ok(false);
        }

        // send subscription request to all peers
        let peer_list = self.peer_topics.keys().cloned().collect::<Vec<_>>();
        if !peer_list.is_empty() {
            let event: RpcProto = Rpc {
                messages: Vec::new(),
                subscriptions: vec![Subscription {
                    topic_hash: topic_hash.clone(),
                    action: SubscriptionAction::Subscribe,
                }],
                control_msgs: Vec::new(),
            }
            .into();

            for peer in peer_list {
                debug!("Sending SUBSCRIBE to peer: {:?}", peer);
                self.send_rpc_message(peer, event.clone())
                    .map_err(SubscriptionError::PublishError)?;
            }
        }

        // call JOIN(topic)
        // this will add new peers to the mesh for the topic
        self.join(&topic_hash);
        debug!("Subscribed to topic: {}", topic);
        Ok(true)
    }

    /// Unsubscribes from a topic.
    ///
    /// Returns [`Ok(true)`] if we were subscribed to this topic.
    pub fn unsubscribe<H: Hasher>(&mut self, topic: &Topic<H>) -> Result<bool, PublishError> {
        debug!("Unsubscribing from topic: {}", topic);

        let topic_hash = topic.hash();
        if self.mesh.get(&topic_hash).is_none() {
            debug!("Already unsubscribed from topic: {:?}", topic_hash);
            return Ok(false);
        }

        // announce to all peers
        let peer_list = self.peer_topics.keys().cloned().collect::<Vec<_>>();
        if !peer_list.is_empty() {
            let event: RpcProto = Rpc {
                messages: Vec::new(),
                subscriptions: vec![Subscription {
                    topic_hash: topic_hash.clone(),
                    action: SubscriptionAction::Unsubscribe,
                }],
                control_msgs: Vec::new(),
            }
            .into();

            for peer in peer_list {
                debug!("Sending UNSUBSCRIBE to peer: {}", peer.to_string());
                self.send_rpc_message(peer, event.clone())?;
            }
        }

        // call LEAVE(topic)
        // this will remove the topic from the mesh
        self.leave(&topic_hash);

        debug!("Unsubscribed from topic: {:?}", topic_hash);
        Ok(true)
    }

    /// Publishes a message with multiple topics to the network.
    pub fn publish(
        &mut self,
        topic: impl Into<TopicHash>,
        data: impl Into<Vec<u8>>,
    ) -> Result<MessageId, PublishError> {
        let raw_data = data.into();
        let topic_hash = topic.into();

        // Transform the data before building a raw_message.
        let transformed_data = self
            .data_transform
            .outbound_transform(&topic_hash, raw_data.clone())?;

        let sequence_number: Option<u64> =
            self.message_seqno_generator.as_mut().map(|gen| gen.next());

        let mut message = MessageRpc::new_with_sequence_number(
            topic_hash.clone(),
            transformed_data,
            sequence_number,
        );
        self.message_signer.sign(&mut message)?;

        // calculate the message id from the un-transformed data
        let msg_id = self.config.message_id(&Message {
            source: message.source(),
            data: raw_data,
            sequence_number,
            topic: topic_hash.clone(),
        });

        let raw_message: RawMessage = message.into();
        let event: RpcProto = Rpc {
            subscriptions: Vec::new(),
            messages: vec![raw_message.clone()],
            control_msgs: Vec::new(),
        }
        .into();

        // check that the size doesn't exceed the max transmission size
        if event.encoded_len() > self.config.max_transmit_size() {
            return Err(PublishError::MessageTooLarge);
        }

        // Check the if the message has been published before
        if self.duplicate_cache.contains(&msg_id) {
            // This message has already been seen. We don't re-publish messages that have already
            // been published on the network.
            warn!(
                "Not publishing a message that has already been published. Msg-id {}",
                msg_id
            );
            return Err(PublishError::Duplicate);
        }

        trace!("Publishing message: {:?}", msg_id);

        // If we are not flood publishing forward the message to mesh peers.
        let mesh_peers_sent = !self.config.flood_publish()
            && self.forward_msg(&msg_id, raw_message.clone(), None, HashSet::new())?;

        let mut recipient_peers = HashSet::new();
        if let Some(set) = self.topic_peers.get(&topic_hash) {
            if self.config.flood_publish() {
                // Forward to all peers above score and all explicit peers
                recipient_peers.extend(
                    set.iter()
                        .filter(|p| {
                            self.explicit_peers.contains(*p)
                                || !self
                                    .peer_score
                                    .score_below_threshold(p, |ts| ts.publish_threshold)
                                    .0
                        })
                        .cloned(),
                );
            } else {
                // Explicit peers
                for peer in &self.explicit_peers {
                    if set.contains(peer) {
                        recipient_peers.insert(*peer);
                    }
                }

                // Floodsub peers
                for peer in self.connected_peers.floodsub_peers() {
                    if !self
                        .peer_score
                        .score_below_threshold(peer, |ts| ts.publish_threshold)
                        .0
                    {
                        recipient_peers.insert(*peer);
                    }
                }

                // Gossipsub peers
                if self.mesh.get(&topic_hash).is_none() {
                    debug!("Topic: {:?} not in the mesh", topic_hash);
                    // If we have fanout peers add them to the map.
                    if self.fanout.contains_key(&topic_hash) {
                        for peer in self.fanout.get(&topic_hash).expect("Topic must exist") {
                            recipient_peers.insert(*peer);
                        }
                    } else {
                        // We have no fanout peers, select mesh_n of them and add them to the fanout
                        let mesh_n = self.config.mesh_n();
                        let new_peers = get_random_peers(
                            &self.topic_peers,
                            &self.connected_peers,
                            &topic_hash,
                            mesh_n,
                            {
                                |p| {
                                    !self.explicit_peers.contains(p)
                                        && !self
                                            .peer_score
                                            .score_below_threshold(p, |pst| pst.publish_threshold)
                                            .0
                                }
                            },
                        );
                        // Add the new peers to the fanout and recipient peers
                        self.fanout.insert(topic_hash.clone(), new_peers.clone());
                        for peer in new_peers {
                            debug!("Peer added to fanout: {:?}", peer);
                            recipient_peers.insert(peer);
                        }
                    }
                    // We are publishing to fanout peers - update the time we published
                    self.fanout_last_pub
                        .insert(topic_hash.clone(), Instant::now());
                }
            }
        }

        if recipient_peers.is_empty() && !mesh_peers_sent {
            return Err(PublishError::InsufficientPeers);
        }

        // If the message isn't a duplicate and we have sent it to some peers add it to the
        // duplicate cache and memcache.
        self.duplicate_cache.insert(msg_id.clone());

        let cached_message = CachedMessage {
            source: raw_message.source,
            data: raw_message.data,
            sequence_number: raw_message.sequence_number,
            topic: raw_message.topic,
            signature: raw_message.signature,
            key: raw_message.key,
            validated: true, // all published messages are valid
        };
        self.mcache.put(&msg_id, cached_message);

        // If the message is anonymous or has a random author add it to the published message IDs
        // cache.
        if self.message_signer.author().is_none() && !self.config.allow_self_origin() {
            self.published_message_ids.insert(msg_id.clone());
        }

        // Send to peers we know are subscribed to the topic.
        let msg_bytes = event.encoded_len();
        for peer_id in recipient_peers.iter() {
            trace!("Sending message to peer: {:?}", peer_id);
            self.send_rpc_message(*peer_id, event.clone())?;
            self.metrics.msg_sent(&topic_hash, msg_bytes);
        }

        debug!("Published message: {:?}", &msg_id);
        self.metrics.register_published_message(&topic_hash);

        Ok(msg_id)
    }

    /// This function should be called when [`Config::validate_messages()`] is `true` after
    /// the message got validated by the caller. Messages are stored in the ['Memcache'] and
    /// validation is expected to be fast enough that the messages should still exist in the cache.
    /// There are three possible validation outcomes and the outcome is given in acceptance.
    ///
    /// If acceptance = [`MessageAcceptance::Accept`] the message will get propagated to the
    /// network. The `propagation_source` parameter indicates who the message was received by and
    /// will not be forwarded back to that peer.
    ///
    /// If acceptance = [`MessageAcceptance::Reject`] the message will be deleted from the memcache
    /// and the P₄ penalty will be applied to the `propagation_source`.
    //
    /// If acceptance = [`MessageAcceptance::Ignore`] the message will be deleted from the memcache
    /// but no P₄ penalty will be applied.
    ///
    /// This function will return true if the message was found in the cache and false if was not
    /// in the cache anymore.
    ///
    /// This should only be called once per message.
    pub fn report_message_validation_result(
        &mut self,
        msg_id: &MessageId,
        propagation_source: &PeerId,
        acceptance: MessageAcceptance,
    ) -> Result<bool, PublishError> {
        let reject_reason = match acceptance {
            MessageAcceptance::Accept => {
                let (raw_message, originating_peers) = match self.mcache.validate(msg_id) {
                    Some((raw_message, originating_peers)) => {
                        (raw_message.clone(), originating_peers)
                    }
                    None => {
                        warn!(
                            "Message not in cache. Ignoring forwarding. Message Id: {}",
                            msg_id
                        );
                        self.metrics.memcache_miss();
                        return Ok(false);
                    }
                };

                self.metrics
                    .register_msg_validation(&raw_message.topic, &acceptance);

                self.forward_msg(
                    msg_id,
                    raw_message.into(),
                    Some(propagation_source),
                    originating_peers,
                )?;
                return Ok(true);
            }
            MessageAcceptance::Reject => RejectReason::ValidationFailed,
            MessageAcceptance::Ignore => RejectReason::ValidationIgnored,
        };

        if let Some((raw_message, originating_peers)) = self.mcache.remove(msg_id) {
            self.metrics
                .register_msg_validation(&raw_message.topic, &acceptance);

            // Tell peer_score about reject
            // Reject the original source, and any duplicates we've seen from other peers.
            self.peer_score.peer_score_reject_message(
                propagation_source,
                msg_id,
                &raw_message.topic,
                reject_reason,
            );
            for peer in originating_peers.iter() {
                self.peer_score.peer_score_reject_message(
                    peer,
                    msg_id,
                    &raw_message.topic,
                    reject_reason,
                );
            }
            Ok(true)
        } else {
            warn!("Rejected message not in cache. Message Id: {}", msg_id);
            Ok(false)
        }
    }

    /// Adds a new peer to the list of explicitly connected peers.
    pub fn add_explicit_peer(&mut self, peer_id: &PeerId) {
        debug!("Adding explicit peer {}", peer_id);

        self.explicit_peers.insert(*peer_id);

        self.check_explicit_peer_connection(peer_id);
    }

    /// This removes the peer from explicitly connected peers, note that this does not disconnect
    /// the peer.
    pub fn remove_explicit_peer(&mut self, peer_id: &PeerId) {
        debug!("Removing explicit peer {}", peer_id);
        self.explicit_peers.remove(peer_id);
    }

    /// Blacklists a peer. All messages from this peer will be rejected and any message that was
    /// created by this peer will be rejected.
    pub fn blacklist_peer(&mut self, peer_id: &PeerId) {
        if self.blacklisted_peers.insert(*peer_id) {
            debug!("Peer has been blacklisted: {}", peer_id);
        }
    }

    /// Removes a peer from the blacklist if it has previously been blacklisted.
    pub fn remove_blacklisted_peer(&mut self, peer_id: &PeerId) {
        if self.blacklisted_peers.remove(peer_id) {
            debug!("Peer has been removed from the blacklist: {}", peer_id);
        }
    }

    /// Activates the peer scoring system with the given parameters. This will reset all scores
    /// if there was already another peer scoring system activated. Returns an error if the
    /// params are not valid or if they got already set.
    pub fn with_peer_score(
        &mut self,
        params: PeerScoreParams,
        threshold: PeerScoreThresholds,
    ) -> Result<(), String> {
        self.with_peer_score_and_message_delivery_time_callback(params, threshold, None)
    }

    /// Activates the peer scoring system with the given parameters and a message delivery time
    /// callback. Returns an error if the parameters got already set.
    pub fn with_peer_score_and_message_delivery_time_callback(
        &mut self,
        params: PeerScoreParams,
        threshold: PeerScoreThresholds,
        callback: Option<fn(&PeerId, &TopicHash, f64)>,
    ) -> Result<(), String> {
        params.validate()?;
        threshold.validate()?;

        let interval = Ticker::new(params.decay_interval);
        let peer_score = PeerScore::new_with_message_delivery_time_callback(params, callback);
        self.peer_score = Box::new(GossipsubPeerScoreService::new(
            peer_score, threshold, interval,
        ));
        Ok(())
    }

    /// Sets scoring parameters for a topic.
    ///
    /// The [`Self::with_peer_score()`] must first be called to initialise peer scoring.
    pub fn set_topic_params<H: Hasher>(
        &mut self,
        topic: Topic<H>,
        params: TopicScoreParams,
    ) -> Result<(), &'static str> {
        self.peer_score
            .peer_score_set_topic_params(topic.hash(), params);
        Ok(())
    }

    /// Sets the application specific score for a peer. Returns true if scoring is active and
    /// the peer is connected or if the score of the peer is not yet expired, false otherwise.
    pub fn set_application_score(&mut self, peer_id: &PeerId, new_score: f64) -> bool {
        self.peer_score
            .peer_score_set_application_score(peer_id, new_score)
    }

    /// Gossipsub JOIN(topic) - adds topic peers to mesh and sends them GRAFT messages.
    fn join(&mut self, topic_hash: &TopicHash) {
        debug!("Running JOIN for topic: {:?}", topic_hash);

        let mut added_peers = HashSet::new();

        self.metrics.joined(topic_hash);

        // check if we have mesh_n peers in fanout[topic] and add them to the mesh if we do,
        // removing the fanout entry.
        if let Some((_, mut peers)) = self.fanout.remove_entry(topic_hash) {
            debug!(
                "JOIN: Removing peers from the fanout for topic: {:?}",
                topic_hash
            );

            // remove explicit peers, peers with negative scores, and backoffed peers
            peers.retain(|p| {
                !self.explicit_peers.contains(p)
                    && !self.peer_score.score_below_threshold(p, |_| 0.0).0
                    && !self.backoffs.is_backoff_with_slack(topic_hash, p)
            });

            // Add up to mesh_n of them them to the mesh
            // NOTE: These aren't randomly added, currently FIFO
            let add_peers = std::cmp::min(peers.len(), self.config.mesh_n());
            debug!(
                "JOIN: Adding {:?} peers from the fanout for topic: {:?}",
                add_peers, topic_hash
            );
            added_peers.extend(peers.iter().cloned().take(add_peers));

            self.mesh.insert(
                topic_hash.clone(),
                peers.into_iter().take(add_peers).collect(),
            );

            // remove the last published time
            self.fanout_last_pub.remove(topic_hash);
        }

        let fanout_added = added_peers.len();
        self.metrics
            .peers_included(topic_hash, Inclusion::Fanout, fanout_added);

        // check if we need to get more peers, which we randomly select
        if added_peers.len() < self.config.mesh_n() {
            // get the peers
            let new_peers = get_random_peers(
                &self.topic_peers,
                &self.connected_peers,
                topic_hash,
                self.config.mesh_n() - added_peers.len(),
                |peer| {
                    !added_peers.contains(peer)
                        && !self.explicit_peers.contains(peer)
                        && !self.peer_score.score_below_threshold(peer, |_| 0.0).0
                        && !self.backoffs.is_backoff_with_slack(topic_hash, peer)
                },
            );
            added_peers.extend(new_peers.clone());
            // add them to the mesh
            debug!(
                "JOIN: Inserting {:?} random peers into the mesh",
                new_peers.len()
            );
            let mesh_peers = self
                .mesh
                .entry(topic_hash.clone())
                .or_insert_with(Default::default);
            mesh_peers.extend(new_peers);
        }

        let random_added = added_peers.len() - fanout_added;
        self.metrics
            .peers_included(topic_hash, Inclusion::Random, random_added);

        for peer_id in added_peers {
            // Send a GRAFT control message
            debug!("JOIN: Sending Graft message to peer: {:?}", peer_id);
            self.peer_score.peer_score_graft(&peer_id, topic_hash);
            Self::control_pool_add(
                &mut self.control_pool,
                peer_id,
                ControlAction::Graft {
                    topic_hash: topic_hash.clone(),
                },
            );

            // If the peer did not previously exist in any mesh, inform the handler
            peer_added_to_mesh(
                peer_id,
                vec![topic_hash],
                &self.mesh,
                self.peer_topics.get(&peer_id),
                &mut self.events,
                &self.connected_peers,
            );
        }

        let mesh_peers = self.mesh_peers(topic_hash).count();
        self.metrics.set_mesh_peers(topic_hash, mesh_peers);

        debug!("Completed JOIN for topic: {:?}", topic_hash);
    }

    /// Creates a PRUNE gossipsub action.
    fn make_prune(
        &mut self,
        topic_hash: &TopicHash,
        peer: &PeerId,
        do_px: bool,
        on_unsubscribe: bool,
    ) -> ControlAction {
        self.peer_score.peer_score_prune(peer, topic_hash);

        match self.connected_peers.kind(peer) {
            Some(PeerKind::Floodsub) => {
                error!("Attempted to prune a Floodsub peer");
            }
            Some(PeerKind::Gossipsub) => {
                // GossipSub v1.0 -- no peer exchange, the peer won't be able to parse it anyway
                return ControlAction::Prune {
                    topic_hash: topic_hash.clone(),
                    peers: Vec::new(),
                    backoff: None,
                };
            }
            None => {
                error!("Attempted to Prune an unknown peer");
            }
            _ => {} // Gossipsub 1.1 peer perform the `Prune`
        }

        // Select peers for peer exchange
        let peers = if do_px {
            get_random_peers(
                &self.topic_peers,
                &self.connected_peers,
                topic_hash,
                self.config.prune_peers(),
                |p| p != peer && !self.peer_score.score_below_threshold(p, |_| 0.0).0,
            )
            .into_iter()
            .map(|p| PeerInfo { peer_id: Some(p) })
            .collect()
        } else {
            Vec::new()
        };

        let backoff = if on_unsubscribe {
            self.config.unsubscribe_backoff()
        } else {
            self.config.prune_backoff()
        };

        // update backoff
        self.backoffs.update_backoff(topic_hash, peer, backoff);

        ControlAction::Prune {
            topic_hash: topic_hash.clone(),
            peers,
            backoff: Some(backoff.as_secs()),
        }
    }

    /// Gossipsub LEAVE(topic) - Notifies mesh\[topic\] peers with PRUNE messages.
    fn leave(&mut self, topic_hash: &TopicHash) {
        debug!("Running LEAVE for topic {:?}", topic_hash);

        // If our mesh contains the topic, send prune to peers and delete it from the mesh
        if let Some((_, peers)) = self.mesh.remove_entry(topic_hash) {
            self.metrics.left(topic_hash);
            for peer in peers {
                // Send a PRUNE control message
                debug!("LEAVE: Sending PRUNE to peer: {:?}", peer);
                let on_unsubscribe = true;
                let control =
                    self.make_prune(topic_hash, &peer, self.config.do_px(), on_unsubscribe);
                Self::control_pool_add(&mut self.control_pool, peer, control);

                // If the peer did not previously exist in any mesh, inform the handler
                peer_removed_from_mesh(
                    peer,
                    topic_hash,
                    &self.mesh,
                    self.peer_topics.get(&peer),
                    &mut self.events,
                    &self.connected_peers,
                );
            }
        }

        debug!("Completed LEAVE for topic: {:?}", topic_hash);
    }

    /// Checks if the given peer is still connected and if not dials the peer again.
    fn check_explicit_peer_connection(&mut self, peer_id: &PeerId) {
        if !self.peer_topics.contains_key(peer_id) {
            // Connect to peer
            debug!("Connecting to explicit peer {:?}", peer_id);
            self.events.push_back(ToSwarm::Dial {
                opts: DialOpts::peer_id(*peer_id).build(),
            });
        }
    }

    fn handle_received_rpc(&mut self, propagation_source: &PeerId, rpc: RpcProto) {
        // Handle subscriptions
        let subscriptions: Vec<Subscription> =
            rpc.subscriptions.into_iter().map(Into::into).collect();

        // Update connected peers topics
        if !subscriptions.is_empty() {
            self.handle_received_subscriptions(&subscriptions, propagation_source);
        }

        // TODO: Review the concept of "gray-listing" peers
        // Check if peer is gray-listed in which case we ignore the event
        if let (true, _) = self
            .peer_score
            .score_below_threshold(propagation_source, |pst| pst.graylist_threshold)
        {
            debug!("RPC Dropped from gray-listed peer {}", propagation_source);
            return;
        }

        // Handle messages
        let mut valid_messages = Vec::with_capacity(rpc.publish.len());
        let mut invalid_messages = Vec::new();

        for message in rpc.publish.into_iter() {
            if let Err(err) = validate_message_proto(&message) {
                // If the message is invalid, add it to the invalid messages and continue
                // processing the other messages.
                // TODO: Review this logic. Possible validation errors here:
                //   - InvalidTopic (empty topic)
                //   - InvalidPeerId
                //   - InvalidSequenceNumber (not a Big-endian encoded u64)
                let raw_message = RawMessage {
                    source: None,          // don't inform the application
                    data: vec![],          // don't inform the application
                    sequence_number: None, // don't inform the application
                    topic: "".into(),      // don't inform the application
                    signature: None,       // don't inform the application
                    key: None,             // don't inform the application
                };
                invalid_messages.push((raw_message, err));

                continue;
            }

            let message: MessageRpc = message.into();
            if let Err(err) = self.message_validator.validate(&message) {
                // If the message is invalid, add it to the invalid messages and continue
                // processing the other messages.
                // TODO: Review this logic, together with invalid messages peer scoring.
                let raw_message = RawMessage {
                    topic: message.topic().into(),
                    data: message.data().to_vec(),
                    source: None,          // don't inform the application
                    sequence_number: None, // don't inform the application
                    signature: None,       // don't inform the application
                    key: None,             // don't inform the application
                };
                invalid_messages.push((raw_message, err));

                continue;
            }

            // This message has passed all validation, add it to the validated messages.
            valid_messages.push(message.into());
        }

        // Handle any invalid messages from this peer
        for (raw_message, validation_error) in invalid_messages {
            let reject_reason = RejectReason::ValidationError(validation_error);
            self.metrics.register_invalid_message(&raw_message.topic);

            let fast_message_id_cache = &self.fast_message_id_cache;

            if let Some(msg_id) = self
                .config
                .fast_message_id(&raw_message)
                .and_then(|id| fast_message_id_cache.get(&id))
            {
                self.peer_score.peer_score_reject_message(
                    propagation_source,
                    msg_id,
                    &raw_message.topic,
                    reject_reason,
                );
                self.peer_score
                    .gossip_promises_reject_message(msg_id, &reject_reason);
            } else {
                // The message is invalid, we reject it ignoring any gossip promises. If a peer is
                // advertising this message via an IHAVE and it's invalid it will be double
                // penalized, one for sending us an invalid and again for breaking a promise.
                self.peer_score
                    .peer_score_reject_invalid_message(propagation_source, &raw_message.topic);
            }
        }

        for (count, raw_message) in valid_messages.into_iter().enumerate() {
            // Only process the amount of messages the configuration allows.
            if self.config.max_messages_per_rpc().is_some()
                && Some(count) >= self.config.max_messages_per_rpc()
            {
                warn!("Received more messages than permitted. Ignoring further messages. Processed: {}", count);
                break;
            }
            self.handle_received_message(raw_message, propagation_source);
        }

        // Handle control messages
        if let Some(rpc_control) = rpc.control {
            for iwant in rpc_control.iwant {
                let iwant_msgs = iwant.message_ids.into_iter().map(Into::into).collect();
                self.handle_iwant(propagation_source, iwant_msgs);
            }

            let ihave_msgs = rpc_control
                .ihave
                .into_iter()
                .map(|ihave| {
                    (
                        TopicHash::from_raw(ihave.topic_id.unwrap_or_default()),
                        ihave.message_ids.into_iter().map(Into::into).collect(),
                    )
                })
                .collect::<Vec<_>>();
            if !ihave_msgs.is_empty() {
                self.handle_ihave(propagation_source, ihave_msgs);
            }

            let graft_msgs = rpc_control
                .graft
                .into_iter()
                .map(|graft| TopicHash::from_raw(graft.topic_id.unwrap_or_default()))
                .collect::<Vec<_>>();
            if !graft_msgs.is_empty() {
                self.handle_graft(propagation_source, graft_msgs);
            }

            let prune_msgs = rpc_control
                .prune
                .into_iter()
                .map(|prune| {
                    let peers = prune
                        .peers
                        .into_iter()
                        .filter_map(|info| PeerInfo::try_from(info).ok()) // filter out invalid peers
                        .collect();
                    let topic_hash = TopicHash::from_raw(prune.topic_id.unwrap_or_default());

                    (topic_hash, peers, prune.backoff)
                })
                .collect::<Vec<_>>();
            if !prune_msgs.is_empty() {
                self.handle_prune(propagation_source, prune_msgs);
            }
        }
    }

    /// Handles an IHAVE control message. Checks our cache of messages. If the message is unknown,
    /// requests it with an IWANT control message.
    fn handle_ihave(&mut self, peer_id: &PeerId, ihave_msgs: Vec<(TopicHash, Vec<MessageId>)>) {
        // We ignore IHAVE gossip from any peer whose score is below the gossip threshold
        if let (true, score) = self
            .peer_score
            .score_below_threshold(peer_id, |pst| pst.gossip_threshold)
        {
            debug!(
                "IHAVE: ignoring peer {:?} with score below threshold [score = {}]",
                peer_id, score
            );
            return;
        }

        // IHAVE flood protection
        let peer_have = self.count_received_ihave.entry(*peer_id).or_insert(0);
        *peer_have += 1;
        if *peer_have > self.config.max_ihave_messages() {
            debug!(
                "IHAVE: peer {} has advertised too many times ({}) within this heartbeat \
            interval; ignoring",
                peer_id, *peer_have
            );
            return;
        }

        if let Some(iasked) = self.count_sent_iwant.get(peer_id) {
            if *iasked >= self.config.max_ihave_length() {
                debug!(
                    "IHAVE: peer {} has already advertised too many messages ({}); ignoring",
                    peer_id, *iasked
                );
                return;
            }
        }

        trace!("Handling IHAVE for peer: {:?}", peer_id);

        let mut iwant_ids = HashSet::new();

        let want_message = |id: &MessageId| {
            if self.duplicate_cache.contains(id) {
                return false;
            }

            if self.pending_iwant_msgs.contains(id) {
                return false;
            }

            self.peer_score.promises_contains(id)
        };

        for (topic, ids) in ihave_msgs {
            // only process the message if we are subscribed
            if !self.mesh.contains_key(&topic) {
                debug!(
                    "IHAVE: Ignoring IHAVE - Not subscribed to topic: {:?}",
                    topic
                );
                continue;
            }

            for id in ids.into_iter().filter(want_message) {
                // have not seen this message and are not currently requesting it
                if iwant_ids.insert(id) {
                    // Register the IWANT metric
                    self.metrics.register_iwant(&topic);
                }
            }
        }

        if !iwant_ids.is_empty() {
            let iasked = self.count_sent_iwant.entry(*peer_id).or_insert(0);
            let mut iask = iwant_ids.len();
            if *iasked + iask > self.config.max_ihave_length() {
                iask = self.config.max_ihave_length().saturating_sub(*iasked);
            }

            // Send the list of IWANT control messages
            debug!(
                "IHAVE: Asking for {} out of {} messages from {}",
                iask,
                iwant_ids.len(),
                peer_id
            );

            // Ask in random order
            let mut iwant_ids_vec: Vec<_> = iwant_ids.into_iter().collect();
            let mut rng = thread_rng();
            iwant_ids_vec.partial_shuffle(&mut rng, iask);

            iwant_ids_vec.truncate(iask);
            *iasked += iask;

            for message_id in &iwant_ids_vec {
                // Add all messages to the pending list
                self.pending_iwant_msgs.insert(message_id.clone());
            }

            self.peer_score.promises_add(
                *peer_id,
                &iwant_ids_vec,
                Instant::now() + self.config.iwant_followup_time(),
            );
            trace!(
                "IHAVE: Asking for the following messages from {}: {:?}",
                peer_id,
                iwant_ids_vec
            );

            Self::control_pool_add(
                &mut self.control_pool,
                *peer_id,
                ControlAction::IWant {
                    message_ids: iwant_ids_vec,
                },
            );
        }
        trace!("Completed IHAVE handling for peer: {:?}", peer_id);
    }

    /// Handles an IWANT control message. Checks our cache of messages. If the message exists it is
    /// forwarded to the requesting peer.
    fn handle_iwant(&mut self, peer_id: &PeerId, iwant_msgs: Vec<MessageId>) {
        // We ignore IWANT gossip from any peer whose score is below the gossip threshold
        if let (true, score) = self
            .peer_score
            .score_below_threshold(peer_id, |pst| pst.gossip_threshold)
        {
            debug!(
                "IWANT: ignoring peer {:?} with score below threshold [score = {}]",
                peer_id, score
            );
            return;
        }

        debug!("Handling IWANT for peer: {:?}", peer_id);
        // build a hashmap of available messages
        let mut cached_messages = HashMap::new();

        for id in iwant_msgs {
            // If we have it and the IHAVE count is not above the threshold, add it do the
            // cached_messages mapping
            if let Some((msg, count)) = self.mcache.get_with_iwant_counts(&id, peer_id) {
                if count > self.config.gossip_retransimission() {
                    debug!(
                        "IWANT: Peer {} has asked for message {} too many times; ignoring \
                    request",
                        peer_id, &id
                    );
                } else {
                    cached_messages.insert(id.clone(), msg.clone());
                }
            }
        }

        if !cached_messages.is_empty() {
            debug!("IWANT: Sending cached messages to peer: {:?}", peer_id);
            // Send the messages to the peer
            let message_list: Vec<RawMessage> = cached_messages
                .into_iter()
                .map(|entry| entry.1.into())
                .collect();

            let topics = message_list
                .iter()
                .map(|message| message.topic.clone())
                .collect::<HashSet<TopicHash>>();

            let message: RpcProto = Rpc {
                subscriptions: Vec::new(),
                messages: message_list,
                control_msgs: Vec::new(),
            }
            .into();

            let msg_bytes = message.encoded_len();

            if self.send_rpc_message(*peer_id, message).is_err() {
                error!("Failed to send cached messages. Messages too large");
            } else {
                // Sending of messages succeeded, register them on the internal metrics.
                for topic in topics.iter() {
                    self.metrics.msg_sent(topic, msg_bytes);
                }
            }
        }

        debug!("Completed IWANT handling for peer: {}", peer_id);
    }

    /// Handles GRAFT control messages. If subscribed to the topic, adds the peer to mesh, if not,
    /// responds with PRUNE messages.
    fn handle_graft(&mut self, peer_id: &PeerId, topics: Vec<TopicHash>) {
        debug!("Handling GRAFT message for peer: {}", peer_id);

        let mut to_prune_topics = HashSet::new();

        let mut do_px = self.config.do_px();

        // For each topic, if a peer has grafted us, then we necessarily must be in their mesh
        // and they must be subscribed to the topic. Ensure we have recorded the mapping.
        for topic in &topics {
            self.peer_topics
                .entry(*peer_id)
                .or_default()
                .insert(topic.clone());
            self.topic_peers
                .entry(topic.clone())
                .or_default()
                .insert(*peer_id);
        }

        // we don't GRAFT to/from explicit peers; complain loudly if this happens
        if self.explicit_peers.contains(peer_id) {
            warn!("GRAFT: ignoring request from direct peer {}", peer_id);
            // this is possibly a bug from non-reciprocal configuration; send a PRUNE for all topics
            to_prune_topics = topics.into_iter().collect();
            // but don't PX
            do_px = false
        } else {
            let (below_zero, score) = self.peer_score.score_below_threshold(peer_id, |_| 0.0);
            let now = Instant::now();
            for topic_hash in topics {
                if let Some(peers) = self.mesh.get_mut(&topic_hash) {
                    // if the peer is already in the mesh ignore the graft
                    if peers.contains(peer_id) {
                        debug!(
                            "GRAFT: Received graft for peer {:?} that is already in topic {:?}",
                            peer_id, &topic_hash
                        );
                        continue;
                    }

                    // make sure we are not backing off that peer
                    if let Some(backoff_time) = self.backoffs.get_backoff_time(&topic_hash, peer_id)
                    {
                        if backoff_time > now {
                            warn!(
                                "[Penalty] Peer attempted graft within backoff time, penalizing {}",
                                peer_id
                            );

                            // add behavioural penalty
                            self.metrics.register_score_penalty(Penalty::GraftBackoff);
                            self.peer_score.peer_score_add_penalty(peer_id, 1);

                            // check the flood cutoff
                            // See: https://github.com/rust-lang/rust-clippy/issues/10061
                            #[allow(unknown_lints, clippy::unchecked_duration_subtraction)]
                            let flood_cutoff = (backoff_time + self.config.graft_flood_threshold())
                                - self.config.prune_backoff();
                            if flood_cutoff > now {
                                //extra penalty
                                self.peer_score.peer_score_add_penalty(peer_id, 1);
                            }

                            // no PX
                            do_px = false;

                            to_prune_topics.insert(topic_hash.clone());
                            continue;
                        }
                    }

                    // check the score
                    if below_zero {
                        // we don't GRAFT peers with negative score
                        debug!(
                            "GRAFT: ignoring peer {:?} with negative score [score = {}, \
                        topic = {}]",
                            peer_id, score, topic_hash
                        );
                        // we do send them PRUNE however, because it's a matter of protocol correctness
                        to_prune_topics.insert(topic_hash.clone());
                        // but we won't PX to them
                        do_px = false;
                        continue;
                    }

                    // check mesh upper bound and only allow graft if the upper bound is not reached or
                    // if it is an outbound peer
                    if peers.len() >= self.config.mesh_n_high()
                        && !self.connected_peers.is_outbound(peer_id)
                    {
                        to_prune_topics.insert(topic_hash.clone());
                        continue;
                    }

                    // add peer to the mesh
                    debug!(
                        "GRAFT: Mesh link added for peer: {:?} in topic: {:?}",
                        peer_id, &topic_hash
                    );

                    if peers.insert(*peer_id) {
                        self.metrics
                            .peers_included(&topic_hash, Inclusion::Subscribed, 1)
                    }

                    // If the peer did not previously exist in any mesh, inform the handler
                    peer_added_to_mesh(
                        *peer_id,
                        vec![&topic_hash],
                        &self.mesh,
                        self.peer_topics.get(peer_id),
                        &mut self.events,
                        &self.connected_peers,
                    );

                    self.peer_score.peer_score_graft(peer_id, &topic_hash);
                } else {
                    // don't do PX when there is an unknown topic to avoid leaking our peers
                    do_px = false;
                    debug!(
                        "GRAFT: Received graft for unknown topic {:?} from peer {:?}",
                        &topic_hash, peer_id
                    );
                    // spam hardening: ignore GRAFTs for unknown topics
                    continue;
                }
            }
        }

        if !to_prune_topics.is_empty() {
            // build the prune messages to send
            let on_unsubscribe = false;
            let prune_messages = to_prune_topics
                .iter()
                .map(|t| self.make_prune(t, peer_id, do_px, on_unsubscribe))
                .collect();
            // Send the prune messages to the peer
            debug!(
                "GRAFT: Not subscribed to topics -  Sending PRUNE to peer: {}",
                peer_id
            );

            if let Err(e) = self.send_control_rpc_message(*peer_id, prune_messages) {
                error!("Failed to send PRUNE: {:?}", e);
            }
        }
        debug!("Completed GRAFT handling for peer: {}", peer_id);
    }

    fn remove_peer_from_mesh(
        &mut self,
        peer_id: &PeerId,
        topic_hash: &TopicHash,
        backoff: Option<u64>,
        always_update_backoff: bool,
        reason: Churn,
    ) {
        let mut update_backoff = always_update_backoff;
        if let Some(peers) = self.mesh.get_mut(topic_hash) {
            // remove the peer if it exists in the mesh
            if peers.remove(peer_id) {
                debug!(
                    "PRUNE: Removing peer: {} from the mesh for topic: {}",
                    peer_id.to_string(),
                    topic_hash
                );
                self.metrics.peers_removed(topic_hash, reason, 1);

                self.peer_score.peer_score_prune(peer_id, topic_hash);

                update_backoff = true;

                // inform the handler
                peer_removed_from_mesh(
                    *peer_id,
                    topic_hash,
                    &self.mesh,
                    self.peer_topics.get(peer_id),
                    &mut self.events,
                    &self.connected_peers,
                );
            }
        }
        if update_backoff {
            let time = if let Some(backoff) = backoff {
                Duration::from_secs(backoff)
            } else {
                self.config.prune_backoff()
            };
            // is there a backoff specified by the peer? if so obey it.
            self.backoffs.update_backoff(topic_hash, peer_id, time);
        }
    }

    /// Handles PRUNE control messages. Removes peer from the mesh.
    fn handle_prune(
        &mut self,
        peer_id: &PeerId,
        prune_data: Vec<(TopicHash, Vec<PeerInfo>, Option<u64>)>,
    ) {
        debug!("Handling PRUNE message for peer: {}", peer_id);
        let (below_threshold, score) = self
            .peer_score
            .score_below_threshold(peer_id, |pst| pst.accept_px_threshold);
        for (topic_hash, px, backoff) in prune_data {
            self.remove_peer_from_mesh(peer_id, &topic_hash, backoff, true, Churn::Prune);

            if self.mesh.contains_key(&topic_hash) {
                //connect to px peers
                if !px.is_empty() {
                    // we ignore PX from peers with insufficient score
                    if below_threshold {
                        debug!(
                            "PRUNE: ignoring PX from peer {:?} with insufficient score \
                             [score ={} topic = {}]",
                            peer_id, score, topic_hash
                        );
                        continue;
                    }

                    // NOTE: We cannot dial any peers from PX currently as we typically will not
                    // know their multiaddr. Until SignedRecords are spec'd this
                    // remains a stub. By default `config.prune_peers()` is set to zero and
                    // this is skipped. If the user modifies this, this will only be able to
                    // dial already known peers (from an external discovery mechanism for
                    // example).
                    if self.config.prune_peers() > 0 {
                        self.px_connect(px);
                    }
                }
            }
        }
        debug!("Completed PRUNE handling for peer: {}", peer_id.to_string());
    }

    fn px_connect(&mut self, mut px: Vec<PeerInfo>) {
        let n = self.config.prune_peers();
        // Ignore peerInfo with no ID
        //
        //TODO: Once signed records are spec'd: Can we use peerInfo without any IDs if they have a
        // signed peer record?
        px.retain(|p| p.peer_id.is_some());
        if px.len() > n {
            // only use at most prune_peers many random peers
            let mut rng = thread_rng();
            px.partial_shuffle(&mut rng, n);
            px = px.into_iter().take(n).collect();
        }

        for p in px {
            // TODO: Once signed records are spec'd: extract signed peer record if given and handle
            // it, see https://github.com/libp2p/specs/pull/217
            if let Some(peer_id) = p.peer_id {
                // mark as px peer
                self.px_peers.insert(peer_id);

                // dial peer
                self.events.push_back(ToSwarm::Dial {
                    opts: DialOpts::peer_id(peer_id).build(),
                });
            }
        }
    }

    /// Applies some basic checks to whether this message is valid. Does not apply user validation
    /// checks.
    fn message_is_valid(
        &mut self,
        msg_id: &MessageId,
        raw_message: &mut RawMessage,
        propagation_source: &PeerId,
    ) -> bool {
        debug!(
            "Handling message: {:?} from peer: {}",
            msg_id,
            propagation_source.to_string()
        );

        // Reject any message from a blacklisted peer
        if self.blacklisted_peers.contains(propagation_source) {
            debug!(
                "Rejecting message from blacklisted peer: {}",
                propagation_source
            );
            self.peer_score.peer_score_reject_message(
                propagation_source,
                msg_id,
                &raw_message.topic,
                RejectReason::BlackListedPeer,
            );
            self.peer_score
                .gossip_promises_reject_message(msg_id, &RejectReason::BlackListedPeer);
            return false;
        }

        // Also reject any message that originated from a blacklisted peer
        if let Some(source) = raw_message.source.as_ref() {
            if self.blacklisted_peers.contains(source) {
                debug!(
                    "Rejecting message from peer {} because of blacklisted source: {}",
                    propagation_source, source
                );
                self.handle_invalid_message(
                    propagation_source,
                    raw_message,
                    RejectReason::BlackListedSource,
                );
                return false;
            }
        }

        // reject messages claiming to be from ourselves but not locally published
        let self_published = !self.config.allow_self_origin()
            && if let Some(own_id) = self.message_signer.author() {
                own_id != propagation_source
                    && raw_message.source.as_ref().map_or(false, |s| s == own_id)
            } else {
                self.published_message_ids.contains(msg_id)
            };

        if self_published {
            debug!(
                "Dropping message {} claiming to be from self but forwarded from {}",
                msg_id, propagation_source
            );
            self.handle_invalid_message(propagation_source, raw_message, RejectReason::SelfOrigin);
            return false;
        }

        true
    }

    /// Handles a newly received [`RawMessage`].
    ///
    /// Forwards the message to all peers in the mesh.
    fn handle_received_message(
        &mut self,
        mut raw_message: RawMessage,
        propagation_source: &PeerId,
    ) {
        // Record the received metric
        self.metrics
            .msg_recvd_unfiltered(&raw_message.topic, raw_message.raw_protobuf_len());

        let fast_message_id = self.config.fast_message_id(&raw_message);

        if let Some(fast_message_id) = fast_message_id.as_ref() {
            if let Some(msg_id) = self.fast_message_id_cache.get(fast_message_id) {
                let msg_id = msg_id.clone();
                // Report the duplicate
                if self.message_is_valid(&msg_id, &mut raw_message, propagation_source) {
                    self.peer_score.peer_score_duplicated_message(
                        propagation_source,
                        &msg_id,
                        &raw_message.topic,
                    );

                    // Update the cache, informing that we have received a duplicate from another peer.
                    // The peers in this cache are used to prevent us forwarding redundant messages onto
                    // these peers.
                    self.mcache.observe_duplicate(&msg_id, propagation_source);
                }

                // This message has been seen previously. Ignore it
                return;
            }
        }

        // Try and perform the data transform to the message. If it fails, consider it invalid.
        let message = match self.data_transform.inbound_transform(raw_message.clone()) {
            Ok(message) => message,
            Err(e) => {
                debug!("Invalid message. Transform error: {:?}", e);
                // Reject the message and return
                self.handle_invalid_message(
                    propagation_source,
                    &raw_message,
                    RejectReason::ValidationError(ValidationError::TransformFailed),
                );
                return;
            }
        };

        // Calculate the message id on the transformed data.
        let msg_id = self.config.message_id(&message);

        // Check the validity of the message
        // Peers get penalized if this message is invalid. We don't add it to the duplicate cache
        // and instead continually penalize peers that repeatedly send this message.
        if !self.message_is_valid(&msg_id, &mut raw_message, propagation_source) {
            return;
        }

        // Add the message to the duplicate caches
        if let Some(fast_message_id) = fast_message_id {
            // add id to cache
            self.fast_message_id_cache
                .entry(fast_message_id)
                .or_insert_with(|| msg_id.clone());
        }

        if !self.duplicate_cache.insert(msg_id.clone()) {
            debug!("Message already received, ignoring. Message: {}", msg_id);
            self.peer_score.peer_score_duplicated_message(
                propagation_source,
                &msg_id,
                &message.topic,
            );
            self.mcache.observe_duplicate(&msg_id, propagation_source);
            return;
        }
        debug!(
            "Put message {:?} in duplicate_cache and resolve promises",
            msg_id
        );

        // Record the received message with the metrics
        self.metrics.msg_recvd(&message.topic);

        // Tells score that message arrived (but is maybe not fully validated yet).
        // Consider the message as delivered for gossip promises.
        self.peer_score
            .peer_score_validate_message(propagation_source, &msg_id, &message.topic);
        self.peer_score.promises_message_delivered(&msg_id);

        // Add the message to our memcache
        let cached_message = {
            let message = raw_message.clone();
            CachedMessage {
                source: message.source,
                data: message.data,
                sequence_number: message.sequence_number,
                topic: message.topic,
                signature: message.signature,
                key: message.key,
                // If we are not validating messages, assume this message is validated
                // This will allow the message to be gossiped without explicitly calling
                // `validate_message`.
                validated: !self.config.validate_messages(),
            }
        };
        self.mcache.put(&msg_id, cached_message);

        // Dispatch the message to the user if we are subscribed to any of the topics
        if self.mesh.contains_key(&message.topic) {
            debug!("Sending received message to user");
            self.events
                .push_back(ToSwarm::GenerateEvent(Event::Message {
                    propagation_source: *propagation_source,
                    message_id: msg_id.clone(),
                    message,
                }));
        } else {
            debug!(
                "Received message on a topic we are not subscribed to: {:?}",
                message.topic
            );
            return;
        }

        // forward the message to mesh peers, if no validation is required
        if !self.config.validate_messages() {
            if self
                .forward_msg(
                    &msg_id,
                    raw_message,
                    Some(propagation_source),
                    HashSet::new(),
                )
                .is_err()
            {
                error!("Failed to forward message. Too large");
            }
            debug!("Completed message handling for message: {:?}", msg_id);
        }
    }

    // Handles invalid messages received.
    fn handle_invalid_message(
        &mut self,
        propagation_source: &PeerId,
        raw_message: &RawMessage,
        reject_reason: RejectReason,
    ) {
        self.metrics.register_invalid_message(&raw_message.topic);

        let fast_message_id_cache = &self.fast_message_id_cache;

        if let Some(msg_id) = self
            .config
            .fast_message_id(raw_message)
            .and_then(|id| fast_message_id_cache.get(&id))
        {
            self.peer_score.peer_score_reject_message(
                propagation_source,
                msg_id,
                &raw_message.topic,
                reject_reason,
            );
            self.peer_score
                .gossip_promises_reject_message(msg_id, &reject_reason);
        } else {
            // The message is invalid, we reject it ignoring any gossip promises. If a peer is
            // advertising this message via an IHAVE and it's invalid it will be double
            // penalized, one for sending us an invalid and again for breaking a promise.
            self.peer_score
                .peer_score_reject_invalid_message(propagation_source, &raw_message.topic);
        }
    }

    /// Handles received subscriptions.
    fn handle_received_subscriptions(
        &mut self,
        subscriptions: &[Subscription],
        propagation_source: &PeerId,
    ) {
        debug!(
            "Handling subscriptions: {:?}, from source: {}",
            subscriptions,
            propagation_source.to_string()
        );

        let mut unsubscribed_peers = Vec::new();

        let subscribed_topics = match self.peer_topics.get_mut(propagation_source) {
            Some(topics) => topics,
            None => {
                error!(
                    "Subscription by unknown peer: {}",
                    propagation_source.to_string()
                );
                return;
            }
        };

        // Collect potential graft topics for the peer.
        let mut topics_to_graft = Vec::new();

        // Notify the application about the subscription, after the grafts are sent.
        let mut application_event = Vec::new();

        let filtered_topics = match self
            .subscription_filter
            .filter_incoming_subscriptions(subscriptions, subscribed_topics)
        {
            Ok(topics) => topics,
            Err(s) => {
                error!(
                    "Subscription filter error: {}; ignoring RPC from peer {}",
                    s,
                    propagation_source.to_string()
                );
                return;
            }
        };

        for subscription in filtered_topics {
            // get the peers from the mapping, or insert empty lists if the topic doesn't exist
            let topic_hash = &subscription.topic_hash;
            let peer_list = self
                .topic_peers
                .entry(topic_hash.clone())
                .or_insert_with(Default::default);

            match subscription.action {
                SubscriptionAction::Subscribe => {
                    if peer_list.insert(*propagation_source) {
                        debug!(
                            "SUBSCRIPTION: Adding gossip peer: {} to topic: {:?}",
                            propagation_source.to_string(),
                            topic_hash
                        );
                    }

                    // add to the peer_topics mapping
                    subscribed_topics.insert(topic_hash.clone());

                    // if the mesh needs peers add the peer to the mesh
                    if !self.explicit_peers.contains(propagation_source)
                        && self
                            .connected_peers
                            .kind(propagation_source)
                            .is_some_and(|k| k.is_gossipsub())
                        && !self
                            .peer_score
                            .score_below_threshold(propagation_source, |_| 0.0)
                            .0
                        && !self
                            .backoffs
                            .is_backoff_with_slack(topic_hash, propagation_source)
                    {
                        if let Some(peers) = self.mesh.get_mut(topic_hash) {
                            if peers.len() < self.config.mesh_n_low()
                                && peers.insert(*propagation_source)
                            {
                                debug!(
                                    "SUBSCRIPTION: Adding peer {} to the mesh for topic {:?}",
                                    propagation_source.to_string(),
                                    topic_hash
                                );
                                self.metrics
                                    .peers_included(topic_hash, Inclusion::Subscribed, 1);

                                // send graft to the peer
                                debug!(
                                    "Sending GRAFT to peer {} for topic {:?}",
                                    propagation_source.to_string(),
                                    topic_hash
                                );
                                self.peer_score
                                    .peer_score_graft(propagation_source, topic_hash);
                                topics_to_graft.push(topic_hash.clone());
                            }
                        }
                    }
                    // generates a subscription event to be polled
                    application_event.push(ToSwarm::GenerateEvent(Event::Subscribed {
                        peer_id: *propagation_source,
                        topic: topic_hash.clone(),
                    }));
                }
                SubscriptionAction::Unsubscribe => {
                    if peer_list.remove(propagation_source) {
                        debug!(
                            "SUBSCRIPTION: Removing gossip peer: {} from topic: {:?}",
                            propagation_source.to_string(),
                            topic_hash
                        );
                    }

                    // remove topic from the peer_topics mapping
                    subscribed_topics.remove(topic_hash);
                    unsubscribed_peers.push((*propagation_source, topic_hash.clone()));
                    // generate an unsubscribe event to be polled
                    application_event.push(ToSwarm::GenerateEvent(Event::Unsubscribed {
                        peer_id: *propagation_source,
                        topic: topic_hash.clone(),
                    }));
                }
            }

            self.metrics.set_topic_peers(topic_hash, peer_list.len());
        }

        // remove unsubscribed peers from the mesh if it exists
        for (peer_id, topic_hash) in unsubscribed_peers {
            self.remove_peer_from_mesh(&peer_id, &topic_hash, None, false, Churn::Unsub);
        }

        // Potentially inform the handler if we have added this peer to a mesh for the first time.
        let topics_joined = topics_to_graft.iter().collect::<Vec<_>>();
        if !topics_joined.is_empty() {
            peer_added_to_mesh(
                *propagation_source,
                topics_joined,
                &self.mesh,
                self.peer_topics.get(propagation_source),
                &mut self.events,
                &self.connected_peers,
            );
        }

        // If we need to send grafts to peer, do so immediately, rather than waiting for the
        // heartbeat.
        if !topics_to_graft.is_empty()
            && self
                .send_control_rpc_message(
                    *propagation_source,
                    topics_to_graft
                        .into_iter()
                        .map(|topic_hash| ControlAction::Graft { topic_hash })
                        .collect(),
                )
                .is_err()
        {
            error!("Failed sending grafts. Message too large");
        }

        // Notify the application of the subscriptions
        for event in application_event {
            self.events.push_back(event);
        }

        trace!(
            "Completed handling subscriptions from source: {:?}",
            propagation_source
        );
    }

    /// Heartbeat function which shifts the memcache and updates the mesh.
    fn on_heartbeat(&mut self, heartbeat_ticks: u64) {
        debug!("Starting heartbeat");
        let start = Instant::now();

        let mut to_graft = HashMap::new();
        let mut to_prune = HashMap::new();
        let mut no_px = HashSet::new();

        // clean up expired backoffs
        self.backoffs.heartbeat();

        // clean up ihave counters
        self.count_sent_iwant.clear();
        self.count_received_ihave.clear();

        // Apply penalties to peers that did not respond to our IWANT requests.
        for (peer, count) in self.peer_score.get_broken_promises() {
            self.peer_score.peer_score_add_penalty(&peer, count);
            self.metrics.register_score_penalty(Penalty::BrokenPromise);
        }

        // check connections to explicit peers
        if heartbeat_ticks % self.config.check_explicit_peers_ticks() == 0 {
            for p in self.explicit_peers.clone() {
                self.check_explicit_peer_connection(&p);
            }
        }

        // Cache the scores of all connected peers, and record metrics for current penalties.
        let mut scores = HashMap::with_capacity(self.connected_peers.len());
        for peer_id in self.connected_peers.peers() {
            scores.entry(peer_id).or_insert_with(|| {
                self.peer_score
                    .peer_score_metric_score(peer_id, &mut self.metrics)
            });
        }

        // maintain the mesh for each topic
        for (topic_hash, peers) in self.mesh.iter_mut() {
            let explicit_peers = &self.explicit_peers;
            let backoffs = &self.backoffs;
            let topic_peers = &self.topic_peers;

            // drop all peers with negative score, without PX
            // if there is at some point a stable retain method for BTreeSet the following can be
            // written more efficiently with retain.
            let mut to_remove_peers = Vec::new();
            for peer_id in peers.iter() {
                let peer_score = *scores.get(peer_id).unwrap_or(&0.0);

                // Record the score per mesh
                self.metrics
                    .observe_mesh_peers_score(topic_hash, peer_score);

                if peer_score < 0.0 {
                    debug!(
                        "HEARTBEAT: Prune peer {:?} with negative score [score = {}, topic = \
                             {}]",
                        peer_id, peer_score, topic_hash
                    );

                    let current_topic = to_prune.entry(*peer_id).or_insert_with(Vec::new);
                    current_topic.push(topic_hash.clone());
                    no_px.insert(*peer_id);
                    to_remove_peers.push(*peer_id);
                }
            }

            self.metrics
                .peers_removed(topic_hash, Churn::BadScore, to_remove_peers.len());

            for peer_id in to_remove_peers {
                peers.remove(&peer_id);
            }

            // too little peers - add some
            if peers.len() < self.config.mesh_n_low() {
                debug!(
                    "HEARTBEAT: Mesh low. Topic: {} Contains: {} needs: {}",
                    topic_hash,
                    peers.len(),
                    self.config.mesh_n_low()
                );
                // not enough peers - get mesh_n - current_length more
                let desired_peers = self.config.mesh_n() - peers.len();
                let peer_list = get_random_peers(
                    topic_peers,
                    &self.connected_peers,
                    topic_hash,
                    desired_peers,
                    |peer| {
                        !peers.contains(peer)
                            && !explicit_peers.contains(peer)
                            && !backoffs.is_backoff_with_slack(topic_hash, peer)
                            && *scores.get(peer).unwrap_or(&0.0) >= 0.0
                    },
                );
                for peer in &peer_list {
                    let current_topic = to_graft.entry(*peer).or_insert_with(Vec::new);
                    current_topic.push(topic_hash.clone());
                }
                // update the mesh
                debug!("Updating mesh, new mesh: {:?}", peer_list);
                self.metrics
                    .peers_included(topic_hash, Inclusion::Random, peer_list.len());
                peers.extend(peer_list);
            }

            // too many peers - remove some
            if peers.len() > self.config.mesh_n_high() {
                debug!(
                    "HEARTBEAT: Mesh high. Topic: {} Contains: {} needs: {}",
                    topic_hash,
                    peers.len(),
                    self.config.mesh_n_high()
                );
                let excess_peer_no = peers.len() - self.config.mesh_n();

                // shuffle the peers and then sort by score ascending beginning with the worst
                let mut rng = thread_rng();
                let mut shuffled = peers.iter().cloned().collect::<Vec<_>>();
                shuffled.shuffle(&mut rng);
                shuffled.sort_by(|p1, p2| {
                    let score_p1 = *scores.get(p1).unwrap_or(&0.0);
                    let score_p2 = *scores.get(p2).unwrap_or(&0.0);

                    score_p1.partial_cmp(&score_p2).unwrap_or(Ordering::Equal)
                });
                // shuffle everything except the last retain_scores many peers (the best ones)
                shuffled[..peers.len() - self.config.retain_scores()].shuffle(&mut rng);

                // count total number of outbound peers
                let mut outbound = {
                    shuffled
                        .iter()
                        .filter(|p| self.connected_peers.is_outbound(p))
                        .count()
                };

                // remove the first excess_peer_no allowed (by outbound restrictions) peers adding
                // them to to_prune
                let mut removed = 0;
                for peer in shuffled {
                    if removed == excess_peer_no {
                        break;
                    }
                    if self.connected_peers.is_outbound(&peer) {
                        if outbound <= self.config.mesh_outbound_min() {
                            // do not remove anymore outbound peers
                            continue;
                        } else {
                            // an outbound peer gets removed
                            outbound -= 1;
                        }
                    }

                    // remove the peer
                    peers.remove(&peer);
                    let current_topic = to_prune.entry(peer).or_insert_with(Vec::new);
                    current_topic.push(topic_hash.clone());
                    removed += 1;
                }

                self.metrics
                    .peers_removed(topic_hash, Churn::Excess, removed);
            }

            // do we have enough outbound peers?
            if peers.len() >= self.config.mesh_n_low() {
                // count number of outbound peers we have
                let outbound = {
                    peers
                        .iter()
                        .filter(|p| self.connected_peers.is_outbound(p))
                        .count()
                };

                // if we have not enough outbound peers, graft to some new outbound peers
                if outbound < self.config.mesh_outbound_min() {
                    let needed = self.config.mesh_outbound_min() - outbound;
                    let peer_list = get_random_peers(
                        topic_peers,
                        &self.connected_peers,
                        topic_hash,
                        needed,
                        |peer| {
                            !peers.contains(peer)
                                && !explicit_peers.contains(peer)
                                && !backoffs.is_backoff_with_slack(topic_hash, peer)
                                && *scores.get(peer).unwrap_or(&0.0) >= 0.0
                                && self.connected_peers.is_outbound(peer)
                        },
                    );
                    for peer in &peer_list {
                        let current_topic = to_graft.entry(*peer).or_insert_with(Vec::new);
                        current_topic.push(topic_hash.clone());
                    }
                    // update the mesh
                    debug!("Updating mesh, new mesh: {:?}", peer_list);
                    self.metrics
                        .peers_included(topic_hash, Inclusion::Outbound, peer_list.len());
                    peers.extend(peer_list);
                }
            }

            // TODO: Review this opportunistic grafting section
            // // should we try to improve the mesh with opportunistic grafting?
            // if self.heartbeat_ticks % self.config.opportunistic_graft_ticks() == 0
            //     && peers.len() > 1
            //     && self.peer_score.is_some()
            // {
            //     if let Some((_, thresholds, _, _)) = &self.peer_score {
            //         // Opportunistic grafting works as follows: we check the median score of peers
            //         // in the mesh; if this score is below the opportunisticGraftThreshold, we
            //         // select a few peers at random with score over the median.
            //         // The intention is to (slowly) improve an underperforming mesh by introducing
            //         // good scoring peers that may have been gossiping at us. This allows us to
            //         // get out of sticky situations where we are stuck with poor peers and also
            //         // recover from churn of good peers.
            //
            //         // now compute the median peer score in the mesh
            //         let mut peers_by_score: Vec<_> = peers.iter().collect();
            //         peers_by_score.sort_by(|p1, p2| {
            //             let p1_score = *scores.get(p1).unwrap_or(&0.0);
            //             let p2_score = *scores.get(p2).unwrap_or(&0.0);
            //             p1_score.partial_cmp(&p2_score).unwrap_or(Ordering::Equal)
            //         });
            //
            //         let middle = peers_by_score.len() / 2;
            //         let median = if peers_by_score.len() % 2 == 0 {
            //             let sub_middle_peer = *peers_by_score
            //                 .get(middle - 1)
            //                 .expect("middle < vector length and middle > 0 since peers.len() > 0");
            //             let sub_middle_score = *scores.get(sub_middle_peer).unwrap_or(&0.0);
            //             let middle_peer =
            //                 *peers_by_score.get(middle).expect("middle < vector length");
            //             let middle_score = *scores.get(middle_peer).unwrap_or(&0.0);
            //
            //             (sub_middle_score + middle_score) * 0.5
            //         } else {
            //             *scores
            //                 .get(*peers_by_score.get(middle).expect("middle < vector length"))
            //                 .unwrap_or(&0.0)
            //         };
            //
            //         // if the median score is below the threshold, select a better peer (if any) and
            //         // GRAFT
            //         if median < thresholds.opportunistic_graft_threshold {
            //             let peer_list = get_random_peers(
            //                 topic_peers,
            //                 &self.connected_peers,
            //                 topic_hash,
            //                 self.config.opportunistic_graft_peers(),
            //                 |peer_id| {
            //                     !peers.contains(peer_id)
            //                         && !explicit_peers.contains(peer_id)
            //                         && !backoffs.is_backoff_with_slack(topic_hash, peer_id)
            //                         && *scores.get(peer_id).unwrap_or(&0.0) > median
            //                 },
            //             );
            //             for peer in &peer_list {
            //                 let current_topic = to_graft.entry(*peer).or_insert_with(Vec::new);
            //                 current_topic.push(topic_hash.clone());
            //             }
            //             // update the mesh
            //             debug!(
            //                 "Opportunistically graft in topic {} with peers {:?}",
            //                 topic_hash, peer_list
            //             );
            //             self.metrics.peers_included(topic_hash, Inclusion::Random, peer_list.len());
            //             peers.extend(peer_list);
            //         }
            //     }
            // }

            // Register the final count of peers in the mesh
            self.metrics.set_mesh_peers(topic_hash, peers.len())
        }

        // remove expired fanout topics
        {
            let fanout = &mut self.fanout; // help the borrow checker
            let fanout_ttl = self.config.fanout_ttl();
            self.fanout_last_pub.retain(|topic_hash, last_pub_time| {
                if *last_pub_time + fanout_ttl < Instant::now() {
                    debug!(
                        "HEARTBEAT: Fanout topic removed due to timeout. Topic: {:?}",
                        topic_hash
                    );
                    fanout.remove(topic_hash);
                    return false;
                }
                true
            });
        }

        // maintain fanout
        // check if our peers are still a part of the topic
        for (topic_hash, peers) in self.fanout.iter_mut() {
            let mut to_remove_peers = Vec::new();
            let publish_threshold = self.peer_score.publish_threshold();
            for peer in peers.iter() {
                // is the peer still subscribed to the topic?
                let peer_score = *scores.get(peer).unwrap_or(&0.0);
                match self.peer_topics.get(peer) {
                    Some(topics) => {
                        if !topics.contains(topic_hash) || peer_score < publish_threshold {
                            debug!(
                                "HEARTBEAT: Peer removed from fanout for topic: {:?}",
                                topic_hash
                            );
                            to_remove_peers.push(*peer);
                        }
                    }
                    None => {
                        // remove if the peer has disconnected
                        to_remove_peers.push(*peer);
                    }
                }
            }
            for to_remove in to_remove_peers {
                peers.remove(&to_remove);
            }

            // not enough peers
            if peers.len() < self.config.mesh_n() {
                debug!(
                    "HEARTBEAT: Fanout low. Contains: {:?} needs: {:?}",
                    peers.len(),
                    self.config.mesh_n()
                );
                let needed_peers = self.config.mesh_n() - peers.len();
                let explicit_peers = &self.explicit_peers;
                let new_peers = get_random_peers(
                    &self.topic_peers,
                    &self.connected_peers,
                    topic_hash,
                    needed_peers,
                    |peer_id| {
                        !peers.contains(peer_id)
                            && !explicit_peers.contains(peer_id)
                            && *scores.get(peer_id).unwrap_or(&0.0) < publish_threshold
                    },
                );
                peers.extend(new_peers);
            }
        }

        // TODO: Review this trace log
        // if self.peer_score.is_some() {
        //     trace!("Mesh message deliveries: {:?}", {
        //         self.mesh
        //             .iter()
        //             .map(|(t, peers)| {
        //                 (
        //                     t.clone(),
        //                     peers
        //                         .iter()
        //                         .map(|p| {
        //                             (
        //                                 *p,
        //                                 self.peer_score
        //                                     .as_ref()
        //                                     .expect("peer_score.is_some()")
        //                                     .0
        //                                     .mesh_message_deliveries(p, t)
        //                                     .unwrap_or(0.0),
        //                             )
        //                         })
        //                         .collect::<HashMap<PeerId, f64>>(),
        //                 )
        //             })
        //             .collect::<HashMap<TopicHash, HashMap<PeerId, f64>>>()
        //     })
        // }

        self.emit_gossip();

        // send graft/prunes
        if !to_graft.is_empty() | !to_prune.is_empty() {
            self.send_graft_prune(to_graft, to_prune, no_px);
        }

        // piggyback pooled control messages
        self.flush_control_pool();

        // shift the memcache
        self.mcache.shift();

        debug!("Completed Heartbeat");
        let duration = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.metrics.observe_heartbeat_duration(duration);
    }

    /// Emits gossip - Send IHAVE messages to a random set of gossip peers. This is applied to mesh
    /// and fanout peers
    fn emit_gossip(&mut self) {
        let mut rng = thread_rng();
        for (topic_hash, peers) in self.mesh.iter().chain(self.fanout.iter()) {
            let mut message_ids = self.mcache.get_gossip_message_ids(topic_hash);
            if message_ids.is_empty() {
                continue;
            }

            // if we are emitting more than GossipSubMaxIHaveLength message_ids, truncate the list
            if message_ids.len() > self.config.max_ihave_length() {
                // we do the truncation (with shuffling) per peer below
                debug!(
                    "too many messages for gossip; will truncate IHAVE list ({} messages)",
                    message_ids.len()
                );
            } else {
                // shuffle to emit in random order
                message_ids.shuffle(&mut rng);
            }

            // dynamic number of peers to gossip based on `gossip_factor` with minimum `gossip_lazy`
            let n_map = |m| {
                max(
                    self.config.gossip_lazy(),
                    (self.config.gossip_factor() * m as f64) as usize,
                )
            };
            // get gossip_lazy random peers
            let to_msg_peers = get_random_peers_dynamic(
                &self.topic_peers,
                &self.connected_peers,
                topic_hash,
                n_map,
                |peer| {
                    !peers.contains(peer)
                        && !self.explicit_peers.contains(peer)
                        && !self
                            .peer_score
                            .score_below_threshold(peer, |ts| ts.gossip_threshold)
                            .0
                },
            );

            debug!("Gossiping IHAVE to {} peers.", to_msg_peers.len());

            for peer in to_msg_peers {
                let mut peer_message_ids = message_ids.clone();

                if peer_message_ids.len() > self.config.max_ihave_length() {
                    // We do this per peer so that we emit a different set for each peer.
                    // we have enough redundancy in the system that this will significantly increase
                    // the message coverage when we do truncate.
                    peer_message_ids.partial_shuffle(&mut rng, self.config.max_ihave_length());
                    peer_message_ids.truncate(self.config.max_ihave_length());
                }

                // send an IHAVE message
                Self::control_pool_add(
                    &mut self.control_pool,
                    peer,
                    ControlAction::IHave {
                        topic_hash: topic_hash.clone(),
                        message_ids: peer_message_ids,
                    },
                );
            }
        }
    }

    /// Handles multiple GRAFT/PRUNE messages and coalesces them into chunked gossip control
    /// messages.
    fn send_graft_prune(
        &mut self,
        to_graft: HashMap<PeerId, Vec<TopicHash>>,
        mut to_prune: HashMap<PeerId, Vec<TopicHash>>,
        no_px: HashSet<PeerId>,
    ) {
        // handle the grafts and overlapping prunes per peer
        for (peer, topics) in to_graft.into_iter() {
            for topic in &topics {
                // inform scoring of graft
                self.peer_score.peer_score_graft(&peer, topic);

                // inform the handler of the peer being added to the mesh
                // If the peer did not previously exist in any mesh, inform the handler
                peer_added_to_mesh(
                    peer,
                    vec![topic],
                    &self.mesh,
                    self.peer_topics.get(&peer),
                    &mut self.events,
                    &self.connected_peers,
                );
            }
            let mut control_msgs: Vec<ControlAction> = topics
                .iter()
                .map(|topic_hash| ControlAction::Graft {
                    topic_hash: topic_hash.clone(),
                })
                .collect();

            // If there are prunes associated with the same peer add them.
            // NOTE: In this case a peer has been added to a topic mesh, and removed from another.
            // It therefore must be in at least one mesh and we do not need to inform the handler
            // of its removal from another.

            // The following prunes are not due to unsubscribing.
            let on_unsubscribe = false;
            if let Some(topics) = to_prune.remove(&peer) {
                let mut prunes = topics
                    .iter()
                    .map(|topic_hash| {
                        self.make_prune(
                            topic_hash,
                            &peer,
                            self.config.do_px() && !no_px.contains(&peer),
                            on_unsubscribe,
                        )
                    })
                    .collect::<Vec<_>>();
                control_msgs.append(&mut prunes);
            }

            // send the control messages
            if self.send_control_rpc_message(peer, control_msgs).is_err() {
                error!("Failed to send control messages. Message too large");
            }
        }

        // handle the remaining prunes
        // The following prunes are not due to unsubscribing.
        let on_unsubscribe = false;
        for (peer, topics) in to_prune.iter() {
            let mut remaining_prunes = Vec::new();
            for topic_hash in topics {
                let prune = self.make_prune(
                    topic_hash,
                    peer,
                    self.config.do_px() && !no_px.contains(peer),
                    on_unsubscribe,
                );
                remaining_prunes.push(prune);
                // inform the handler
                peer_removed_from_mesh(
                    *peer,
                    topic_hash,
                    &self.mesh,
                    self.peer_topics.get(peer),
                    &mut self.events,
                    &self.connected_peers,
                );
            }

            if self
                .send_control_rpc_message(*peer, remaining_prunes)
                .is_err()
            {
                error!("Failed to send prune messages. Message too large");
            }
        }
    }

    /// Helper function which forwards a message to mesh\[topic\] peers.
    ///
    /// Returns true if at least one peer was messaged.
    fn forward_msg(
        &mut self,
        msg_id: &MessageId,
        message: RawMessage,
        propagation_source: Option<&PeerId>,
        originating_peers: HashSet<PeerId>,
    ) -> Result<bool, PublishError> {
        // message is fully validated inform peer_score
        if let Some(peer) = propagation_source {
            self.peer_score
                .peer_score_deliver_message(peer, msg_id, &message.topic);
        }

        debug!("Forwarding message: {:?}", msg_id);
        let mut recipient_peers = HashSet::new();

        // Populate the recipient peers mapping
        {
            // Add explicit peers
            for peer_id in &self.explicit_peers {
                if let Some(topics) = self.peer_topics.get(peer_id) {
                    if Some(peer_id) != propagation_source
                        && !originating_peers.contains(peer_id)
                        && Some(peer_id) != message.source.as_ref()
                        && topics.contains(&message.topic)
                    {
                        recipient_peers.insert(*peer_id);
                    }
                }
            }

            // add mesh peers
            let topic = &message.topic;
            // mesh
            if let Some(mesh_peers) = self.mesh.get(topic) {
                for peer_id in mesh_peers {
                    if Some(peer_id) != propagation_source
                        && !originating_peers.contains(peer_id)
                        && Some(peer_id) != message.source.as_ref()
                    {
                        recipient_peers.insert(*peer_id);
                    }
                }
            }
        }

        // forward the message to peers
        if recipient_peers.is_empty() {
            return Ok(false);
        }

        let event: RpcProto = Rpc {
            subscriptions: Vec::new(),
            messages: vec![message.clone()],
            control_msgs: Vec::new(),
        }
        .into();

        let msg_bytes = event.encoded_len();
        for peer in recipient_peers.iter() {
            debug!("Sending message: {:?} to peer {:?}", msg_id, peer);
            self.send_rpc_message(*peer, event.clone())?;
            self.metrics.msg_sent(&message.topic, msg_bytes);
        }
        debug!("Completed forwarding message");
        Ok(true)
    }

    // adds a control action to control_pool
    fn control_pool_add(
        control_pool: &mut HashMap<PeerId, Vec<ControlAction>>,
        peer: PeerId,
        control: ControlAction,
    ) {
        control_pool
            .entry(peer)
            .or_insert_with(Vec::new)
            .push(control);
    }

    /// Takes each control action mapping and turns it into a message
    fn flush_control_pool(&mut self) {
        for (peer, controls) in self.control_pool.drain().collect::<Vec<_>>() {
            if self.send_control_rpc_message(peer, controls).is_err() {
                error!("Failed to flush control pool. Message too large");
            }
        }

        // This clears all pending IWANT messages
        self.pending_iwant_msgs.clear();
    }

    /// Send a [`Rpc`] message to a peer. This will wrap the message in an arc if it
    /// is not already an arc.
    fn send_rpc_message(&mut self, peer_id: PeerId, rpc: RpcProto) -> Result<(), PublishError> {
        // If the message is oversized, try and fragment it. If it cannot be fragmented, log an
        // error and drop the message (all individual messages should be small enough to fit in the
        // max_transmit_size)
        let messages = fragment_rpc_message(rpc, self.config.max_transmit_size())
            .map_err(|_| PublishError::MessageTooLarge)?;

        for message in messages {
            self.events.push_back(ToSwarm::NotifyHandler {
                peer_id,
                event: HandlerIn::Message(message),
                handler: NotifyHandler::Any,
            })
        }
        Ok(())
    }

    fn send_control_rpc_message(
        &mut self,
        peer_id: PeerId,
        control: Vec<ControlAction>,
    ) -> Result<(), PublishError> {
        let rpc = Rpc {
            subscriptions: Vec::new(),
            messages: Vec::new(),
            control_msgs: control,
        };

        self.send_rpc_message(peer_id, rpc.into())
    }

    fn send_subscription_rpc_message(
        &mut self,
        peer_id: PeerId,
        subscriptions: Vec<Subscription>,
    ) -> Result<(), PublishError> {
        let rpc = Rpc {
            subscriptions,
            messages: Vec::new(),
            control_msgs: Vec::new(),
        };

        self.send_rpc_message(peer_id, rpc.into())
    }

    fn on_connection_established(
        &mut self,
        ConnectionEstablished {
            peer_id,
            connection_id,
            endpoint,
            other_established,
            ..
        }: ConnectionEstablished,
    ) {
        // By default we assume a peer is only a floodsub peer.
        //
        // The protocol negotiation occurs once a message is sent/received. Once this happens we
        // update the type of peer that this is in order to determine which kind of routing should
        // occur.
        //
        // If the first connection is outbound and it is not a peer from peer exchange, we mark
        // it as outbound peer. This diverges from the Go implementation: we only consider a peer
        // as outbound peer if its first connection is outbound.
        let outbound =
            endpoint.is_dialer() && other_established == 0 && !self.px_peers.contains(&peer_id);
        self.connected_peers
            .track_connection(peer_id, connection_id, PeerKind::Floodsub, outbound);

        // Add the IP to the peer scoring system
        if let Some(ip) = get_ip_addr(endpoint.get_remote_address()) {
            self.peer_score.peer_score_add_ip(&peer_id, ip);
        } else {
            trace!(
                "Couldn't extract ip from endpoint of peer {} with endpoint {:?}",
                peer_id,
                endpoint
            )
        }

        if other_established == 0 {
            // Ignore connections from blacklisted peers.
            if self.blacklisted_peers.contains(&peer_id) {
                debug!("Ignoring connection from blacklisted peer: {}", peer_id);
            } else {
                debug!("New peer connected: {}", peer_id);
                // We need to send our subscriptions to the newly-connected node.
                let mut subscriptions = vec![];
                for topic_hash in self.mesh.keys() {
                    subscriptions.push(Subscription {
                        topic_hash: topic_hash.clone(),
                        action: SubscriptionAction::Subscribe,
                    });
                }

                if !subscriptions.is_empty() {
                    // send our subscriptions to the peer
                    if self
                        .send_subscription_rpc_message(peer_id, subscriptions)
                        .is_err()
                    {
                        error!("Failed to send subscriptions, message too large");
                    }
                }
            }

            // Insert an empty set of the topics of this peer until known.
            self.peer_topics.insert(peer_id, Default::default());

            self.peer_score.peer_score_add_peer(peer_id);
        }
    }

    fn on_connection_closed(
        &mut self,
        ConnectionClosed {
            peer_id,
            connection_id,
            endpoint,
            remaining_established,
            ..
        }: ConnectionClosed<<Self as NetworkBehaviour>::ConnectionHandler>,
    ) {
        // Remove IP from peer scoring system
        if let Some(ip) = get_ip_addr(endpoint.get_remote_address()) {
            self.peer_score.peer_score_remove_ip(&peer_id, &ip);
        } else {
            trace!(
                "Couldn't extract ip from endpoint of peer {} with endpoint {:?}",
                peer_id,
                endpoint
            )
        }

        if remaining_established != 0 {
            // Remove the connection from the list
            self.connected_peers
                .remove_connection(&peer_id, connection_id);

            // If there are more connections and this peer is in a mesh, inform the first connection
            // handler.
            if let Some(connection) = self.connected_peers.connections(&peer_id).next() {
                if let Some(topics) = self.peer_topics.get(&peer_id) {
                    for topic in topics {
                        if let Some(mesh_peers) = self.mesh.get(topic) {
                            if mesh_peers.contains(&peer_id) {
                                self.events.push_back(ToSwarm::NotifyHandler {
                                    peer_id,
                                    event: HandlerIn::JoinedMesh,
                                    handler: NotifyHandler::One(*connection),
                                });
                                break;
                            }
                        }
                    }
                }
            }
        } else {
            // remove from mesh, topic_peers, peer_topic and the fanout
            debug!("Peer disconnected: {}", peer_id);
            {
                let topics = match self.peer_topics.get(&peer_id) {
                    Some(topics) => topics,
                    None => {
                        debug_assert!(
                            self.blacklisted_peers.contains(&peer_id),
                            "Disconnected node not in connected list"
                        );
                        return;
                    }
                };

                // remove peer from all mappings
                for topic in topics {
                    // check the mesh for the topic
                    if let Some(mesh_peers) = self.mesh.get_mut(topic) {
                        // check if the peer is in the mesh and remove it
                        if mesh_peers.remove(&peer_id) {
                            self.metrics.peers_removed(topic, Churn::Dc, 1);
                            self.metrics.set_mesh_peers(topic, mesh_peers.len());
                        };
                    }

                    // remove from topic_peers
                    if let Some(peer_list) = self.topic_peers.get_mut(topic) {
                        if !peer_list.remove(&peer_id) {
                            // debugging purposes
                            warn!(
                                "Disconnected node: {} not in topic_peers peer list",
                                peer_id
                            );
                        }
                        self.metrics.set_topic_peers(topic, peer_list.len())
                    } else {
                        warn!(
                            "Disconnected node: {} with topic: {:?} not in topic_peers",
                            &peer_id, &topic
                        );
                    }

                    // remove from fanout
                    self.fanout
                        .get_mut(topic)
                        .map(|peers| peers.remove(&peer_id));
                }
            }

            // Forget px and outbound status for this peer
            self.px_peers.remove(&peer_id);

            // Remove peer from peer_topics and connected_peers
            // NOTE: It is possible the peer has already been removed from all mappings if it does not
            // support the protocol.
            self.peer_topics.remove(&peer_id);

            // If metrics are enabled, register the disconnection of a peer based on its protocol.
            let peer_kind = self
                .connected_peers
                .kind(&peer_id)
                .expect("Connected peer must be registered");
            self.metrics.peer_protocol_disconnected(peer_kind);

            self.connected_peers.remove_peer(&peer_id);

            self.peer_score.peer_score_remove_peer(&peer_id);
        }
    }

    fn on_address_change(
        &mut self,
        AddressChange {
            peer_id,
            old: endpoint_old,
            new: endpoint_new,
            ..
        }: AddressChange,
    ) {
        // Exchange IP in peer scoring system
        if let Some(ip) = get_ip_addr(endpoint_old.get_remote_address()) {
            self.peer_score.peer_score_remove_ip(&peer_id, &ip);
        } else {
            trace!(
                "Couldn't extract ip from endpoint of peer {} with endpoint {:?}",
                &peer_id,
                endpoint_old
            )
        }

        if let Some(ip) = get_ip_addr(endpoint_new.get_remote_address()) {
            self.peer_score.peer_score_add_ip(&peer_id, ip);
        } else {
            trace!(
                "Couldn't extract ip from endpoint of peer {} with endpoint {:?}",
                peer_id,
                endpoint_new
            )
        }
    }
}

impl<C, F> NetworkBehaviour for Behaviour<C, F>
where
    C: DataTransform + Send + 'static,
    F: TopicSubscriptionFilter + Send + 'static,
{
    type ConnectionHandler = Handler;
    type OutEvent = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(
            ProtocolUpgrade::new(&self.config),
            self.config.idle_timeout(),
        ))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        _: &Multiaddr,
        _: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(
            ProtocolUpgrade::new(&self.config),
            self.config.idle_timeout(),
        ))
    }

    fn on_swarm_event(&mut self, event: FromSwarm<Self::ConnectionHandler>) {
        match event {
            FromSwarm::ConnectionEstablished(connection_established) => {
                self.on_connection_established(connection_established)
            }
            FromSwarm::ConnectionClosed(connection_closed) => {
                self.on_connection_closed(connection_closed)
            }
            FromSwarm::AddressChange(address_change) => self.on_address_change(address_change),
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        propagation_source: PeerId,
        _connection_id: ConnectionId,
        handler_event: THandlerOutEvent<Self>,
    ) {
        match handler_event {
            HandlerEvent::PeerKind(kind) => {
                // We have identified the protocol this peer is using
                self.metrics.peer_protocol_connected(kind);

                if let PeerKind::NotSupported = kind {
                    debug!(
                        "Peer does not support gossipsub protocols. {}",
                        propagation_source
                    );
                    self.events
                        .push_back(ToSwarm::GenerateEvent(Event::GossipsubNotSupported {
                            peer_id: propagation_source,
                        }));
                } else if let Some(peer_kind) = self.connected_peers.kind(&propagation_source) {
                    // Only change the value if the old value is Floodsub (the default set in
                    // `NetworkBehaviour::on_event` with FromSwarm::ConnectionEstablished).
                    // All other PeerKind changes are ignored.
                    debug!(
                        "New peer type found: {} for peer: {}",
                        kind, propagation_source
                    );
                    if peer_kind.is_floodsub() {
                        self.connected_peers.set_kind(&propagation_source, kind);
                    }
                }
            }
            HandlerEvent::Rpc(rpc) => self.handle_received_rpc(&propagation_source, rpc),
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
        _: &mut impl PollParameters,
    ) -> Poll<ToSwarm<Self::OutEvent, THandlerInEvent<Self>>> {
        if let Some(event) = self.events.pop_front() {
            return Poll::Ready(event);
        }

        // update scores
        self.peer_score.poll_ticker_refresh_scores(cx);

        while let Poll::Ready(Some(tick)) = self.heartbeat.poll_next_unpin(cx) {
            self.on_heartbeat(tick);
        }

        Poll::Pending
    }
}

impl<C: DataTransform, F: TopicSubscriptionFilter> fmt::Debug for Behaviour<C, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Behaviour")
            .field("config", &self.config)
            .field("events", &self.events.len())
            .field("control_pool", &self.control_pool)
            .field("topic_peers", &self.topic_peers)
            .field("peer_topics", &self.peer_topics)
            .field("mesh", &self.mesh)
            .field("fanout", &self.fanout)
            .field("fanout_last_pub", &self.fanout_last_pub)
            .field("mcache", &self.mcache)
            .finish()
    }
}
