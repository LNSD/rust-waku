use std::collections::HashMap;
use std::net::IpAddr;
use std::task::{Context, Poll};
use std::time::Instant;

use futures::StreamExt;
use futures_ticker::Ticker;
use libp2p::identity::PeerId;

use crate::gossipsub::gossip_promises::GossipPromises;
use crate::gossipsub::message_id::MessageId;
use crate::gossipsub::metrics::Metrics;
use crate::gossipsub::peer_score::{PeerScore, PeerScoreThresholds, RejectReason};
use crate::gossipsub::topic::TopicHash;
use crate::gossipsub::TopicScoreParams;

pub(crate) trait PeerScoreService {
    fn peer_score(&self, peer_id: &PeerId) -> Option<f64>;

    fn publish_threshold(&self) -> f64;

    /// Determines if a peer's score is below a given `PeerScoreThreshold` chosen via the
    /// `threshold` parameter.
    fn score_below_threshold(
        &self,
        peer_id: &PeerId,
        threshold: fn(&PeerScoreThresholds) -> f64,
    ) -> (bool, f64);

    fn peer_score_reject_message(
        &mut self,
        from: &PeerId,
        msg_id: &MessageId,
        topic_hash: &TopicHash,
        reason: RejectReason,
    );

    fn gossip_promises_reject_message(&mut self, message_id: &MessageId, reason: &RejectReason);

    fn peer_score_duplicated_message(
        &mut self,
        peer_id: &PeerId,
        message_id: &MessageId,
        topic: &TopicHash,
    );

    fn peer_score_validate_message(
        &mut self,
        peer_id: &PeerId,
        message_id: &MessageId,
        topic: &TopicHash,
    );

    fn peer_score_reject_invalid_message(&mut self, from: &PeerId, topic_hash: &TopicHash);

    fn peer_score_set_topic_params(&mut self, topic_hash: TopicHash, params: TopicScoreParams);

    fn peer_score_set_application_score(&mut self, peer_id: &PeerId, new_score: f64) -> bool;

    fn peer_score_graft(&mut self, peer_id: &PeerId, topic_hash: &TopicHash);

    fn peer_score_prune(&mut self, peer_id: &PeerId, topic_hash: &TopicHash);

    fn peer_score_deliver_message(
        &mut self,
        peer_id: &PeerId,
        msg_id: &MessageId,
        topic_hash: &TopicHash,
    );

    fn peer_score_add_penalty(&mut self, peer_id: &PeerId, penalty: usize);

    fn peer_score_add_ip(&mut self, peer_id: &PeerId, ip: IpAddr);

    fn peer_score_remove_ip(&mut self, peer_id: &PeerId, ip: &IpAddr);

    fn peer_score_add_peer(&mut self, peer_id: PeerId);

    fn peer_score_remove_peer(&mut self, peer_id: &PeerId);

    fn get_broken_promises(&mut self) -> HashMap<PeerId, usize>;

    fn promises_contains(&self, message_id: &MessageId) -> bool;

    fn promises_add(&mut self, peer_id: PeerId, iwant_ids_vec: &[MessageId], expires: Instant);

    fn promises_message_delivered(&mut self, message_id: &MessageId);

    fn peer_score_metric_score(
        &mut self,
        peer_id: &PeerId,
        metrics: &mut Box<dyn Metrics + Send>,
    ) -> f64;

    fn poll_ticker_refresh_scores(&mut self, cx: &mut Context<'_>);
}

pub struct NoopPeerScoreService;

impl NoopPeerScoreService {
    pub fn new() -> Self {
        Self {}
    }
}

impl PeerScoreService for NoopPeerScoreService {
    fn peer_score(&self, _peer_id: &PeerId) -> Option<f64> {
        None
    }

    fn publish_threshold(&self) -> f64 {
        0.0
    }

    fn score_below_threshold(
        &self,
        _peer_id: &PeerId,
        _threshold: fn(&PeerScoreThresholds) -> f64,
    ) -> (bool, f64) {
        (false, 0.0)
    }

    fn peer_score_reject_message(
        &mut self,
        _from: &PeerId,
        _msg_id: &MessageId,
        _topic_hash: &TopicHash,
        _reason: RejectReason,
    ) {
        // Do nothing
    }

    fn gossip_promises_reject_message(&mut self, _message_id: &MessageId, _reason: &RejectReason) {
        // Do nothing
    }

    fn peer_score_duplicated_message(
        &mut self,
        _peer_id: &PeerId,
        _message_id: &MessageId,
        _topic: &TopicHash,
    ) {
        // Do nothing
    }

    fn peer_score_validate_message(
        &mut self,
        _peer_id: &PeerId,
        _message_id: &MessageId,
        _topic: &TopicHash,
    ) {
        // Do nothing
    }

    fn peer_score_reject_invalid_message(&mut self, _from: &PeerId, _topic_hash: &TopicHash) {
        // Do nothing
    }

    fn peer_score_set_topic_params(&mut self, _topic_hash: TopicHash, _params: TopicScoreParams) {
        // Do nothing
    }

    fn peer_score_set_application_score(&mut self, _peer_id: &PeerId, _new_score: f64) -> bool {
        false
    }

    fn peer_score_graft(&mut self, _peer_id: &PeerId, _topic_hash: &TopicHash) {
        // Do nothing
    }

    fn peer_score_prune(&mut self, _peer_id: &PeerId, _topic_hash: &TopicHash) {
        // Do nothing
    }

    fn peer_score_deliver_message(
        &mut self,
        _peer_id: &PeerId,
        _msg_id: &MessageId,
        _topic_hash: &TopicHash,
    ) {
        // Do nothing
    }

    fn peer_score_add_penalty(&mut self, _peer_id: &PeerId, _penalty: usize) {
        // Do nothing
    }

    fn peer_score_add_ip(&mut self, _peer_id: &PeerId, _ip: IpAddr) {
        // Do nothing
    }

    fn peer_score_remove_ip(&mut self, _peer_id: &PeerId, _ip: &IpAddr) {
        // Do nothing
    }

    fn peer_score_add_peer(&mut self, _peer_id: PeerId) {
        // Do nothing
    }

    fn peer_score_remove_peer(&mut self, _peer_id: &PeerId) {
        // Do nothing
    }

    fn get_broken_promises(&mut self) -> HashMap<PeerId, usize> {
        Default::default()
    }

    fn promises_contains(&self, _message_id: &MessageId) -> bool {
        true
    }

    fn promises_add(&mut self, _peer_id: PeerId, _iwant_ids_vec: &[MessageId], _expires: Instant) {
        // Do nothing
    }

    fn promises_message_delivered(&mut self, _message_id: &MessageId) {
        // Do nothing
    }

    fn peer_score_metric_score(
        &mut self,
        _peer_id: &PeerId,
        _metrics: &mut Box<dyn Metrics + Send>,
    ) -> f64 {
        0.0
    }

    fn poll_ticker_refresh_scores(&mut self, _cx: &mut Context<'_>) {
        // Do nothing
    }
}

pub(crate) struct GossipsubPeerScoreService {
    scores: PeerScore,
    thresholds: PeerScoreThresholds,
    ticker: Ticker,
    promises: GossipPromises,
}

impl GossipsubPeerScoreService {
    pub(crate) fn new(scores: PeerScore, thresholds: PeerScoreThresholds, ticker: Ticker) -> Self {
        Self {
            scores,
            thresholds,
            ticker,
            promises: GossipPromises::default(),
        }
    }
}

impl PeerScoreService for GossipsubPeerScoreService {
    fn peer_score(&self, peer_id: &PeerId) -> Option<f64> {
        Some(self.scores.score(peer_id))
    }

    fn publish_threshold(&self) -> f64 {
        self.thresholds.publish_threshold
    }

    fn score_below_threshold(
        &self,
        peer_id: &PeerId,
        threshold: fn(&PeerScoreThresholds) -> f64,
    ) -> (bool, f64) {
        let score = self.scores.score(peer_id);
        if score < threshold(&self.thresholds) {
            return (true, score);
        }

        (false, score)
    }

    fn peer_score_reject_message(
        &mut self,
        from: &PeerId,
        msg_id: &MessageId,
        topic_hash: &TopicHash,
        reason: RejectReason,
    ) {
        self.scores.reject_message(from, msg_id, topic_hash, reason);
    }

    fn gossip_promises_reject_message(&mut self, message_id: &MessageId, reason: &RejectReason) {
        self.promises.reject_message(message_id, reason);
    }

    fn peer_score_duplicated_message(
        &mut self,
        peer_id: &PeerId,
        message_id: &MessageId,
        topic: &TopicHash,
    ) {
        self.scores.duplicated_message(peer_id, message_id, topic);
    }

    fn peer_score_validate_message(
        &mut self,
        peer_id: &PeerId,
        message_id: &MessageId,
        topic: &TopicHash,
    ) {
        self.scores.validate_message(peer_id, message_id, topic);
    }

    fn peer_score_reject_invalid_message(&mut self, from: &PeerId, topic_hash: &TopicHash) {
        self.scores.reject_invalid_message(from, topic_hash);
    }

    fn peer_score_set_topic_params(&mut self, topic_hash: TopicHash, params: TopicScoreParams) {
        self.scores.set_topic_params(topic_hash, params);
    }

    fn peer_score_set_application_score(&mut self, peer_id: &PeerId, new_score: f64) -> bool {
        self.scores.set_application_score(peer_id, new_score)
    }

    fn peer_score_graft(&mut self, peer_id: &PeerId, topic_hash: &TopicHash) {
        self.scores.graft(peer_id, topic_hash.clone());
    }

    fn peer_score_prune(&mut self, peer_id: &PeerId, topic_hash: &TopicHash) {
        self.scores.prune(peer_id, topic_hash.clone());
    }

    fn peer_score_deliver_message(
        &mut self,
        peer_id: &PeerId,
        msg_id: &MessageId,
        topic_hash: &TopicHash,
    ) {
        self.scores.deliver_message(peer_id, msg_id, topic_hash);
    }

    fn peer_score_add_penalty(&mut self, peer_id: &PeerId, penalty: usize) {
        self.scores.add_penalty(peer_id, penalty);
    }

    fn peer_score_add_ip(&mut self, peer_id: &PeerId, ip: IpAddr) {
        self.scores.add_ip(peer_id, ip);
    }

    fn peer_score_remove_ip(&mut self, peer_id: &PeerId, ip: &IpAddr) {
        self.scores.remove_ip(peer_id, ip);
    }

    fn peer_score_add_peer(&mut self, peer_id: PeerId) {
        self.scores.add_peer(peer_id);
    }

    fn peer_score_remove_peer(&mut self, peer_id: &PeerId) {
        self.scores.remove_peer(peer_id);
    }

    fn get_broken_promises(&mut self) -> HashMap<PeerId, usize> {
        self.promises.get_broken_promises()
    }

    fn promises_contains(&self, message_id: &MessageId) -> bool {
        !self.promises.contains(message_id)
    }

    fn promises_add(&mut self, peer_id: PeerId, iwant_ids_vec: &[MessageId], expires: Instant) {
        self.promises.add_promise(peer_id, iwant_ids_vec, expires);
    }

    fn promises_message_delivered(&mut self, message_id: &MessageId) {
        self.promises.message_delivered(message_id);
    }

    fn peer_score_metric_score(
        &mut self,
        peer_id: &PeerId,
        metrics: &mut Box<dyn Metrics + Send>,
    ) -> f64 {
        self.scores.metric_score(peer_id, metrics)
    }

    fn poll_ticker_refresh_scores(&mut self, cx: &mut Context<'_>) {
        while let Poll::Ready(Some(_)) = self.ticker.poll_next_unpin(cx) {
            self.scores.refresh_scores();
        }
    }
}
