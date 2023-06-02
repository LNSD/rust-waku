use bytes::Bytes;
use libp2p::identity::PeerId;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::gossipsub::Message;

/// Macro for declaring message id types
macro_rules! declare_message_id_type {
    ($name: ident, $name_string: expr) => {
        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Vec<u8>);

        impl $name {
            pub fn new<T: Into<Vec<u8>>>(value: T) -> Self {
                Self(value.into())
            }

            pub fn new_from_slice(value: &[u8]) -> Self {
                Self(value.to_vec())
            }
        }

        impl From<Vec<u8>> for $name {
            fn from(value: Vec<u8>) -> Self {
                Self(value)
            }
        }

        impl From<Bytes> for $name {
            fn from(value: Bytes) -> Self {
                Self(value.to_vec())
            }
        }

        impl Into<Bytes> for $name {
            fn into(self) -> Bytes {
                Bytes::from(self.0)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", hex_fmt::HexFmt(&self.0))
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({})", $name_string, hex_fmt::HexFmt(&self.0))
            }
        }
    };
}

// A type for gossipsub message ids.
declare_message_id_type!(MessageId, "MessageId");

// A type for gossipsub fast message ids, not to confuse with "real" message ids.
//
// A fast-message-id is an optional message_id that can be used to filter duplicates quickly. On
// high intensive networks with lots of messages, where the message_id is based on the result of
// decompressed traffic, it is beneficial to specify a `fast-message-id` that can identify and
// filter duplicates quickly without performing the overhead of decompression.
declare_message_id_type!(FastMessageId, "FastMessageId");

pub(crate) fn default_message_id_fn(msg: &Message) -> MessageId {
    // default message id is: source + sequence number
    // NOTE: If either the peer_id or source is not provided, we set to 0;
    let mut source_string = if let Some(peer_id) = msg.source.as_ref() {
        peer_id.to_base58()
    } else {
        PeerId::from_bytes(&[0, 1, 0])
            .expect("Valid peer id")
            .to_base58()
    };
    source_string.push_str(&msg.sequence_number.unwrap_or_default().to_string());
    MessageId::new(source_string.into_bytes())
}
