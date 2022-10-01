use std::io;

use crate::request::HistoryRequest;
use crate::rpc::proto::waku::store::v2beta4::history_rpc::ReqRes;
use crate::rpc::proto::waku::store::v2beta4::paging_info::Direction;
use crate::rpc::proto::waku::store::v2beta4::{
    ContentFilter, HistoryQuery, HistoryRpc, PagingInfo,
};

impl TryInto<HistoryRequest> for HistoryRpc {
    type Error = io::Error;

    fn try_into(self) -> Result<HistoryRequest, Self::Error> {
        let request_id = self.request_id.clone();
        let request_query = match self.req_res {
            Some(ReqRes::Request(query)) => query,
            _ => return Err(io::ErrorKind::InvalidData.into()),
        };

        let request_pubsub_topic = request_query.pubsub_topic;
        let request_content_topics = request_query
            .content_filters
            .iter()
            .map(|cf| cf.content_topic.clone())
            .collect::<Vec<String>>();
        let request_page_size = request_query
            .paging_info
            .as_ref()
            .map(|info| info.page_size as usize);
        let request_ascending = request_query
            .paging_info
            .as_ref()
            .map(|info| info.direction == Direction::Forward.into());
        let request_cursor = request_query
            .paging_info
            .as_ref()
            .and_then(|info| info.cursor.as_ref())
            .map(|cursor| cursor.into());
        let request_start_time = request_query.start_time;
        let request_end_time = request_query.start_time;

        Ok(HistoryRequest {
            request_id,
            pubsub_topic: request_pubsub_topic,
            content_topics: request_content_topics,
            page_size: request_page_size,
            ascending: request_ascending,
            cursor: request_cursor,
            start_time: request_start_time,
            end_time: request_end_time,
        })
    }
}

impl Into<HistoryRpc> for HistoryRequest {
    fn into(self) -> HistoryRpc {
        let resp_paging_info =
            if self.page_size.is_none() && self.cursor.is_none() && self.ascending.is_none() {
                None
            } else {
                Some(PagingInfo {
                    page_size: self.page_size.unwrap_or(0) as u64,
                    cursor: self.cursor.map(|cursor| cursor.into()),
                    direction: i32::from(match self.ascending {
                        Some(asc) => {
                            if asc {
                                Direction::Forward
                            } else {
                                Direction::Backward
                            }
                        }
                        None => Direction::Forward,
                    }),
                })
            };
        let resp_content_filters = self
            .content_topics
            .iter()
            .map(|topic| ContentFilter {
                content_topic: topic.clone(),
            })
            .collect::<Vec<ContentFilter>>();

        HistoryRpc {
            request_id: self.request_id.clone(),
            req_res: Some(ReqRes::Request(HistoryQuery {
                pubsub_topic: self.pubsub_topic,
                content_filters: resp_content_filters,
                paging_info: resp_paging_info,
                start_time: self.start_time,
                end_time: self.end_time,
            })),
        }
    }
}
