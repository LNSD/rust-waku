use bytes::Bytes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRequest {
    pub(crate) request_id: String,
    pub(crate) pubsub_topic: Option<String>,
    pub(crate) content_topics: Vec<String>,
    pub(crate) page_size: Option<usize>,
    pub(crate) ascending: Option<bool>,
    pub(crate) cursor: Option<(i64, Bytes, String)>,
    pub(crate) start_time: Option<i64>,
    pub(crate) end_time: Option<i64>,
}
