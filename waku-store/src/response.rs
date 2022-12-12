use crate::pagination::PageCursor;
use crate::rpc::WakuMessage;

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryResponseBody {
    pub(crate) messages: Vec<WakuMessage>,
    pub(crate) next_page: Option<PageCursor>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum HistoryErrorKind {
    #[error("invalid pagination cursor")]
    InvalidCursor,

    #[error("unknown error: {0}")]
    Unknown(i32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryResponse {
    pub(crate) request_id: String,
    pub(crate) result: Result<HistoryResponseBody, HistoryErrorKind>,
}
