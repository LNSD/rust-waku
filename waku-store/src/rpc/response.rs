use std::io;

use crate::response::{HistoryErrorKind, HistoryResponse, HistoryResponseBody};
use crate::rpc::{
    history_response, HistoryResponse as RpcResponse, HistoryRpc, paging_info, PagingInfo,
};
use crate::rpc::history_rpc::ReqRes;

impl From<i32> for HistoryErrorKind {
    fn from(error: i32) -> Self {
        match error {
            e if e == history_response::Error::InvalidCursor as i32 => {
                HistoryErrorKind::InvalidCursor
            }
            _ => HistoryErrorKind::Unknown(error),
        }
    }
}

impl From<HistoryErrorKind> for i32 {
    fn from(err: HistoryErrorKind) -> Self {
        match err {
            HistoryErrorKind::InvalidCursor => history_response::Error::InvalidCursor as i32,
            HistoryErrorKind::Unknown(_) => {
                unreachable!("Unknown error not supported by current protocol")
            }
        }
    }
}

impl TryInto<HistoryResponse> for HistoryRpc {
    type Error = io::Error;

    fn try_into(self) -> Result<HistoryResponse, Self::Error> {
        let request_id = self.request_id.clone();
        let response_body = match self.req_res {
            Some(ReqRes::Response(resp)) => resp,
            _ => return Err(io::ErrorKind::InvalidData.into()),
        };

        if response_body.error != history_response::Error::None as i32 {
            return Ok(HistoryResponse {
                request_id,
                result: Err(response_body.error.into()),
            });
        }

        let response_messages = response_body.messages;
        let response_cursor = response_body
            .paging_info
            .as_ref()
            .and_then(|info| info.cursor.as_ref())
            .map(|cursor| cursor.into());

        Ok(HistoryResponse {
            request_id,
            result: Ok(HistoryResponseBody {
                messages: response_messages,
                next_page: response_cursor,
            }),
        })
    }
}

impl Into<HistoryRpc> for HistoryResponse {
    fn into(self) -> HistoryRpc {
        let resp_request_id = self.request_id.clone();
        if self.result.is_err() {
            return HistoryRpc {
                request_id: resp_request_id,
                req_res: Some(ReqRes::Response(RpcResponse {
                    messages: vec![],
                    paging_info: None,
                    error: self.result.unwrap_err().into(),
                })),
            };
        }

        let resp = self.result.unwrap();
        let resp_messages = resp.messages.clone();
        let resp_paging_info = resp.next_page.map(|cursor| PagingInfo {
            page_size: resp_messages.len() as u64,
            cursor: Some(cursor.into()),
            direction: paging_info::Direction::Forward as i32,
        });

        HistoryRpc {
            request_id: resp_request_id,
            req_res: Some(ReqRes::Response(RpcResponse {
                messages: resp_messages,
                paging_info: resp_paging_info,
                error: history_response::Error::None as i32,
            })),
        }
    }
}
//
// impl Into<HistoryRpc> for HistoryError {
//     fn into(self) -> HistoryRpc {
//         HistoryRpc {
//             // TODO: Pass the request id into the response
//             request_id: String::new(),
//             req_res: Some(ReqRes::Response(RpcResponse {
//                 messages: Vec::new(),
//                 paging_info: None,
//                 error: self.into(),
//             })),
//         }
//     }
// }
