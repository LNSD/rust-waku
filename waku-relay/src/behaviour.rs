use libp2p::identity::PeerId;
use libp2p::swarm::NetworkBehaviour;
use prost::Message;

use waku_core::message::proto::waku::message::v1::WakuMessage as WakuMessageProto;
use waku_core::message::WakuMessage;
use waku_core::pubsub_topic::PubsubTopic;

use crate::error::{PublishError, SubscriptionError};
use crate::event::Event;
use crate::gossipsub::{self, IdentTopic, MessageAuthenticity, MessageId, ValidationMode};
use crate::message_id::deterministic_message_id_fn;
use crate::proto::MAX_WAKU_RELAY_MESSAGE_SIZE;

pub const PROTOCOL_ID: &str = "/vac/waku/relay/2.0.0";

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "Event")]
pub struct Behaviour {
    pubsub: gossipsub::Behaviour,
}

impl Default for Behaviour {
    fn default() -> Self {
        let pubsub_config = gossipsub::ConfigBuilder::default()
            .protocol_id(PROTOCOL_ID, gossipsub::Version::V1_1)
            .validation_mode(ValidationMode::Anonymous) // StrictNoSign
            .message_id_fn(deterministic_message_id_fn)
            .max_transmit_size(MAX_WAKU_RELAY_MESSAGE_SIZE)
            .build()
            .expect("valid pubsub configuration");

        let pubsub = gossipsub::Behaviour::new(MessageAuthenticity::Anonymous, pubsub_config)
            .expect("valid pubsub configuration");

        Self { pubsub }
    }
}

impl Behaviour {
    pub fn subscribe(&mut self, topic: &PubsubTopic) -> Result<bool, SubscriptionError> {
        let ident_topic = IdentTopic::new(topic.to_string());
        self.pubsub.subscribe(&ident_topic).map_err(Into::into)
    }

    pub fn unsubscribe(&mut self, topic: &PubsubTopic) -> Result<bool, PublishError> {
        let ident_topic = IdentTopic::new(topic.to_string());
        self.pubsub.unsubscribe(&ident_topic).map_err(Into::into)
    }

    pub fn publish(
        &mut self,
        topic: &PubsubTopic,
        msg: WakuMessage,
    ) -> Result<MessageId, PublishError> {
        let ident_topic = IdentTopic::new(topic.to_string());
        let message_proto: WakuMessageProto = msg.into();
        self.pubsub
            .publish(ident_topic, message_proto.encode_to_vec())
            .map_err(Into::into)
    }

    pub fn add_peer(&mut self, peer_id: &PeerId) {
        self.pubsub.add_explicit_peer(peer_id);
    }
}
