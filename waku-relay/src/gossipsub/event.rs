use crate::gossipsub::{Message, MessageId, TopicHash};
use libp2p::PeerId;

/// Event that can be emitted by the gossipsub behaviour.
#[derive(Debug)]
pub enum Event {
    /// A message has been received.
    Message {
        /// The peer that forwarded us this message.
        propagation_source: PeerId,
        /// The [`MessageId`] of the message. This should be referenced by the application when
        /// validating a message (if required).
        message_id: MessageId,
        /// The decompressed message itself.
        message: Message,
    },
    /// A remote subscribed to a topic.
    Subscribed {
        /// Remote that has subscribed.
        peer_id: PeerId,
        /// The topic it has subscribed to.
        topic: TopicHash,
    },
    /// A remote unsubscribed from a topic.
    Unsubscribed {
        /// Remote that has unsubscribed.
        peer_id: PeerId,
        /// The topic it has subscribed from.
        topic: TopicHash,
    },
    /// A peer that does not support gossipsub has connected.
    GossipsubNotSupported { peer_id: PeerId },
}
