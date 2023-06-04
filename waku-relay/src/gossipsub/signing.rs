use bytes::Bytes;
use libp2p::identity::{Keypair, PeerId, SigningError};
use prost::Message as _;

use crate::gossipsub::rpc::MessageProto;

pub trait MessageSigner {
    fn author(&self) -> Option<&PeerId>;

    fn sign(&self, message: &mut MessageProto) -> Result<(), SigningError>;
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

    fn sign(&self, message: &mut MessageProto) -> Result<(), SigningError> {
        // Libp2p's pubsub message signature generation requires the `from` field to be set.
        let author = self.author.clone().to_bytes();
        message.from = Some(Bytes::from(author));

        let signature = generate_message_signature(message, &self.keypair)?;
        message.signature = Some(Bytes::from(signature));
        message.key = self.inline_key.as_deref().map(Bytes::copy_from_slice);

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

    fn sign(&self, message: &mut MessageProto) -> Result<(), SigningError> {
        let author = self.author.clone().to_bytes();
        message.from = Some(Bytes::from(author));
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

    fn sign(&self, message: &mut MessageProto) -> Result<(), SigningError> {
        let author = PeerId::random().to_bytes();
        message.from = Some(Bytes::from(author));
        Ok(())
    }
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

    fn sign(&self, _message: &mut MessageProto) -> Result<(), SigningError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use crate::gossipsub::validation::verify_message_signature;

    use super::*;

    fn test_keypair() -> Keypair {
        Keypair::generate_secp256k1()
    }

    fn test_message() -> MessageProto {
        MessageProto {
            from: None,
            data: Some(Bytes::from("test-message")),
            seqno: Some(Bytes::copy_from_slice(&42_u64.to_be_bytes())),
            topic: "test-topic".to_string(),
            signature: None,
            key: None,
        }
    }

    #[test]
    fn generate_signature() {
        //// Given
        let keypair = test_keypair();
        let message = test_message();

        //// When
        let signature = generate_message_signature(&message, &keypair).expect("signing failed");

        //// Then
        assert!(verify_message_signature(
            &message,
            &Bytes::from(signature),
            keypair.public()
        ));
    }

    mod libp2p_signer {
        use crate::gossipsub::validation::{MessageValidator, StrictMessageValidator};

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
            assert_matches!(&message.from, Some(from) => {
                let from_peer_id = PeerId::from_bytes(&from[..]).expect("invalid peer id");
                assert_eq!(from_peer_id, author);
            });
            assert_matches!(&message.signature, Some(signature) => {
                assert!(verify_message_signature(&message, signature, keypair.public()));
            });
            assert!(message.key.is_none()); // Already inlined in `from` field

            // Validate with strict message validator
            let validator = StrictMessageValidator::new();
            validator.validate(&message).expect("validation failed");
        }
    }

    mod author_only_signer {
        use crate::gossipsub::validation::{MessageValidator, PermissiveMessageValidator};

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
            assert_matches!(&message.from, Some(from) => {
                let from_peer_id = PeerId::from_bytes(&from[..]).expect("invalid peer id");
                assert_eq!(from_peer_id, author);
            });
            assert!(message.signature.is_none());
            assert!(message.key.is_none());

            // Validate with permissive validator
            let validator = PermissiveMessageValidator::new();
            assert!(validator.validate(&message).is_ok());
        }
    }

    mod random_author_signer {
        use crate::gossipsub::validation::{MessageValidator, PermissiveMessageValidator};

        use super::*;

        #[test]
        fn sign() {
            //// Given
            let signer = RandomAuthorSigner::new();

            let mut message = test_message();

            //// When
            signer.sign(&mut message).expect("signing failed");

            //// Then
            assert!(message.from.is_some());
            assert!(message.signature.is_none());
            assert!(message.key.is_none());

            // Validate with permissive validator
            let validator = PermissiveMessageValidator::new();
            assert!(validator.validate(&message).is_ok());
        }
    }

    mod noop_signer {
        use crate::gossipsub::validation::{MessageValidator, PermissiveMessageValidator};

        use super::*;

        #[test]
        fn sign() {
            //// Given
            let signer = NoopSigner::new();

            let mut message = test_message();

            //// When
            signer.sign(&mut message).expect("signing failed");

            //// Then
            assert!(message.from.is_none());
            assert!(message.signature.is_none());
            assert!(message.key.is_none());

            // Validate with anonymous validator
            let validator = PermissiveMessageValidator::new();
            assert!(validator.validate(&message).is_ok());
        }
    }
}
