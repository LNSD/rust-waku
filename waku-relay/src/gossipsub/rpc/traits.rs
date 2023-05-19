use bytes::Bytes;

use crate::gossipsub::rpc::proto::waku::relay::v2::{
    rpc::SubOpts as SubOptsProto, ControlGraft as ControlGraftProto,
    ControlIHave as ControlIHaveProto, ControlIWant as ControlIWantProto,
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
