use libp2p::PeerId;

use crate::gossipsub::error::MessageValidationError;
use crate::gossipsub::rpc::MessageProto;

pub fn validate_message_proto(message: &MessageProto) -> Result<(), MessageValidationError> {
    if message.topic.is_empty() {
        // topic field must not be empty
        return Err(MessageValidationError::InvalidTopic);
    }

    // If present, from field must hold a valid PeerId
    if let Some(peer_id) = message.from.as_ref() {
        if PeerId::from_bytes(peer_id).is_err() {
            return Err(MessageValidationError::InvalidPeerId);
        }
    }

    // If present, seqno field must be a 64-bit big-endian serialized unsigned integer
    if let Some(seq_no) = message.seqno.as_ref() {
        if seq_no.len() != 8 {
            return Err(MessageValidationError::InvalidSequenceNumber);
        }
    }

    Ok(())
}
