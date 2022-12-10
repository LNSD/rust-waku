//! Error types that can result from Waku relay.

use libp2p::gossipsub::error::PublishError as GossipsubPublishError;
use libp2p::gossipsub::error::SubscriptionError as GossipsubSubscriptionError;

use crate::error::PublishError::{Duplicate, GossipsubError, InsufficientPeers, MessageTooLarge};
use crate::error::SubscriptionError::NotAllowed;

/// Error associated with publishing a Waku message.
#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    /// This message has already been published.
    #[error("duplicate message")]
    Duplicate,
    /// There were no peers to send this message to.
    #[error("insufficient peers")]
    InsufficientPeers,
    /// The overall message was too large. This could be due to excessive topics or an excessive
    /// message size.
    #[error("message too large")]
    MessageTooLarge,
    /// Unknown Waku relay publish error.
    #[error("unknown gossipsub publish error")]
    GossipsubError(GossipsubPublishError),
}

impl From<GossipsubPublishError> for PublishError {
    fn from(err: GossipsubPublishError) -> Self {
        match err {
            GossipsubPublishError::Duplicate => Duplicate,
            GossipsubPublishError::InsufficientPeers => InsufficientPeers,
            GossipsubPublishError::MessageTooLarge => MessageTooLarge,
            _ => GossipsubError(err),
        }
    }
}

/// Error associated with subscribing to a topic.
#[derive(Debug, thiserror::Error)]
pub enum SubscriptionError {
    /// Couldn't publish our subscription.
    #[error("subscription publication failed")]
    PublishError(PublishError),
    /// We are not allowed to subscribe to this topic by the subscription filter.
    #[error("subscription not allowed")]
    NotAllowed,
}

impl From<GossipsubSubscriptionError> for SubscriptionError {
    fn from(err: GossipsubSubscriptionError) -> Self {
        match err {
            GossipsubSubscriptionError::PublishError(e) => {
                SubscriptionError::PublishError(e.into())
            }
            GossipsubSubscriptionError::NotAllowed => NotAllowed,
        }
    }
}
