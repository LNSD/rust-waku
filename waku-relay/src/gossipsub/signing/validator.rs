use libp2p::identity::PublicKey;
use log::{debug, warn};
use prost::Message as _;

use crate::gossipsub::error::MessageValidationError;
use crate::gossipsub::rpc::{MessageProto, MessageRpc};

pub(crate) const SIGNING_PREFIX: &[u8] = b"libp2p-pubsub:";

pub trait MessageValidator {
    fn validate(&self, message: &MessageRpc) -> Result<(), MessageValidationError>;
}

/// Extract the public key from a message.
///
/// If the key field is not present, the source field is used.
fn extract_key(message: &MessageRpc) -> Result<PublicKey, MessageValidationError> {
    // If the key field os present, use it
    if let Some(key) = message.key() {
        return PublicKey::try_decode_protobuf(key).map_err(|_| MessageValidationError::InvalidKey);
    }

    // We assume that the source field has been validated previously
    let source = message
        .as_proto()
        .from
        .as_deref()
        .ok_or(MessageValidationError::MissingMessageSource)?;
    PublicKey::try_decode_protobuf(&source[2..]).map_err(|_| MessageValidationError::InvalidPeerId)
}

/// Verify the signature of a message.
///
/// The signature is calculated over the bytes "libp2p-pubsub:<protobuf-message>".
pub fn verify_message_signature(
    message: &MessageProto,
    signature: &[u8],
    public_key: &PublicKey,
) -> bool {
    let mut msg = message.clone();
    msg.signature = None;
    msg.key = None;

    // Construct the signature bytes
    let mut sign_bytes = Vec::with_capacity(SIGNING_PREFIX.len() + msg.encoded_len());
    sign_bytes.extend(SIGNING_PREFIX.to_vec());
    sign_bytes.extend(msg.encode_to_vec());

    public_key.verify(&sign_bytes, signature)
}

/// Do not validate the message
#[derive(Default)]
pub struct NoopMessageValidator;

impl NoopMessageValidator {
    pub fn new() -> Self {
        Default::default()
    }
}

impl MessageValidator for NoopMessageValidator {
    fn validate(&self, _message: &MessageRpc) -> Result<(), MessageValidationError> {
        Ok(())
    }
}

/// Verify the message signature, the sequence number and the source are not present
#[derive(Default)]
pub struct AnonymousMessageValidator;

impl AnonymousMessageValidator {
    pub fn new() -> Self {
        Default::default()
    }
}

impl MessageValidator for AnonymousMessageValidator {
    fn validate(&self, message: &MessageRpc) -> Result<(), MessageValidationError> {
        if message.signature().is_some() {
            warn!("Signature field was non-empty and anonymous validation mode is set");
            return Err(MessageValidationError::SignaturePresent);
        } else if message.sequence_number().is_some() {
            warn!("Sequence number was non-empty and anonymous validation mode is set");
            return Err(MessageValidationError::SequenceNumberPresent);
        } else if message.source().is_some() {
            warn!("Message source was non-empty and anonymous validation mode is set");
            return Err(MessageValidationError::MessageSourcePresent);
        } else if message.key().is_some() {
            warn!("Message key was non-empty and anonymous validation mode is set");
            return Err(MessageValidationError::KeyPresent);
        }

        Ok(())
    }
}

/// If the fields are present, validate them
#[derive(Default)]
pub struct PermissiveMessageValidator;

impl PermissiveMessageValidator {
    pub fn new() -> Self {
        Default::default()
    }
}

impl MessageValidator for PermissiveMessageValidator {
    fn validate(&self, message: &MessageRpc) -> Result<(), MessageValidationError> {
        // verify message signature, if present
        if let Some(signature) = message.signature() {
            let public_key = match extract_key(message) {
                Ok(value) => value,
                Err(_) => return Err(MessageValidationError::MissingPublicKey),
            };

            // The key must match the peer_id
            if let Some(src) = message.source() {
                if src != public_key.to_peer_id() {
                    warn!("Signature verification failed: Public key doesn't match source peer id");
                    return Err(MessageValidationError::InvalidKey);
                }
            }

            if !verify_message_signature(message.as_proto(), signature, &public_key) {
                warn!("Invalid signature for received message");
                return Err(MessageValidationError::InvalidSignature);
            }
        }

        Ok(())
    }
}

/// Validate all fields
#[derive(Default)]
pub struct StrictMessageValidator;

impl StrictMessageValidator {
    pub fn new() -> Self {
        Default::default()
    }
}

impl MessageValidator for StrictMessageValidator {
    fn validate(&self, message: &MessageRpc) -> Result<(), MessageValidationError> {
        // message sequence number must be present
        if message.sequence_number().is_none() {
            debug!("Sequence number not present, but expected");
            return Err(MessageValidationError::MissingSequenceNumber);
        }

        // message source must be present
        if message.source().is_none() {
            debug!("Message source not present, but expected");
            return Err(MessageValidationError::InvalidPeerId);
        }

        // message signature must be present
        if message.signature().is_none() {
            debug!("Message signature not present, but expected");
            return Err(MessageValidationError::MissingSignature);
        }

        let public_key = match extract_key(message) {
            Ok(value) => value,
            Err(_) => return Err(MessageValidationError::MissingPublicKey),
        };

        // The key must match the peer_id
        if message.source().unwrap() != public_key.to_peer_id() {
            warn!("Signature verification failed: Public key doesn't match source peer id");
            return Err(MessageValidationError::InvalidKey);
        }

        // verify message signatures
        let signature = message.signature().unwrap();
        if !verify_message_signature(message.as_proto(), signature, &public_key) {
            warn!("Invalid signature for received message");
            return Err(MessageValidationError::InvalidSignature);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use bytes::Bytes;
    use hex_literal::hex;
    use libp2p::identity::{Keypair, PeerId};

    use crate::gossipsub::signing::signer::generate_message_signature;

    use super::*;

    fn test_keypair() -> Keypair {
        Keypair::generate_secp256k1()
    }

    fn new_test_message(
        from: Option<PeerId>,
        seq_no: Option<u64>,
        signature: Option<Bytes>,
        key: Option<Bytes>,
    ) -> MessageRpc {
        let mut rpc = MessageRpc::new("test-topic", b"test-data".to_vec());
        rpc.set_source(from);
        rpc.set_sequence_number(seq_no);
        rpc.set_signature(signature);
        rpc.set_key(key);
        rpc
    }

    fn new_test_signed_message(keypair: &Keypair, seq_no: Option<u64>) -> MessageRpc {
        let from = Some(keypair.public().to_peer_id());
        let mut message = new_test_message(from, seq_no, None, None);

        let sign = generate_message_signature(message.as_proto(), keypair)
            .expect("message signing failed");
        let key = keypair.public().encode_protobuf();

        message.set_signature(Some(sign));
        message.set_key(Some(key));

        message
    }

    mod noop_validator {
        use super::*;

        #[test]
        fn test_valid_message() {
            // Given
            let message = MessageRpc::new("test-topic", b"test-data".to_vec());
            let validator = NoopMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert!(result.is_ok());
        }
    }

    mod anonymous_validator {
        use super::*;

        #[test]
        fn test_valid_message() {
            // Given
            let message = new_test_message(None, None, None, None);
            let validator = AnonymousMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert!(result.is_ok());
        }

        #[test]
        fn test_error_signature_present() {
            // Given
            let keypair = test_keypair();
            let message = new_test_signed_message(&keypair, Some(1234));
            let validator = AnonymousMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert_matches!(result, Err(MessageValidationError::SignaturePresent));
        }

        #[test]
        fn test_error_seqno_present() {
            // Given
            let message = new_test_message(Some(PeerId::random()), Some(1234), None, None);
            let validator = AnonymousMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert_matches!(result, Err(MessageValidationError::SequenceNumberPresent));
        }

        #[test]
        fn test_error_from_present() {
            // Given
            let message = new_test_message(Some(PeerId::random()), None, None, None);
            let validator = AnonymousMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert_matches!(result, Err(MessageValidationError::MessageSourcePresent));
        }
    }

    mod permissive_validator {
        use super::*;

        #[test]
        fn test_valid_fields_not_present() {
            // Given
            let message = new_test_message(None, None, None, None);
            let validator = PermissiveMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert!(result.is_ok());
        }

        #[test]
        fn test_valid_fields_present() {
            // Given
            let keypair = test_keypair();
            let message = new_test_signed_message(&keypair, Some(1234));
            let validator = PermissiveMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert!(result.is_ok());
        }

        #[test]
        fn test_valid_key_not_present() {
            // Given
            let keypair = Keypair::generate_ed25519();
            let message = {
                let mut msg = new_test_signed_message(&keypair, Some(1234));
                msg.set_key(Option::<Vec<u8>>::None);
                msg
            };
            let validator = PermissiveMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert!(result.is_ok());
        }

        #[test]
        fn test_error_invalid_signature() {
            // Given
            let keypair = test_keypair();
            let message = {
                let mut msg = new_test_signed_message(&keypair, Some(1234));
                msg.set_signature(Some(hex!("cafebabe").to_vec()));
                msg
            };
            let validator = PermissiveMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert_matches!(result, Err(MessageValidationError::InvalidSignature));
        }
    }

    mod strict_validator {
        use super::*;

        #[test]
        fn test_error_seqno_not_present() {
            // Given
            let keypair = test_keypair();
            let message = new_test_signed_message(&keypair, None);
            let validator = StrictMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert_matches!(result, Err(MessageValidationError::MissingSequenceNumber));
        }

        #[test]
        fn test_error_from_not_present() {
            // Given
            let keypair = test_keypair();
            let message = {
                let mut msg = new_test_message(None, Some(1234), None, None);

                let sign = generate_message_signature(msg.as_proto(), &keypair)
                    .expect("message signing failed");
                let key = keypair.public().encode_protobuf();

                msg.set_signature(Some(sign));
                msg.set_key(Some(key));

                msg
            };
            let validator = StrictMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert_matches!(result, Err(MessageValidationError::InvalidPeerId));
        }

        #[test]
        fn test_error_invalid_signature() {
            // Given
            let keypair = test_keypair();
            let message = {
                let mut msg = new_test_signed_message(&keypair, Some(1234));
                msg.set_signature(Some(hex!("cafebabe").to_vec()));
                msg
            };
            let validator = StrictMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert_matches!(result, Err(MessageValidationError::InvalidSignature));
        }

        #[test]
        fn test_valid_no_key_present() {
            // Given
            let keypair = test_keypair();
            let msg = {
                let mut message = new_test_signed_message(&keypair, Some(1234));
                message.set_key(Option::<Vec<u8>>::None);
                message
            };
            let validator = StrictMessageValidator::new();

            // When
            let result = validator.validate(&msg);

            // Then
            assert!(result.is_ok());
        }

        #[test]
        fn test_error_from_and_key_do_not_match() {
            // Given
            let keypair = test_keypair();
            let message = {
                let mut msg = new_test_message(Some(PeerId::random()), Some(1234), None, None);

                let sign = generate_message_signature(msg.as_proto(), &keypair)
                    .expect("message signing failed");
                let key = keypair.public().encode_protobuf();

                msg.set_signature(Some(sign));
                msg.set_key(Some(key));

                msg
            };
            let validator = PermissiveMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert_matches!(result, Err(MessageValidationError::InvalidKey));
        }

        #[test]
        fn test_error_valid_from_and_key_present() {
            // Given
            let keypair = test_keypair();
            let message = new_test_signed_message(&keypair, Some(1234));
            let validator = StrictMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert!(result.is_ok());
        }
    }
}
