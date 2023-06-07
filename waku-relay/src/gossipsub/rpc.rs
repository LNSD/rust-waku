pub use proto::waku::relay::v2::{
    ControlGraft as ControlGraftProto, ControlIHave as ControlIHaveProto, ControlIHave,
    ControlIWant as ControlIWantProto, ControlMessage as ControlMessageProto,
    ControlPrune as ControlPruneProto, Message as MessageProto, PeerInfo as PeerInfoProto,
    Rpc as RpcProto, TopicDescriptor as TopicDescriptorProto,
};
pub use types::MessageRpc;
pub use validation::validate_message_proto;

mod proto;
mod traits;
mod types;
mod validation;
