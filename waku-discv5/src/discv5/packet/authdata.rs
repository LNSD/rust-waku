use bytes::Bytes;

pub const MESSAGE_AUTHDATA_SIZE: usize = 32;
pub const MESSAGE_AUTHDATA_SRCID_SIZE: usize = 32;

#[derive(Debug)]
pub struct MessageAuthdata {
    pub(crate) src_id: [u8; 32],
}

pub const WHOAREYOU_AUTHDATA_SIZE: usize = 24;
pub const WHOAREYOU_AUTHDATA_IDNONCE_SIZE: usize = 16;
pub const WHOAREYOU_AUTHDATA_ENRSEQ_SIZE: usize = 8;

#[derive(Debug)]
pub struct WhoAreYouAuthdata {
    pub(crate) id_nonce: u128,
    pub(crate) enr_seq: u64,
}

pub const HANDSHAKE_AUTHDATA_MIN_SIZE: usize = 34;
pub const HANDSHAKE_AUTHDATA_SRCID_SIZE: usize = 32;
pub const HANDSHAKE_AUTHDATA_SIGSIZE_SIZE: usize = 1;
pub const HANDSHAKE_AUTHDATA_EPHKEYSIZE_SIZE: usize = 1;

#[derive(Debug)]
pub struct HandshakeAuthdata {
    pub(crate) src_id: [u8; 32],
    pub(crate) id_signature: Bytes,
    pub(crate) ephemeral_pubkey: Bytes,
    pub(crate) record: Bytes,
}
