use byteorder::{BigEndian, ByteOrder};
use bytes::Bytes;
use libp2p::PeerId;

use crate::gossipsub::rpc::proto::waku::relay::v2::{
    rpc::SubOpts as SubOptsProto, ControlGraft as ControlGraftProto,
    ControlIHave as ControlIHaveProto, ControlIHave, ControlIWant as ControlIWantProto,
    ControlMessage as ControlMessageProto, ControlPrune as ControlPruneProto,
    Message as MessageProto, PeerInfo as PeerInfoProto, Rpc as RpcProto,
};
use crate::gossipsub::types::{ControlAction, PeerInfo, Subscription, SubscriptionAction};
use crate::gossipsub::{RawMessage, Rpc, TopicHash};

impl From<RawMessage> for MessageProto {
    /// Converts the message into protobuf format.
    fn from(msg: RawMessage) -> Self {
        Self {
            from: msg.source.map(|m| Bytes::from(m.to_bytes())),
            data: Some(Bytes::from(msg.data)),
            seqno: msg
                .sequence_number
                .map(|s| Bytes::copy_from_slice(&s.to_be_bytes())),
            topic: TopicHash::into_string(msg.topic),
            signature: msg.signature.map(Bytes::from),
            key: msg.key.map(Bytes::from),
        }
    }
}

impl From<MessageProto> for RawMessage {
    fn from(msg: MessageProto) -> Self {
        Self {
            source: msg
                .from
                .map(|b| PeerId::from_bytes(&b).expect("PeerId to be valid")),
            data: msg.data.map(Into::into).unwrap_or_default(),
            sequence_number: msg.seqno.as_ref().map(|v| BigEndian::read_u64(v)),
            topic: TopicHash::from_raw(msg.topic),
            signature: msg.signature.map(Into::into),
            key: msg.key.map(Into::into),
            validated: false,
        }
    }
}

impl From<Subscription> for SubOptsProto {
    /// Converts the subscription into protobuf format.
    fn from(sub: Subscription) -> Self {
        Self {
            subscribe: Some(sub.action == SubscriptionAction::Subscribe),
            topic_id: Some(sub.topic_hash.into_string()),
        }
    }
}

impl From<SubOptsProto> for Subscription {
    fn from(sub: SubOptsProto) -> Self {
        Self {
            action: if Some(true) == sub.subscribe {
                SubscriptionAction::Subscribe
            } else {
                SubscriptionAction::Unsubscribe
            },
            topic_hash: TopicHash::from_raw(sub.topic_id.unwrap_or_default()),
        }
    }
}

impl From<PeerInfo> for PeerInfoProto {
    /// Converts the peer info into protobuf format.
    fn from(info: PeerInfo) -> Self {
        Self {
            peer_id: info.peer_id.map(|id| Bytes::from(id.to_bytes())),
            /// TODO, see https://github.com/libp2p/specs/pull/217
            signed_peer_record: None,
        }
    }
}

impl TryFrom<PeerInfoProto> for PeerInfo {
    type Error = anyhow::Error;

    fn try_from(info: PeerInfoProto) -> Result<Self, Self::Error> {
        let peer_id = info.peer_id.unwrap();
        let peer_id = PeerId::from_bytes(&peer_id[..])?;
        Ok(Self {
            peer_id: Some(peer_id),
        })
    }
}

impl FromIterator<ControlAction> for ControlMessageProto {
    fn from_iter<I: IntoIterator<Item = ControlAction>>(iter: I) -> Self {
        let mut control = ControlMessageProto {
            ihave: Vec::new(),
            iwant: Vec::new(),
            graft: Vec::new(),
            prune: Vec::new(),
        };

        for action in iter {
            match action {
                ControlAction::IHave {
                    topic_hash,
                    message_ids,
                } => {
                    let rpc_ihave = ControlIHaveProto {
                        topic_id: Some(topic_hash.into_string()),
                        message_ids: message_ids.into_iter().map(Into::into).collect(),
                    };
                    control.ihave.push(rpc_ihave);
                }

                ControlAction::IWant { message_ids } => {
                    let rpc_iwant = ControlIWantProto {
                        message_ids: message_ids.into_iter().map(Into::into).collect(),
                    };
                    control.iwant.push(rpc_iwant);
                }

                ControlAction::Graft { topic_hash } => {
                    let rpc_graft = ControlGraftProto {
                        topic_id: Some(topic_hash.into_string()),
                    };
                    control.graft.push(rpc_graft);
                }

                ControlAction::Prune {
                    topic_hash,
                    peers,
                    backoff,
                } => {
                    let rpc_prune = ControlPruneProto {
                        topic_id: Some(topic_hash.into_string()),
                        peers: peers.into_iter().map(Into::into).collect(),
                        backoff,
                    };
                    control.prune.push(rpc_prune);
                }
            }
        }

        control
    }
}

impl From<ControlIHaveProto> for ControlAction {
    fn from(ihave: ControlIHave) -> Self {
        Self::IHave {
            topic_hash: TopicHash::from_raw(ihave.topic_id.unwrap_or_default()),
            message_ids: ihave.message_ids.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ControlIWantProto> for ControlAction {
    fn from(iwant: ControlIWantProto) -> Self {
        Self::IWant {
            message_ids: iwant.message_ids.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ControlGraftProto> for ControlAction {
    fn from(graft: ControlGraftProto) -> Self {
        Self::Graft {
            topic_hash: TopicHash::from_raw(graft.topic_id.unwrap_or_default()),
        }
    }
}

impl From<ControlPruneProto> for ControlAction {
    fn from(prune: ControlPruneProto) -> Self {
        let peers = prune
            .peers
            .into_iter()
            .filter_map(|info| PeerInfo::try_from(info).ok()) // filter out invalid peers
            .collect();

        let topic_hash = TopicHash::from_raw(prune.topic_id.unwrap_or_default());

        Self::Prune {
            topic_hash,
            peers,
            backoff: prune.backoff,
        }
    }
}

impl From<Rpc> for RpcProto {
    /// Converts the RPC into protobuf format.
    fn from(rpc: Rpc) -> Self {
        let publish = rpc.messages.into_iter().map(Into::into).collect();
        let subscriptions = rpc.subscriptions.into_iter().map(Into::into).collect();
        let control = rpc
            .control_msgs
            .is_empty()
            .then(|| rpc.control_msgs.into_iter().collect());

        Self {
            subscriptions,
            publish,
            control,
        }
    }
}
