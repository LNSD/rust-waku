use std::io;

use asynchronous_codec::{Decoder, Encoder};
use bytes::BytesMut;

use waku_core::common::protobuf_codec;

use crate::gossipsub::handler::HandlerEvent;
use crate::gossipsub::rpc::proto::waku::relay::v2::Rpc as RpcProto;
use crate::gossipsub::types::ControlAction;
use crate::gossipsub::validation::{
    AnonymousMessageValidator, MessageValidator, NoopMessageValidator, PermissiveMessageValidator,
    StrictMessageValidator,
};
use crate::gossipsub::{RawMessage, Rpc, TopicHash, ValidationMode};

pub struct Codec {
    /// The codec to handle common encoding/decoding of protobuf messages
    codec: protobuf_codec::Codec<RpcProto>,
    /// The validator to use for validating messages
    message_validator: Box<dyn MessageValidator>,
}

pub(crate) const SIGNING_PREFIX: &[u8] = b"libp2p-pubsub:";

impl Codec {
    pub fn new(max_len_bytes: usize, validation_mode: ValidationMode) -> Self {
        let codec = protobuf_codec::Codec::new(max_len_bytes);
        let message_validator: Box<dyn MessageValidator> = match validation_mode {
            ValidationMode::Strict => Box::new(StrictMessageValidator::new()),
            ValidationMode::Permissive => Box::new(PermissiveMessageValidator::new()),
            ValidationMode::Anonymous => Box::new(AnonymousMessageValidator::new()),
            ValidationMode::None => Box::new(NoopMessageValidator::new()),
        };
        Self {
            codec,
            message_validator,
        }
    }
}

impl Encoder for Codec {
    type Item = RpcProto;
    type Error = io::Error;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        self.codec.encode(item, dst)
    }
}

impl Decoder for Codec {
    type Item = HandlerEvent;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let rpc = match self.codec.decode(src)? {
            Some(p) => p,
            None => return Ok(None),
        };

        let mut valid_messages = Vec::with_capacity(rpc.publish.len());
        let mut invalid_messages = Vec::new();

        for message in rpc.publish.into_iter() {
            if let Err(err) = self.message_validator.validate(&message) {
                // If the message is invalid, add it to the invalid messages and continue
                // processing the other messages.
                let raw_message = RawMessage {
                    source: None, // don't inform the application
                    data: message.data.map(Into::into).unwrap_or_default(),
                    sequence_number: None, // don't inform the application
                    topic: TopicHash::from_raw(&message.topic),
                    signature: None, // don't inform the application
                    key: message.key.map(Into::into),
                    validated: false,
                };
                invalid_messages.push((raw_message, err));

                continue;
            }

            // This message has passed all validation, add it to the validated messages.
            valid_messages.push(message.into());
        }

        let control_msgs: Vec<ControlAction> = rpc
            .control
            .map(|rpc_control| {
                // Collect the gossipsub control messages
                let ihave_msgs_iter = rpc_control.ihave.into_iter().map(Into::into);
                let iwant_msgs_iter = rpc_control.iwant.into_iter().map(Into::into);
                let graft_msgs_iter = rpc_control.graft.into_iter().map(Into::into);
                let prune_msgs_iter = rpc_control.prune.into_iter().map(Into::into);

                ihave_msgs_iter
                    .chain(iwant_msgs_iter)
                    .chain(graft_msgs_iter)
                    .chain(prune_msgs_iter)
                    .collect()
            })
            .unwrap_or_default();

        let subscriptions = rpc.subscriptions.into_iter().map(Into::into).collect();

        Ok(Some(HandlerEvent::Message {
            rpc: Rpc {
                messages: valid_messages,
                subscriptions,
                control_msgs,
            },
            invalid_messages,
        }))
    }
}
