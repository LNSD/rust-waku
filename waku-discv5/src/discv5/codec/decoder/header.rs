use aes::cipher::{KeyIvInit, StreamCipher};
use aes::cipher::consts::U16;
use aes::cipher::generic_array::GenericArray;
use bytes::{Buf, Bytes, BytesMut};

use crate::discv5::codec::decoder::error::DecoderError;

type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;

/// Packet header field sizes
const HEADER_PROTOCOL_ID_SIZE: usize = 6;
const HEADER_VERSION_SIZE: usize = 2;
const HEADER_FLAG_SIZE: usize = 1;
const HEADER_NONCE_SIZE: usize = 12;
const HEADER_AUTHSIZE_SIZE: usize = 2;

enum HeaderToken {
    ProtocolId,
    Version,
    Flag,
    Nonce,
    AuthData,
}

#[derive(Debug)]
pub enum HeaderField {
    ProtocolId(Bytes),
    Version(u16),
    Flag(u8),
    Nonce(Bytes),
    AuthData(Bytes),
}

static HEADER_TOKENS: &[HeaderToken] = &[
    HeaderToken::ProtocolId,
    HeaderToken::Version,
    HeaderToken::Flag,
    HeaderToken::Nonce,
    HeaderToken::AuthData,
];

pub struct Decoder<'dec> {
    buffer: &'dec mut BytesMut,
    token_iter: Box<dyn Iterator<Item=&'static HeaderToken>>,
    cipher: Aes128Ctr,
}

impl<'dec> Decoder<'dec> {
    fn new_cipher(masking_key: &'dec [u8; 16], masking_iv: &'dec [u8; 16]) -> Aes128Ctr {
        let masking_key: GenericArray<u8, U16> = GenericArray::clone_from_slice(masking_key);
        let masking_iv: GenericArray<u8, U16> = GenericArray::clone_from_slice(masking_iv);
        Aes128Ctr::new(&masking_key.into(), &masking_iv.into())
    }

    pub fn new(
        buffer: &'dec mut BytesMut,
        masking_key: &'dec [u8; 16],
        masking_iv: &'dec [u8; 16],
    ) -> Self {
        Self {
            buffer,
            token_iter: Box::new(HEADER_TOKENS.iter()),
            cipher: Self::new_cipher(masking_key, masking_iv),
        }
    }

    fn unmask_and_extract_protocol_id(&mut self) -> Bytes {
        let mut protocol_id_bytes = self.buffer.split_to(HEADER_PROTOCOL_ID_SIZE);
        self.cipher.apply_keystream(&mut protocol_id_bytes);

        protocol_id_bytes.freeze()
    }

    fn unmask_and_extract_version(&mut self) -> u16 {
        let mut version_bytes = self.buffer.split_to(HEADER_VERSION_SIZE);
        self.cipher.apply_keystream(&mut version_bytes);

        version_bytes.get_u16()
    }

    fn unmask_and_extract_flag(&mut self) -> u8 {
        let mut flag_bytes = self.buffer.split_to(HEADER_FLAG_SIZE);
        self.cipher.apply_keystream(&mut flag_bytes);

        flag_bytes.get_u8()
    }

    fn unmask_and_extract_nonce(&mut self) -> Bytes {
        let mut nonce_bytes = self.buffer.split_to(HEADER_NONCE_SIZE);
        self.cipher.apply_keystream(&mut nonce_bytes);

        nonce_bytes.freeze()
    }

    fn unmask_and_extract_authsize(&mut self) -> usize {
        let mut authsize_bytes = self.buffer.split_to(HEADER_AUTHSIZE_SIZE);
        self.cipher.apply_keystream(&mut authsize_bytes);

        authsize_bytes.get_u16() as usize
    }

    fn unmask_and_extract_authdata(&mut self, size: usize) -> Bytes {
        let mut authdata_bytes = self.buffer.split_to(size);
        self.cipher.apply_keystream(&mut authdata_bytes);

        authdata_bytes.freeze()
    }

    fn extract(&mut self, token: &HeaderToken) -> Result<HeaderField, DecoderError> {
        match token {
            HeaderToken::ProtocolId => {
                if self.buffer.len() < HEADER_PROTOCOL_ID_SIZE {
                    return Err(DecoderError::InsufficientBytes("protocol-id"));
                }

                let protocol_id = self.unmask_and_extract_protocol_id();
                Ok(HeaderField::ProtocolId(protocol_id))
            }
            HeaderToken::Version => {
                if self.buffer.len() < HEADER_VERSION_SIZE {
                    return Err(DecoderError::InsufficientBytes("version"));
                }

                let version = self.unmask_and_extract_version();
                Ok(HeaderField::Version(version))
            }
            HeaderToken::Flag => {
                if self.buffer.len() < HEADER_FLAG_SIZE {
                    return Err(DecoderError::InsufficientBytes("flag"));
                }

                let flag = self.unmask_and_extract_flag();
                Ok(HeaderField::Flag(flag))
            }
            HeaderToken::Nonce => {
                if self.buffer.len() < HEADER_NONCE_SIZE {
                    return Err(DecoderError::InsufficientBytes("nonce"));
                }

                let nonce = self.unmask_and_extract_nonce();
                Ok(HeaderField::Nonce(nonce))
            }
            HeaderToken::AuthData => {
                if self.buffer.len() < HEADER_AUTHSIZE_SIZE {
                    return Err(DecoderError::InsufficientBytes("authsize"));
                }

                let authsize = self.unmask_and_extract_authsize();

                if self.buffer.len() < authsize {
                    return Err(DecoderError::InsufficientBytes("authdata"));
                }

                let authdata = self.unmask_and_extract_authdata(authsize);
                Ok(HeaderField::AuthData(authdata))
            }
        }
    }
}

impl<'dec> Iterator for Decoder<'dec> {
    type Item = Result<HeaderField, DecoderError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.token_iter.next().map(|token| self.extract(token))
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use bytes::{Bytes, BytesMut};
    use hex_literal::hex;

    use super::{Decoder, DecoderError, HeaderField};

    #[test]
    fn test_valid_header() {
        //// Given
        // Test vector: HANDSHAKE<Ping> packet (flag = 2)
        // From: https://github.com/ethereum/devp2p/blob/master/discv5/discv5-wire-test-vectors.md
        const MASKING_KEY: [u8; 16] = hex!("bbbb9d047f0488c0b5a93c1c3f2d8baf");
        const MASKING_IV: [u8; 16] = hex!("00000000000000000000000000000000");
        const PACKET_RAW: [u8; 178] = hex!(
            "088b3d4342774649305f313964a39e55ea96c005ad521d8c7560413a7008f16c
             9e6d2f43bbea8814a546b7409ce783d34c4f53245d08da4bb252012b2cba3f4f
             374a90a75cff91f142fa9be3e0a5f3ef268ccb9065aeecfd67a999e7fdc137e0
             62b2ec4a0eb92947f0d9a74bfbf44dfba776b21301f8b65efd5796706adff216
             ab862a9186875f9494150c4ae06fa4d1f0396c93f215fa4ef524f1eadf5f0f41
             26b79336671cbcf7a885b1f8bd2a5d839cf8"
        );

        let mut buffer = BytesMut::from(&PACKET_RAW[..]);

        //// When
        let mut decoder = Decoder::new(&mut buffer, &MASKING_KEY, &MASKING_IV);

        let protocol_id = decoder.next();
        let version = decoder.next();
        let flag = decoder.next();
        let nonce = decoder.next();
        let authdata = decoder.next();

        let no_field = decoder.next();

        //// Then
        assert_matches!(protocol_id, Some(Ok(HeaderField::ProtocolId(value))) => {
           assert_eq!(value, Bytes::from_static(b"discv5"))
        });
        assert_matches!(version, Some(Ok(HeaderField::Version(value))) => {
            assert_eq!(value, 0x0001)
        });
        assert_matches!(flag, Some(Ok(HeaderField::Flag(value))) => {
            assert_eq!(value, 2)
        });
        assert_matches!(nonce, Some(Ok(HeaderField::Nonce(value))) => {
            assert_eq!(value, Bytes::copy_from_slice(&[0xff; 12]))
        });
        assert_matches!(authdata, Some(Ok(HeaderField::AuthData(value))) => {
            assert_eq!(value.len(), 131)
        });

        assert!(no_field.is_none());
        assert!(buffer.len() > 0);
    }

    #[test]
    fn test_invalid_truncated_header_nonce() {
        //// Given
        // Test vector: HANDSHAKE<Ping> packet (flag = 2)
        // From: https://github.com/ethereum/devp2p/blob/master/discv5/discv5-wire-test-vectors.md
        const MASKING_KEY: [u8; 16] = hex!("bbbb9d047f0488c0b5a93c1c3f2d8baf");
        const MASKING_IV: [u8; 16] = hex!("00000000000000000000000000000000");
        const PACKET_RAW: [u8; 16] = hex!(
            // Header is truncated in the middle of the nonce field
            "088b3d4342774649305f313964a39e55"
        );

        let mut buffer = BytesMut::from(&PACKET_RAW[..]);

        //// When
        let mut decoder = Decoder::new(&mut buffer, &MASKING_KEY, &MASKING_IV);

        let protocol_id = decoder.next();
        let version = decoder.next();
        let flag = decoder.next();
        let nonce = decoder.next();

        let no_field = decoder.next();

        //// Then
        assert_matches!(protocol_id, Some(Ok(HeaderField::ProtocolId(value))) => {
           assert_eq!(value, Bytes::from_static(b"discv5"))
        });
        assert_matches!(version, Some(Ok(HeaderField::Version(value))) => {
            assert_eq!(value, 0x0001)
        });
        assert_matches!(flag, Some(Ok(HeaderField::Flag(value))) => {
            assert_eq!(value, 2)
        });
        assert_matches!(nonce, Some(Err(DecoderError::InsufficientBytes("nonce"))));

        assert!(no_field.is_none());
        assert_eq!(buffer.len(), 7);
    }

    #[test]
    fn test_invalid_truncated_header_authdata() {
        //// Given
        // Test vector: HANDSHAKE<Ping> packet (flag = 2)
        // From: https://github.com/ethereum/devp2p/blob/master/discv5/discv5-wire-test-vectors.md
        const MASKING_KEY: [u8; 16] = hex!("bbbb9d047f0488c0b5a93c1c3f2d8baf");
        const MASKING_IV: [u8; 16] = hex!("00000000000000000000000000000000");
        const PACKET_RAW: [u8; 32] = hex!(
            // Header is truncated in the middle of the authdata variable-length field
            "088b3d4342774649305f313964a39e55ea96c005ad521d8c7560413a7008f16c"
        );

        let mut buffer = BytesMut::from(&PACKET_RAW[..]);

        //// When
        let mut decoder = Decoder::new(&mut buffer, &MASKING_KEY, &MASKING_IV);

        let protocol_id = decoder.next();
        let version = decoder.next();
        let flag = decoder.next();
        let nonce = decoder.next();
        let authdata = decoder.next();

        let no_field = decoder.next();

        //// Then
        assert_matches!(protocol_id, Some(Ok(HeaderField::ProtocolId(value))) => {
           assert_eq!(value, Bytes::from_static(b"discv5"))
        });
        assert_matches!(version, Some(Ok(HeaderField::Version(value))) => {
            assert_eq!(value, 0x0001)
        });
        assert_matches!(flag, Some(Ok(HeaderField::Flag(value))) => {
            assert_eq!(value, 2)
        });
        assert_matches!(nonce, Some(Ok(HeaderField::Nonce(value))) => {
            assert_eq!(value, Bytes::copy_from_slice(&[0xff; 12]))
        });
        assert_matches!(
            authdata,
            Some(Err(DecoderError::InsufficientBytes("authdata")))
        );

        assert!(no_field.is_none());
        assert!(buffer.len() > 0);
    }
}
