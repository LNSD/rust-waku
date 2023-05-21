use bytes::Bytes;
use libp2p::identity::{Keypair, PublicKey};
use libp2p::PeerId;
use log::{debug, warn};
use prost::Message as _;
use void::unreachable;

use crate::gossipsub::rpc::proto::waku::relay::v2::Message as MessageProto;

pub(crate) const SIGNING_PREFIX: &[u8] = b"libp2p-pubsub:";

#[derive(Debug, Clone, Copy)]
pub enum ValidationError {
    /// The message has an invalid signature,
    InvalidSignature,
    /// The sequence number was empty, expected a value.
    EmptySequenceNumber,
    /// The sequence number was the incorrect size
    InvalidSequenceNumber,
    /// The PeerId was invalid
    InvalidPeerId,
    /// Signature existed when validation has been sent to
    /// [`crate::behaviour::MessageAuthenticity::Anonymous`].
    SignaturePresent,
    /// Sequence number existed when validation has been sent to
    /// [`crate::behaviour::MessageAuthenticity::Anonymous`].
    SequenceNumberPresent,
    /// Message source existed when validation has been sent to
    /// [`crate::behaviour::MessageAuthenticity::Anonymous`].
    MessageSourcePresent,
    /// The data transformation failed.
    TransformFailed,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ValidationError {}

pub trait MessageValidator: Send + Sync {
    fn validate(&self, message: &MessageProto) -> Result<(), ValidationError>;
}

/// Verifies a gossipsub message. This returns either a success or failure. All errors
/// are logged, which prevents error handling in the codec and handler. We simply drop invalid
/// messages and log warnings, rather than propagating errors through the codec.
// TODO: Return a proper error enum instead of logging the reason (use thiserror)
fn verify_signature(message: &MessageProto) -> bool {
    let signature = match message.signature.as_ref() {
        Some(v) => v,
        None => {
            debug!("Signature verification failed: No signature provided");
            return false;
        }
    };
    let from = match message.from.as_ref() {
        Some(v) => v,
        None => {
            debug!("Signature verification failed: No source id given");
            return false;
        }
    };

    let source = match PeerId::from_bytes(from) {
        Ok(v) => v,
        Err(_) => {
            debug!("Signature verification failed: Invalid Peer Id");
            return false;
        }
    };

    // If there is a key value in the protobuf, use that key otherwise the key must be
    // obtained from the inlined source peer_id.
    let public_key = match message.key.as_deref().map(PublicKey::try_decode_protobuf) {
        Some(Ok(key)) => key,
        _ => match PublicKey::try_decode_protobuf(&source.to_bytes()[2..]) {
            Ok(v) => v,
            Err(_) => {
                warn!("Signature verification failed: No valid public key supplied");
                return false;
            }
        },
    };

    // The key must match the peer_id
    if source != public_key.to_peer_id() {
        warn!("Signature verification failed: Public key doesn't match source peer id");
        return false;
    }

    verify_message_signature(message, signature, public_key)
}

/// Generate the signature for a message.
///
/// The signature is calculated over the bytes "libp2p-pubsub:<protobuf-message>".
pub fn generate_message_signature(
    message: &MessageProto,
    keypair: &Keypair,
) -> anyhow::Result<Vec<u8>> {
    let mut msg = message.clone();
    msg.signature = None;
    msg.key = None;

    // Construct the signature bytes
    let mut sign_bytes = Vec::with_capacity(SIGNING_PREFIX.len() + msg.encoded_len());
    sign_bytes.extend(SIGNING_PREFIX.to_vec());
    sign_bytes.extend(msg.encode_to_vec());

    Ok(keypair.sign(&sign_bytes)?)
}

/// Verify the signature of a message.
///
/// The signature is calculated over the bytes "libp2p-pubsub:<protobuf-message>".
pub fn verify_message_signature(
    message: &MessageProto,
    signature: &Bytes,
    public_key: PublicKey,
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
    fn validate(&self, _: &MessageProto) -> Result<(), ValidationError> {
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
    fn validate(&self, message: &MessageProto) -> Result<(), ValidationError> {
        if message.signature.is_some() {
            warn!("Signature field was non-empty and anonymous validation mode is set");
            return Err(ValidationError::SignaturePresent);
        } else if message.seqno.is_some() {
            warn!("Sequence number was non-empty and anonymous validation mode is set");
            return Err(ValidationError::SequenceNumberPresent);
        } else if message.from.is_some() {
            warn!("Message source was non-empty and anonymous validation mode is set");
            return Err(ValidationError::MessageSourcePresent);
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
    fn validate(&self, message: &MessageProto) -> Result<(), ValidationError> {
        // ensure the sequence number is a u64
        if let Some(seq_no) = &message.seqno {
            if seq_no.is_empty() {
                // sequence number was present but empty
                debug!("Sequence number present but empty");
                return Err(ValidationError::EmptySequenceNumber);
            }

            if seq_no.len() != 8 {
                debug!(
                    "Invalid sequence number length for received message. SeqNo: {:?} Size: {}",
                    seq_no,
                    seq_no.len()
                );
                return Err(ValidationError::InvalidSequenceNumber);
            }
        }

        // Verify the message source
        if let Some(peer_bytes) = &message.from {
            if !peer_bytes.is_empty() && PeerId::from_bytes(peer_bytes).is_err() {
                // invalid peer id, add to invalid messages
                debug!("Message source has an invalid PeerId");
                return Err(ValidationError::InvalidPeerId);
            }
        }

        // verify message signatures
        if message.signature.is_some() && !verify_signature(message) {
            warn!("Invalid signature for received message");
            return Err(ValidationError::InvalidSignature);
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
    fn validate(&self, message: &MessageProto) -> Result<(), ValidationError> {
        // ensure the sequence number is a u64
        match &message.seqno {
            None => {
                debug!("Sequence number not present, but expected");
                return Err(ValidationError::EmptySequenceNumber);
            }
            Some(seq_no) if seq_no.is_empty() => {
                debug!("Sequence number present, but empty");
                return Err(ValidationError::EmptySequenceNumber);
            }
            Some(seq_no) if seq_no.len() != 8 => {
                debug!(
                    "Invalid sequence number length for received message. SeqNo: {:?} Size: {}",
                    seq_no,
                    seq_no.len()
                );
                return Err(ValidationError::InvalidSequenceNumber);
            }
            _ => {} // Valid sequence number length
        };

        // Verify the message source
        match &message.from {
            Some(bytes) if bytes.is_empty() => {
                debug!("Message source present, but empty");
                return Err(ValidationError::InvalidPeerId);
            }
            Some(bytes) if !bytes.is_empty() => {
                if PeerId::from_bytes(bytes).is_err() {
                    // invalid peer id, add to invalid messages
                    debug!("Message source has an invalid PeerId");
                    return Err(ValidationError::InvalidPeerId);
                }
            }
            None => {
                debug!("Message source not present, but expected");
                return Err(ValidationError::InvalidPeerId);
            }
            _ => unreachable!(),
        }

        // verify message signatures
        if !verify_signature(message) {
            warn!("Invalid signature for received message");
            return Err(ValidationError::InvalidSignature);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use hex_literal::hex;

    use super::*;

    fn new_test_message(
        from: Option<PeerId>,
        seqno: Option<u64>,
        signature: Option<Bytes>,
        key: Option<Bytes>,
    ) -> MessageProto {
        MessageProto {
            data: Some(Bytes::from_static(b"test")),
            topic: "test".to_string(),
            from: from.map(|id| Bytes::from(id.to_bytes())),
            seqno: seqno.map(|seq| Bytes::copy_from_slice(&seq.to_be_bytes())),
            signature,
            key,
        }
    }

    fn new_test_signed_message(keypair: &Keypair, seqno: Option<u64>) -> MessageProto {
        let from = Some(keypair.public().to_peer_id());
        let mut message = new_test_message(from, seqno, None, None);

        let sign = generate_message_signature(&message, keypair).expect("message signing failed");
        let key = keypair.public().encode_protobuf();

        message.signature = Some(Bytes::from(sign));
        message.key = Some(Bytes::from(key));

        message
    }

    mod noop_validator {
        use super::*;

        #[test]
        fn test_valid_message() {
            // Given
            let message = MessageProto::default();
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
            let keypair = Keypair::generate_ed25519();
            let message = new_test_signed_message(&keypair, Some(1234));
            let validator = AnonymousMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert_matches!(result, Err(ValidationError::SignaturePresent));
        }

        #[test]
        fn test_error_seqno_present() {
            // Given
            let message = new_test_message(Some(PeerId::random()), Some(1234), None, None);
            let validator = AnonymousMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert_matches!(result, Err(ValidationError::SequenceNumberPresent));
        }

        #[test]
        fn test_error_from_present() {
            // Given
            let message = new_test_message(Some(PeerId::random()), None, None, None);
            let validator = AnonymousMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert_matches!(result, Err(ValidationError::MessageSourcePresent));
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
            let keypair = Keypair::generate_ed25519();
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
                msg.key = None;
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
            let keypair = Keypair::generate_ed25519();
            let message = {
                let mut msg = new_test_signed_message(&keypair, Some(1234));
                msg.signature = Some(Bytes::copy_from_slice(&hex!("cafebabe")));
                msg
            };
            let validator = PermissiveMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert_matches!(result, Err(ValidationError::InvalidSignature));
        }
    }

    mod strict_validator {
        use super::*;

        #[test]
        fn test_error_seqno_not_present() {
            // Given
            let keypair = Keypair::generate_ed25519();
            let message = new_test_signed_message(&keypair, None);
            let validator = StrictMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert_matches!(result, Err(ValidationError::EmptySequenceNumber));
        }

        #[test]
        fn test_error_from_not_present() {
            // Given
            let keypair = Keypair::generate_ed25519();
            let message = {
                let mut msg = new_test_message(None, Some(1234), None, None);

                let sign =
                    generate_message_signature(&msg, &keypair).expect("message signing failed");
                let key = keypair.public().encode_protobuf();

                msg.signature = Some(Bytes::from(sign));
                msg.key = Some(Bytes::from(key));

                msg
            };
            let validator = StrictMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert_matches!(result, Err(ValidationError::InvalidPeerId));
        }

        #[test]
        fn test_error_invalid_signature() {
            // Given
            let keypair = Keypair::generate_ed25519();
            let message = {
                let mut msg = new_test_signed_message(&keypair, Some(1234));
                msg.signature = Some(Bytes::copy_from_slice(&hex!("cafebabe")));
                msg
            };
            let validator = StrictMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert_matches!(result, Err(ValidationError::InvalidSignature));
        }

        #[test]
        fn test_valid_no_key_present() {
            // Given
            let keypair = Keypair::generate_ed25519();
            let msg = {
                let mut message = new_test_signed_message(&keypair, Some(1234));
                message.key = None;
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
            let keypair = Keypair::generate_ed25519();
            let message = {
                let mut msg = new_test_message(Some(PeerId::random()), Some(1234), None, None);

                let sign =
                    generate_message_signature(&msg, &keypair).expect("message signing failed");
                let key = keypair.public().encode_protobuf();

                msg.signature = Some(Bytes::from(sign));
                msg.key = Some(Bytes::from(key));

                msg
            };
            let validator = PermissiveMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert_matches!(result, Err(ValidationError::InvalidSignature));
        }

        #[test]
        fn test_error_valid_from_and_key_present() {
            // Given
            let keypair = Keypair::generate_ed25519();
            let message = new_test_signed_message(&keypair, Some(1234));
            let validator = StrictMessageValidator::new();

            // When
            let result = validator.validate(&message);

            // Then
            assert!(result.is_ok());
        }
    }
}
