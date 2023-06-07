use libp2p::identity::{Keypair, PeerId, SigningError};
use prost::Message as _;

use crate::gossipsub::rpc::{MessageProto, MessageRpc};

pub trait MessageSigner {
    fn author(&self) -> Option<&PeerId>;

    fn sign(&self, message: &mut MessageRpc) -> Result<(), SigningError>;
}

/// A [`MessageSigner`] implementation that does not sign messages.
pub struct NoopSigner;

impl NoopSigner {
    pub fn new() -> Self {
        Self {}
    }
}

impl MessageSigner for NoopSigner {
    fn author(&self) -> Option<&PeerId> {
        None
    }

    fn sign(&self, _message: &mut MessageRpc) -> Result<(), SigningError> {
        Ok(())
    }
}

const SIGNING_PREFIX: &[u8] = b"libp2p-pubsub:";

/// Generate the Libp2p gossipsub signature for a message.
///
/// The signature is calculated over the bytes "libp2p-pubsub:<protobuf-message>".
pub fn generate_message_signature(
    message: &MessageProto,
    keypair: &Keypair,
) -> Result<Vec<u8>, SigningError> {
    let mut msg = message.clone();
    msg.signature = None;
    msg.key = None;

    // Construct the signature bytes
    let mut sign_bytes = Vec::with_capacity(SIGNING_PREFIX.len() + msg.encoded_len());
    sign_bytes.extend(SIGNING_PREFIX.to_vec());
    sign_bytes.extend(msg.encode_to_vec());

    keypair.sign(&sign_bytes)
}

/// A [`MessageSigner`] implementation that uses a [`Keypair`] to sign messages.
///
/// This signer will include the public key in the [`Message::key`] field if it is too large to be
/// inlined in the [`Message::from`] field.
///
/// The signature is calculated over the bytes "libp2p-pubsub:<protobuf-message>". This is specified
/// in the libp2p pubsub spec: https://github.com/libp2p/specs/tree/master/pubsub#message-signing
pub struct Libp2pSigner {
    keypair: Keypair,
    author: PeerId,
    inline_key: Option<Vec<u8>>,
}

impl Libp2pSigner {
    pub fn new(keypair: &Keypair) -> Self {
        let peer_id = keypair.public().to_peer_id();
        let key_enc = keypair.public().encode_protobuf();
        let inline_key = if key_enc.len() <= 42 {
            // The public key can be inlined in [`Message::from`], so we don't include it
            // specifically in the [`Message::key`] field.
            None
        } else {
            // Include the protobuf encoding of the public key in the message.
            Some(key_enc)
        };

        Self {
            keypair: keypair.clone(),
            author: peer_id,
            inline_key,
        }
    }
}

impl MessageSigner for Libp2pSigner {
    fn author(&self) -> Option<&PeerId> {
        Some(&self.author)
    }

    fn sign(&self, message: &mut MessageRpc) -> Result<(), SigningError> {
        // Libp2p's pubsub message signature generation requires the `from` field to be set.
        message.set_source(Some(self.author));

        let signature = generate_message_signature(message.as_proto(), &self.keypair)?;
        message.set_signature(Some(signature));
        message.set_key(self.inline_key.clone());

        Ok(())
    }
}

/// A [`MessageSigner`] implementation that uses a [`PeerId`] to sign messages.
pub struct AuthorOnlySigner {
    author: PeerId,
}

impl AuthorOnlySigner {
    pub fn new(author: PeerId) -> Self {
        Self { author }
    }
}

impl MessageSigner for AuthorOnlySigner {
    fn author(&self) -> Option<&PeerId> {
        Some(&self.author)
    }

    fn sign(&self, message: &mut MessageRpc) -> Result<(), SigningError> {
        message.set_source(Some(self.author));
        Ok(())
    }
}

/// A [`MessageSigner`] implementation that uses a random [`PeerId`] to sign messages.
pub struct RandomAuthorSigner;

impl RandomAuthorSigner {
    pub fn new() -> Self {
        Self {}
    }
}

impl MessageSigner for RandomAuthorSigner {
    fn author(&self) -> Option<&PeerId> {
        None
    }

    fn sign(&self, message: &mut MessageRpc) -> Result<(), SigningError> {
        message.set_source(Some(PeerId::random()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use bytes::Bytes;

    use crate::gossipsub::signing::validator::verify_message_signature;

    use super::*;

    fn test_keypair() -> Keypair {
        Keypair::generate_secp256k1()
    }

    fn test_message() -> MessageRpc {
        MessageRpc::new_with_sequence_number(
            "test-topic".to_string(),
            b"test-message".to_vec(),
            Some(42),
        )
    }

    #[test]
    fn generate_signature() {
        //// Given
        let keypair = test_keypair();
        let message = test_message();

        //// When
        let signature =
            generate_message_signature(message.as_proto(), &keypair).expect("signing failed");

        //// Then
        assert!(verify_message_signature(
            message.as_proto(),
            &Bytes::from(signature),
            &keypair.public()
        ));
    }

    mod libp2p_signer {
        use crate::gossipsub::signing::validator::{MessageValidator, StrictMessageValidator};

        use super::*;

        #[test]
        fn sign() {
            //// Given
            let keypair = test_keypair();
            let author = keypair.public().to_peer_id();

            let signer = Libp2pSigner::new(&keypair);

            let mut message = test_message();

            //// When
            signer.sign(&mut message).expect("signing failed");

            //// Then
            assert_matches!(message.source(), Some(from_peer_id) => {
                assert_eq!(from_peer_id, author);
            });
            assert_matches!(message.signature(), Some(signature) => {
                assert!(verify_message_signature(message.as_proto(), signature, &keypair.public()));
            });
            assert!(message.key().is_none()); // Already inlined in `from` field

            // Validate with strict message validator
            let validator = StrictMessageValidator::new();
            assert_matches!(validator.validate(&message), Ok(()));
        }
    }

    mod author_only_signer {
        use crate::gossipsub::signing::validator::{MessageValidator, PermissiveMessageValidator};

        use super::*;

        #[test]
        fn sign() {
            //// Given
            let keypair = test_keypair();
            let author = keypair.public().to_peer_id();

            let signer = AuthorOnlySigner::new(author);

            let mut message = test_message();

            //// When
            signer.sign(&mut message).expect("signing failed");

            //// Then
            assert_matches!(message.source(), Some(from_peer_id) => {
                assert_eq!(from_peer_id, author);
            });
            assert!(message.signature().is_none());
            assert!(message.key().is_none());

            // Validate with permissive validator
            let validator = PermissiveMessageValidator::new();
            assert_matches!(validator.validate(&message), Ok(()));
        }
    }

    mod random_author_signer {
        use crate::gossipsub::signing::validator::{MessageValidator, PermissiveMessageValidator};

        use super::*;

        #[test]
        fn sign() {
            //// Given
            let signer = RandomAuthorSigner::new();

            let mut message = test_message();

            //// When
            signer.sign(&mut message).expect("signing failed");

            //// Then
            assert!(message.source().is_some());
            assert!(message.signature().is_none());
            assert!(message.key().is_none());

            // Validate with permissive validator
            let validator = PermissiveMessageValidator::new();
            assert_matches!(validator.validate(&message), Ok(()));
        }
    }

    mod noop_signer {
        use crate::gossipsub::signing::validator::{MessageValidator, PermissiveMessageValidator};

        use super::*;

        #[test]
        fn sign() {
            //// Given
            let signer = NoopSigner::new();

            let mut message = test_message();

            //// When
            signer.sign(&mut message).expect("signing failed");

            //// Then
            assert!(message.source().is_none());
            assert!(message.signature().is_none());
            assert!(message.key().is_none());

            // Validate with anonymous validator
            let validator = PermissiveMessageValidator::new();
            assert_matches!(validator.validate(&message), Ok(()));
        }
    }
}
