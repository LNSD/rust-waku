// Copyright 2020 Sigma Prime Pty Ltd.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use std::io;
use std::pin::Pin;

use asynchronous_codec::{Decoder, Encoder, Framed};
use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;
use futures::future;
use futures::prelude::*;
use libp2p::core::{ProtocolName, UpgradeInfo};
use libp2p::identity::PublicKey;
use libp2p::PeerId;
use libp2p::{InboundUpgrade, OutboundUpgrade};
use log::{debug, warn};
use prost::Message;
use unsigned_varint::codec;
use void::Void;

use waku_core::common::{protobuf_codec, quick_protobuf_codec};

use crate::gossipsub::config::{ValidationMode, Version};
use crate::gossipsub::handler::HandlerEvent;
use crate::gossipsub::rpc::proto::waku::relay::v2::{Message as MessageProto, Rpc as RpcProto};
use crate::gossipsub::topic::TopicHash;
use crate::gossipsub::types::{
    ControlAction, PeerInfo, PeerKind, RawMessage, Rpc, Subscription, SubscriptionAction,
};
use crate::gossipsub::Config;
use crate::gossipsub::ValidationError;

pub(crate) const SIGNING_PREFIX: &[u8] = b"libp2p-pubsub:";

/// Implementation of [`InboundUpgrade`] and [`OutboundUpgrade`] for the Gossipsub protocol.
#[derive(Debug, Clone)]
pub struct ProtocolConfig {
    /// The Gossipsub protocol id to listen on.
    protocol_ids: Vec<ProtocolId>,
    /// The maximum transmit size for a packet.
    max_transmit_size: usize,
    /// Determines the level of validation to be done on incoming messages.
    validation_mode: ValidationMode,
}

impl ProtocolConfig {
    /// Builds a new [`ProtocolConfig`].
    ///
    /// Sets the maximum gossip transmission size.
    pub fn new(gossipsub_config: &Config) -> ProtocolConfig {
        let mut protocol_ids = match gossipsub_config.custom_id_version() {
            Some(v) => match v {
                Version::V1_0 => vec![ProtocolId::new(
                    gossipsub_config.protocol_id(),
                    PeerKind::Gossipsub,
                    false,
                )],
                Version::V1_1 => vec![ProtocolId::new(
                    gossipsub_config.protocol_id(),
                    PeerKind::Gossipsubv1_1,
                    false,
                )],
            },
            None => {
                vec![
                    ProtocolId::new(
                        gossipsub_config.protocol_id(),
                        PeerKind::Gossipsubv1_1,
                        true,
                    ),
                    ProtocolId::new(gossipsub_config.protocol_id(), PeerKind::Gossipsub, true),
                ]
            }
        };

        // add floodsub support if enabled.
        if gossipsub_config.support_floodsub() {
            protocol_ids.push(ProtocolId::new("", PeerKind::Floodsub, false));
        }

        ProtocolConfig {
            protocol_ids,
            max_transmit_size: gossipsub_config.max_transmit_size(),
            validation_mode: gossipsub_config.validation_mode().clone(),
        }
    }
}

/// The protocol ID
#[derive(Clone, Debug)]
pub struct ProtocolId {
    /// The RPC message type/name.
    pub protocol_id: Vec<u8>,
    /// The type of protocol we support
    pub kind: PeerKind,
}

/// An RPC protocol ID.
impl ProtocolId {
    pub fn new(id: &str, kind: PeerKind, prefix: bool) -> Self {
        let protocol_id = match kind {
            PeerKind::Gossipsubv1_1 => match prefix {
                true => format!("/{}/{}", id, "1.1.0"),
                false => id.to_string(),
            },
            PeerKind::Gossipsub => match prefix {
                true => format!("/{}/{}", id, "1.0.0"),
                false => id.to_string(),
            },
            PeerKind::Floodsub => format!("/{}/{}", "floodsub", "1.0.0"),
            // NOTE: This is used for informing the behaviour of unsupported peers. We do not
            // advertise this variant.
            PeerKind::NotSupported => unreachable!("Should never advertise NotSupported"),
        }
        .into_bytes();
        ProtocolId { protocol_id, kind }
    }
}

impl ProtocolName for ProtocolId {
    fn protocol_name(&self) -> &[u8] {
        &self.protocol_id
    }
}

impl UpgradeInfo for ProtocolConfig {
    type Info = ProtocolId;
    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        self.protocol_ids.clone()
    }
}

impl<TSocket> InboundUpgrade<TSocket> for ProtocolConfig
where
    TSocket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = (Framed<TSocket, GossipsubCodec>, PeerKind);
    type Error = Void;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, socket: TSocket, protocol_id: Self::Info) -> Self::Future {
        let mut length_codec = codec::UviBytes::default();
        length_codec.set_max_len(self.max_transmit_size);
        Box::pin(future::ok((
            Framed::new(
                socket,
                GossipsubCodec::new(length_codec, self.validation_mode),
            ),
            protocol_id.kind,
        )))
    }
}

impl<TSocket> OutboundUpgrade<TSocket> for ProtocolConfig
where
    TSocket: AsyncWrite + AsyncRead + Unpin + Send + 'static,
{
    type Output = (Framed<TSocket, GossipsubCodec>, PeerKind);
    type Error = Void;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, socket: TSocket, protocol_id: Self::Info) -> Self::Future {
        let mut length_codec = codec::UviBytes::default();
        length_codec.set_max_len(self.max_transmit_size);
        Box::pin(future::ok((
            Framed::new(
                socket,
                GossipsubCodec::new(length_codec, self.validation_mode),
            ),
            protocol_id.kind,
        )))
    }
}

/* Gossip codec for the framing */

pub struct GossipsubCodec {
    /// Determines the level of validation performed on incoming messages.
    validation_mode: ValidationMode,
    /// The codec to handle common encoding/decoding of protobuf messages
    codec: protobuf_codec::Codec<RpcProto>,
}

impl GossipsubCodec {
    pub fn new(length_codec: codec::UviBytes, validation_mode: ValidationMode) -> GossipsubCodec {
        let codec = protobuf_codec::Codec::new(length_codec.max_len());
        GossipsubCodec {
            validation_mode,
            codec,
        }
    }

    /// Verifies a gossipsub message. This returns either a success or failure. All errors
    /// are logged, which prevents error handling in the codec and handler. We simply drop invalid
    /// messages and log warnings, rather than propagating errors through the codec.
    fn verify_signature(message: &MessageProto) -> bool {
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

        let signature = match message.signature.as_ref() {
            Some(v) => v,
            None => {
                debug!("Signature verification failed: No signature provided");
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

        // Construct the signature bytes
        let mut message_sig = message.clone();
        message_sig.signature = None;
        message_sig.key = None;
        let mut buf = Vec::with_capacity(message_sig.encoded_len());
        message_sig.encode(&mut buf).expect("Encoding to succeed");
        let mut signature_bytes = SIGNING_PREFIX.to_vec();
        signature_bytes.extend_from_slice(&buf);
        public_key.verify(&signature_bytes, signature)
    }
}

impl Encoder for GossipsubCodec {
    type Item = RpcProto;
    type Error = io::Error;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        self.codec.encode(item, dst)
    }
}

impl Decoder for GossipsubCodec {
    type Item = HandlerEvent;
    type Error = quick_protobuf_codec::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let rpc = match self.codec.decode(src)? {
            Some(p) => p,
            None => return Ok(None),
        };

        // Store valid messages.
        let mut messages = Vec::with_capacity(rpc.publish.len());
        // Store any invalid messages.
        let mut invalid_messages = Vec::new();

        for message in rpc.publish.into_iter() {
            // Keep track of the type of invalid message.
            let mut invalid_kind = None;
            let mut verify_signature = false;
            let mut verify_sequence_no = false;
            let mut verify_source = false;

            match self.validation_mode {
                ValidationMode::Strict => {
                    // Validate everything
                    verify_signature = true;
                    verify_sequence_no = true;
                    verify_source = true;
                }
                ValidationMode::Permissive => {
                    // If the fields exist, validate them
                    if message.signature.is_some() {
                        verify_signature = true;
                    }
                    if message.seqno.is_some() {
                        verify_sequence_no = true;
                    }
                    if message.from.is_some() {
                        verify_source = true;
                    }
                }
                ValidationMode::Anonymous => {
                    if message.signature.is_some() {
                        warn!("Signature field was non-empty and anonymous validation mode is set");
                        invalid_kind = Some(ValidationError::SignaturePresent);
                    } else if message.seqno.is_some() {
                        warn!("Sequence number was non-empty and anonymous validation mode is set");
                        invalid_kind = Some(ValidationError::SequenceNumberPresent);
                    } else if message.from.is_some() {
                        warn!("Message dropped. Message source was non-empty and anonymous validation mode is set");
                        invalid_kind = Some(ValidationError::MessageSourcePresent);
                    }
                }
                ValidationMode::None => {}
            }

            // If the initial validation logic failed, add the message to invalid messages and
            // continue processing the others.
            if let Some(validation_error) = invalid_kind.take() {
                let message = RawMessage {
                    source: None, // don't bother inform the application
                    data: message.data.map(Into::into).unwrap_or_default(),
                    sequence_number: None, // don't inform the application
                    topic: TopicHash::from_raw(message.topic),
                    signature: None, // don't inform the application
                    key: message.key.map(Into::into),
                    validated: false,
                };
                invalid_messages.push((message, validation_error));
                // proceed to the next message
                continue;
            }

            // verify message signatures if required
            if verify_signature && !GossipsubCodec::verify_signature(&message) {
                warn!("Invalid signature for received message");

                // Build the invalid message (ignoring further validation of sequence number
                // and source)
                let message = RawMessage {
                    source: None, // don't bother inform the application
                    data: message.data.map(Into::into).unwrap_or_default(),
                    sequence_number: None, // don't inform the application
                    topic: TopicHash::from_raw(message.topic),
                    signature: None, // don't inform the application
                    key: message.key.map(Into::into),
                    validated: false,
                };
                invalid_messages.push((message, ValidationError::InvalidSignature));
                // proceed to the next message
                continue;
            }

            // ensure the sequence number is a u64
            let sequence_number = if verify_sequence_no {
                if let Some(seq_no) = message.seqno {
                    if seq_no.is_empty() {
                        None
                    } else if seq_no.len() != 8 {
                        debug!(
                            "Invalid sequence number length for received message. SeqNo: {:?} Size: {}",
                            seq_no,
                            seq_no.len()
                        );
                        let message = RawMessage {
                            source: None, // don't bother inform the application
                            data: message.data.map(Into::into).unwrap_or_default(),
                            sequence_number: None, // don't inform the application
                            topic: TopicHash::from_raw(message.topic),
                            signature: message.signature.map(Into::into), // don't inform the application
                            key: message.key.map(Into::into),
                            validated: false,
                        };
                        invalid_messages.push((message, ValidationError::InvalidSequenceNumber));
                        // proceed to the next message
                        continue;
                    } else {
                        // valid sequence number
                        Some(BigEndian::read_u64(&seq_no))
                    }
                } else {
                    // sequence number was not present
                    debug!("Sequence number not present but expected");
                    let message = RawMessage {
                        source: None, // don't bother inform the application
                        data: message.data.map(Into::into).unwrap_or_default(),
                        sequence_number: None, // don't inform the application
                        topic: TopicHash::from_raw(message.topic),
                        signature: message.signature.map(Into::into), // don't inform the application
                        key: message.key.map(Into::into),
                        validated: false,
                    };
                    invalid_messages.push((message, ValidationError::EmptySequenceNumber));
                    continue;
                }
            } else {
                // Do not verify the sequence number, consider it empty
                None
            };

            // Verify the message source if required
            let source = if verify_source {
                if let Some(bytes) = message.from {
                    if !bytes.is_empty() {
                        match PeerId::from_bytes(&bytes) {
                            Ok(peer_id) => Some(peer_id), // valid peer id
                            Err(_) => {
                                // invalid peer id, add to invalid messages
                                debug!("Message source has an invalid PeerId");
                                let message = RawMessage {
                                    source: None, // don't bother inform the application
                                    data: message.data.map(Into::into).unwrap_or_default(),
                                    sequence_number,
                                    topic: TopicHash::from_raw(message.topic),
                                    signature: message.signature.map(Into::into), // don't inform the application
                                    key: message.key.map(Into::into),
                                    validated: false,
                                };
                                invalid_messages.push((message, ValidationError::InvalidPeerId));
                                continue;
                            }
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // This message has passed all validation, add it to the validated messages.
            messages.push(RawMessage {
                source,
                data: message.data.map(Into::into).unwrap_or_default(),
                sequence_number,
                topic: TopicHash::from_raw(message.topic),
                signature: message.signature.map(Into::into),
                key: message.key.map(Into::into),
                validated: false,
            });
        }

        let mut control_msgs = Vec::new();

        if let Some(rpc_control) = rpc.control {
            // Collect the gossipsub control messages
            let ihave_msgs: Vec<ControlAction> = rpc_control
                .ihave
                .into_iter()
                .map(|ihave| ControlAction::IHave {
                    topic_hash: TopicHash::from_raw(ihave.topic_id.unwrap_or_default()),
                    message_ids: ihave.message_ids.into_iter().map(Into::into).collect(),
                })
                .collect();

            let iwant_msgs: Vec<ControlAction> = rpc_control
                .iwant
                .into_iter()
                .map(|iwant| ControlAction::IWant {
                    message_ids: iwant
                        .message_ids
                        .into_iter()
                        .map(Into::into)
                        .collect::<Vec<_>>(),
                })
                .collect();

            let graft_msgs: Vec<ControlAction> = rpc_control
                .graft
                .into_iter()
                .map(|graft| ControlAction::Graft {
                    topic_hash: TopicHash::from_raw(graft.topic_id.unwrap_or_default()),
                })
                .collect();

            let mut prune_msgs = Vec::new();

            for prune in rpc_control.prune {
                // filter out invalid peers
                let peers = prune
                    .peers
                    .into_iter()
                    .filter_map(|info| {
                        info.peer_id
                            .as_ref()
                            .and_then(|id| PeerId::from_bytes(id).ok())
                            .map(|peer_id|
                                    //TODO signedPeerRecord, see https://github.com/libp2p/specs/pull/217
                                    PeerInfo {
                                        peer_id: Some(peer_id),
                                    })
                    })
                    .collect::<Vec<PeerInfo>>();

                let topic_hash = TopicHash::from_raw(prune.topic_id.unwrap_or_default());
                prune_msgs.push(ControlAction::Prune {
                    topic_hash,
                    peers,
                    backoff: prune.backoff,
                });
            }

            control_msgs.extend(ihave_msgs);
            control_msgs.extend(iwant_msgs);
            control_msgs.extend(graft_msgs);
            control_msgs.extend(prune_msgs);
        }

        Ok(Some(HandlerEvent::Message {
            rpc: Rpc {
                messages,
                subscriptions: rpc
                    .subscriptions
                    .into_iter()
                    .map(|sub| Subscription {
                        action: if Some(true) == sub.subscribe {
                            SubscriptionAction::Subscribe
                        } else {
                            SubscriptionAction::Unsubscribe
                        },
                        topic_hash: TopicHash::from_raw(sub.topic_id.unwrap_or_default()),
                    })
                    .collect(),
                control_msgs,
            },
            invalid_messages,
        }))
    }
}
