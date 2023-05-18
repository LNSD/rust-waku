use prost::Message;
use sha2::digest::generic_array::GenericArray;
use sha2::digest::typenum::U32;
use sha2::digest::FixedOutput;
use sha2::{Digest, Sha256};

use waku_core::message::proto::waku::message::v1::WakuMessage;

use crate::gossipsub;
use crate::gossipsub::MessageId;

/// Fallback message ID function.
fn fallback_message_id_fn(message: &gossipsub::Message) -> MessageId {
    let mut hasher = Sha256::new();
    hasher.update(&message.data);
    let result = hasher.finalize_fixed();

    MessageId::new(result.as_ref())
}

/// Compute Waku v2 message's [deterministic hash](https://rfc.vac.dev/spec/14/#deterministic-message-hashing).
///
/// ```text
/// message_hash = sha256(concat(pubsub_topic, message.payload, message.content_topic, message.meta))
/// ```
fn compute_deterministic_message_hash(topic: &str, message: WakuMessage) -> GenericArray<u8, U32> {
    let mut hasher = Sha256::new();
    hasher.update(topic);
    hasher.update(message.payload);
    hasher.update(message.content_topic);
    if let Some(meta) = message.meta {
        hasher.update(meta);
    }
    hasher.finalize_fixed()
}

/// Deterministic message ID function based on the message deterministic hash specification.
/// See [RFC 14/WAKU2-MESSAGE](https://rfc.vac.dev/spec/14/#deterministic-message-hashing).
///
/// If the message is not a valid `WakuMessage` (e.g., failed deserialization), this
/// function calls `fallback_message_id_fn`.
pub fn deterministic_message_id_fn(message: &gossipsub::Message) -> MessageId {
    let pubsub_topic = message.topic.as_str();
    let waku_message = match WakuMessage::decode(&message.data[..]) {
        Ok(msg) => msg,
        _ => return fallback_message_id_fn(message),
    };

    let result = compute_deterministic_message_hash(pubsub_topic, waku_message);

    MessageId::new(result.as_ref())
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use hex_literal::hex;

    use waku_core::message::proto::waku::message::v1::WakuMessage;

    use super::compute_deterministic_message_hash;

    /// https://rfc.vac.dev/spec/14/#test-vectors (Test vector 1)
    #[test]
    fn test_deterministic_message_id_fn_rfc1_12bytes_meta() {
        // Given
        let pubsub_topic = "/waku/2/default-waku/proto";
        let waku_message = WakuMessage {
            payload: Bytes::from_static(&hex!("010203045445535405060708")),
            content_topic: String::from("/waku/2/default-content/proto"),
            meta: Some(Bytes::from_static(&hex!("73757065722d736563726574"))),
            ..Default::default()
        };

        // When
        let message_id = compute_deterministic_message_hash(pubsub_topic, waku_message);

        // Then
        assert_eq!(
            message_id.as_slice(),
            hex!("4fdde1099c9f77f6dae8147b6b3179aba1fc8e14a7bf35203fc253ee479f135f")
        );
    }

    /// https://rfc.vac.dev/spec/14/#test-vectors (Test vector 2)
    #[test]
    fn test_deterministic_message_id_fn_rfc2_64bytes_meta() {
        // Given
        let pubsub_topic = "/waku/2/default-waku/proto";
        let waku_message = WakuMessage {
            payload: Bytes::from_static(&hex!("010203045445535405060708")),
            content_topic: String::from("/waku/2/default-content/proto"),
            meta: Some(Bytes::from_static(&hex!(
                "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
                202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f"
            ))),
            ..Default::default()
        };

        // When
        let message_id = compute_deterministic_message_hash(pubsub_topic, waku_message);

        // Then
        assert_eq!(
            message_id.as_slice(),
            hex!("c32ed3b51f0c432be1c7f50880110e1a1a60f6067cd8193ca946909efe1b26ad")
        );
    }

    /// https://rfc.vac.dev/spec/14/#test-vectors (Test vector 3)
    #[test]
    fn test_deterministic_message_id_fn_rfc3_not_present_meta() {
        // Given
        let pubsub_topic = "/waku/2/default-waku/proto";
        let waku_message = WakuMessage {
            payload: Bytes::from_static(&hex!("010203045445535405060708")),
            content_topic: String::from("/waku/2/default-content/proto"),
            meta: None,
            ..Default::default()
        };

        // When
        let message_id = compute_deterministic_message_hash(pubsub_topic, waku_message);

        // Then
        assert_eq!(
            message_id.as_slice(),
            hex!("87619d05e563521d9126749b45bd4cc2430df0607e77e23572d874ed9c1aaa62")
        );
    }

    /// https://rfc.vac.dev/spec/14/#test-vectors (Test vector 4)
    #[test]
    fn test_deterministic_message_id_fn_rfc4_empty_payload() {
        // Given
        let pubsub_topic = "/waku/2/default-waku/proto";
        let waku_message = WakuMessage {
            payload: Bytes::new(),
            content_topic: String::from("/waku/2/default-content/proto"),
            meta: Some(Bytes::from_static(&hex!("73757065722d736563726574"))),
            ..Default::default()
        };

        // When
        let message_id = compute_deterministic_message_hash(pubsub_topic, waku_message);

        // Then
        assert_eq!(
            message_id.as_slice(),
            hex!("e1a9596237dbe2cc8aaf4b838c46a7052df6bc0d42ba214b998a8bfdbe8487d6")
        );
    }
}
