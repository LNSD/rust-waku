use std::iter;

use libp2p::request_response::{ProtocolSupport, RequestResponse, RequestResponseEvent};

use crate::codec::WakuStoreCodec;
use crate::protocol::WakuStoreProtocol;
use crate::request::HistoryRequest;
use crate::response::HistoryResponse;

pub type WakuStoreProtocolEvent = RequestResponseEvent<HistoryRequest, HistoryResponse>;

pub type WakuStoreBehaviour = RequestResponse<WakuStoreCodec>;

// TODO: To be used with `SwarmBuilder`
pub fn new_waku_store_behaviour() -> WakuStoreBehaviour {
    RequestResponse::new(
        WakuStoreCodec(),
        iter::once((WakuStoreProtocol(), ProtocolSupport::Full)),
        Default::default(),
    )
}
