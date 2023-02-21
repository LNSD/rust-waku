#[derive(Debug, thiserror::Error)]
pub enum DecoderError {
    #[error("Insufficient bytes to extract {0} header")]
    InsufficientBytes(&'static str),
}
