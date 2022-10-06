// @generated
///  17/WAKU-RLN-RELAY rfc: <https://rfc.vac.dev/spec/17/>
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RateLimitProof {
    #[prost(bytes="bytes", tag="1")]
    pub proof: ::prost::bytes::Bytes,
    #[prost(bytes="bytes", tag="2")]
    pub merkle_root: ::prost::bytes::Bytes,
    #[prost(bytes="bytes", tag="3")]
    pub epoch: ::prost::bytes::Bytes,
    #[prost(bytes="bytes", tag="4")]
    pub share_x: ::prost::bytes::Bytes,
    #[prost(bytes="bytes", tag="5")]
    pub share_y: ::prost::bytes::Bytes,
    #[prost(bytes="bytes", tag="6")]
    pub nullifier: ::prost::bytes::Bytes,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WakuMessage {
    #[prost(bytes="bytes", tag="1")]
    pub payload: ::prost::bytes::Bytes,
    #[prost(string, tag="2")]
    pub content_topic: ::prost::alloc::string::String,
    #[prost(uint32, tag="3")]
    pub version: u32,
    #[prost(sint64, optional, tag="10")]
    pub timestamp: ::core::option::Option<i64>,
    #[prost(message, optional, tag="21")]
    pub rate_limit_proof: ::core::option::Option<RateLimitProof>,
    #[prost(bool, optional, tag="31")]
    pub ephemeral: ::core::option::Option<bool>,
}
// @@protoc_insertion_point(module)
