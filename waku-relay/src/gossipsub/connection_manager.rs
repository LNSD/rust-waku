use std::collections::{HashMap, HashSet};

use libp2p::identity::PeerId;
use libp2p::swarm::ConnectionId;

use crate::gossipsub::types::PeerKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PeerConnections {
    /// The kind of protocol the peer supports.
    pub(crate) kind: PeerKind,
    /// Its current connections.
    pub(crate) connections: Vec<ConnectionId>,
}

#[derive(Debug, Default)]
pub(crate) struct ConnectionManager {
    /// A set of connected peers, indexed by their [`PeerId`] tracking both the [`PeerKind`] and
    /// the set of [`ConnectionId`]s.
    peers: HashMap<PeerId, PeerConnections>,

    /// Set of connected outbound peers (we only consider true outbound peers found through
    /// discovery and not by peer exchange).
    outbound_peers: HashSet<PeerId>,
}

impl ConnectionManager {
    pub(crate) fn new() -> Self {
        Default::default()
    }

    pub(crate) fn peers(&self) -> impl Iterator<Item = &PeerId> {
        self.peers.keys()
    }

    pub(crate) fn floodsub_peers(&self) -> impl Iterator<Item = &PeerId> {
        self.peers
            .iter()
            .filter(|(_, peer)| peer.kind.is_floodsub())
            .map(|(peer_id, _)| peer_id)
    }

    pub(crate) fn gossipsub_peers(&self) -> impl Iterator<Item = &PeerId> {
        self.peers
            .iter()
            .filter(|(_, peer)| peer.kind.is_gossipsub())
            .map(|(peer_id, _)| peer_id)
    }

    pub(crate) fn is_outbound(&self, peer_id: &PeerId) -> bool {
        self.outbound_peers.contains(peer_id)
    }

    /// List all known peers and their associated protocol.
    pub(crate) fn peer_protocol(&self) -> impl Iterator<Item = (&PeerId, &PeerKind)> {
        self.peers
            .iter()
            .map(|(peer_id, peer)| (peer_id, &peer.kind))
    }

    pub(crate) fn kind(&self, peer_id: &PeerId) -> Option<PeerKind> {
        self.peers.get(peer_id).map(|peer| peer.kind)
    }

    pub(crate) fn connections(&self, peer_id: &PeerId) -> impl Iterator<Item = &ConnectionId> {
        self.peers
            .get(peer_id)
            .map(|peer| peer.connections.iter())
            .unwrap_or_default()
    }

    pub(crate) fn track_connection(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        kind: PeerKind,
        outbound: bool,
    ) {
        self.peers
            .entry(peer_id)
            .or_insert_with(|| PeerConnections {
                kind,
                connections: Vec::new(),
            })
            .connections
            .push(connection_id);

        if outbound {
            self.outbound_peers.insert(peer_id);
        }
    }

    pub(crate) fn remove_connection(&mut self, peer_id: &PeerId, connection_id: ConnectionId) {
        if let Some(peer) = self.peers.get_mut(peer_id) {
            peer.connections.retain(|c| *c != connection_id);
            if peer.connections.is_empty() {
                self.peers.remove(peer_id);
            }
        }
    }

    pub(crate) fn set_kind(&mut self, peer_id: &PeerId, kind: PeerKind) {
        if let Some(peer) = self.peers.get_mut(peer_id) {
            peer.kind = kind;
        }
    }

    pub(crate) fn remove_peer(&mut self, peer_id: &PeerId) {
        self.peers.remove(peer_id);
        self.outbound_peers.remove(peer_id);
    }

    pub(crate) fn len(&self) -> usize {
        self.peers.len()
    }
}
