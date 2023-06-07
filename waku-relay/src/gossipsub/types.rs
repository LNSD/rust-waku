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

//! A collection of types using the Gossipsub system.
use std::fmt;

use libp2p::swarm::ConnectionId;
use libp2p::PeerId;
use prometheus_client::encoding::EncodeLabelValue;
use prost::Message as _;

use crate::gossipsub::mcache::CachedMessage;
use crate::gossipsub::message_id::MessageId;
use crate::gossipsub::rpc::{MessageProto, MessageRpc};
use crate::gossipsub::topic::TopicHash;

/// Validation kinds from the application for received messages.
#[derive(Debug)]
pub enum MessageAcceptance {
    /// The message is considered valid, and it should be delivered and forwarded to the network.
    Accept,
    /// The message is considered invalid, and it should be rejected and trigger the P₄ penalty.
    Reject,
    /// The message is neither delivered nor forwarded to the network, but the router does not
    /// trigger the P₄ penalty.
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PeerConnections {
    /// The kind of protocol the peer supports.
    pub(crate) kind: PeerKind,
    /// Its current connections.
    pub(crate) connections: Vec<ConnectionId>,
}

/// Describes the types of peers that can exist in the gossipsub context.
#[derive(Debug, Clone, PartialEq, Hash, EncodeLabelValue, Eq)]
pub enum PeerKind {
    /// A gossipsub 1.1 peer.
    Gossipsubv1_1,
    /// A gossipsub 1.0 peer.
    Gossipsub,
    /// A floodsub peer.
    Floodsub,
    /// The peer doesn't support any of the protocols.
    NotSupported,
}

impl fmt::Display for PeerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Self::NotSupported => "Not Supported",
            Self::Floodsub => "Floodsub",
            Self::Gossipsub => "Gossipsub v1.0",
            Self::Gossipsubv1_1 => "Gossipsub v1.1",
        };
        f.write_str(kind)
    }
}

/// A message received by the gossipsub system and stored locally in caches..
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct RawMessage {
    /// Id of the peer that published this message.
    pub source: Option<PeerId>,
    /// Content of the message. Its meaning is out of scope of this library.
    pub data: Vec<u8>,
    /// A random sequence number.
    pub sequence_number: Option<u64>,
    /// The topic this message belongs to
    pub topic: TopicHash,
    /// The signature of the message if it's signed.
    pub signature: Option<Vec<u8>>,
    /// The public key of the message if it is signed and the source [`PeerId`] cannot be inlined.
    pub key: Option<Vec<u8>>,
}

impl RawMessage {
    /// Calculates the encoded length of this message (used for calculating metrics).
    pub fn raw_protobuf_len(&self) -> usize {
        let message: MessageProto = self.clone().into();
        message.encoded_len()
    }
}

impl From<MessageRpc> for RawMessage {
    fn from(rpc: MessageRpc) -> Self {
        Self {
            source: rpc.source(),
            data: rpc.data().to_vec(),
            sequence_number: rpc.sequence_number(),
            topic: rpc.topic().into(),
            signature: rpc.signature().map(|s| s.to_vec()),
            key: rpc.key().map(|s| s.to_vec()),
        }
    }
}

impl From<CachedMessage> for RawMessage {
    fn from(message: CachedMessage) -> Self {
        Self {
            source: message.source,
            data: message.data,
            sequence_number: message.sequence_number,
            topic: message.topic,
            signature: message.signature,
            key: message.key,
        }
    }
}

/// The message sent to the user after a [`RawMessage`] has been transformed by a
/// [`crate::DataTransform`].
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Message {
    /// Id of the peer that published this message.
    pub source: Option<PeerId>,

    /// Content of the message.
    pub data: Vec<u8>,

    /// A random sequence number.
    pub sequence_number: Option<u64>,

    /// The topic this message belongs to
    pub topic: TopicHash,
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Message")
            .field(
                "data",
                &format_args!("{:<20}", &hex_fmt::HexFmt(&self.data)),
            )
            .field("source", &self.source)
            .field("sequence_number", &self.sequence_number)
            .field("topic", &self.topic)
            .finish()
    }
}

/// A subscription received by the gossipsub system.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Subscription {
    /// Action to perform.
    pub action: SubscriptionAction,
    /// The topic from which to subscribe or unsubscribe.
    pub topic_hash: TopicHash,
}

/// Action that a subscription wants to perform.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SubscriptionAction {
    /// The remote wants to subscribe to the given topic.
    Subscribe,
    /// The remote wants to unsubscribe from the given topic.
    Unsubscribe,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PeerInfo {
    pub peer_id: Option<PeerId>,
    //TODO add this when RFC: Signed Address Records got added to the spec (see pull request
    // https://github.com/libp2p/specs/pull/217)
    //pub signed_peer_record: ?,
}

/// A Control message received by the gossipsub system.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ControlAction {
    /// Node broadcasts known messages per topic - IHave control message.
    IHave {
        /// The topic of the messages.
        topic_hash: TopicHash,
        /// A list of known message ids (peer_id + sequence _number) as a string.
        message_ids: Vec<MessageId>,
    },
    /// The node requests specific message ids (peer_id + sequence _number) - IWant control message.
    IWant {
        /// A list of known message ids (peer_id + sequence _number) as a string.
        message_ids: Vec<MessageId>,
    },
    /// The node has been added to the mesh - Graft control message.
    Graft {
        /// The mesh topic the peer should be added to.
        topic_hash: TopicHash,
    },
    /// The node has been removed from the mesh - Prune control message.
    Prune {
        /// The mesh topic the peer should be removed from.
        topic_hash: TopicHash,
        /// A list of peers to be proposed to the removed peer as peer exchange
        peers: Vec<PeerInfo>,
        /// The backoff time in seconds before we allow to reconnect
        backoff: Option<u64>,
    },
}

/// An RPC received/sent.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Rpc {
    /// List of messages that were part of this RPC query.
    pub messages: Vec<RawMessage>,
    /// List of subscriptions.
    pub subscriptions: Vec<Subscription>,
    /// List of Gossipsub control messages.
    pub control_msgs: Vec<ControlAction>,
}

impl fmt::Debug for Rpc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut b = f.debug_struct("GossipsubRpc");
        if !self.messages.is_empty() {
            b.field("messages", &self.messages);
        }
        if !self.subscriptions.is_empty() {
            b.field("subscriptions", &self.subscriptions);
        }
        if !self.control_msgs.is_empty() {
            b.field("control_msgs", &self.control_msgs);
        }
        b.finish()
    }
}
