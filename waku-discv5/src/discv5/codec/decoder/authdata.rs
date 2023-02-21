use std::convert::TryInto;

use bytes::{Buf, Bytes};

use crate::discv5::codec::decoder::error::DecoderError;
use crate::discv5::packet::authdata::{
    HANDSHAKE_AUTHDATA_EPHKEYSIZE_SIZE, HANDSHAKE_AUTHDATA_SIGSIZE_SIZE, HANDSHAKE_AUTHDATA_SRCID_SIZE, HandshakeAuthdata,
    MESSAGE_AUTHDATA_SRCID_SIZE, MessageAuthdata, WHOAREYOU_AUTHDATA_ENRSEQ_SIZE,
    WHOAREYOU_AUTHDATA_IDNONCE_SIZE, WhoAreYouAuthdata,
};

pub fn parse_message_authdata(mut buffer: Bytes) -> Result<MessageAuthdata, DecoderError> {
    // src-id
    if buffer.len() < MESSAGE_AUTHDATA_SRCID_SIZE {
        return Err(DecoderError::InsufficientBytes("src-id"));
    }
    let src_id_bytes = buffer.split_to(MESSAGE_AUTHDATA_SRCID_SIZE);
    let src_id = TryInto::try_into(&src_id_bytes[..]).unwrap();

    Ok(MessageAuthdata { src_id })
}

pub fn parse_whoareyou_authdata(mut buffer: Bytes) -> Result<WhoAreYouAuthdata, DecoderError> {
    // id-nonce
    if buffer.len() < WHOAREYOU_AUTHDATA_IDNONCE_SIZE {
        return Err(DecoderError::InsufficientBytes("id-nonce"));
    }
    let id_nonce = buffer.get_u128();

    // enr-seq
    if buffer.len() < WHOAREYOU_AUTHDATA_ENRSEQ_SIZE {
        return Err(DecoderError::InsufficientBytes("enr-seq"));
    }
    let enr_seq = buffer.get_u64();

    Ok(WhoAreYouAuthdata { id_nonce, enr_seq })
}

pub fn parse_handshake_authdata(mut buffer: Bytes) -> Result<HandshakeAuthdata, DecoderError> {
    // src-id
    if buffer.len() < HANDSHAKE_AUTHDATA_SRCID_SIZE {
        return Err(DecoderError::InsufficientBytes("src-id"));
    }
    let src_id_bytes = buffer.split_to(HANDSHAKE_AUTHDATA_SRCID_SIZE);
    let src_id = TryInto::try_into(&src_id_bytes[..]).unwrap();

    // sig-size
    if buffer.len() < HANDSHAKE_AUTHDATA_SIGSIZE_SIZE {
        return Err(DecoderError::InsufficientBytes("sig-size"));
    }
    let sig_size = buffer.get_u8() as usize;

    // eph-key-size
    if buffer.len() < HANDSHAKE_AUTHDATA_EPHKEYSIZE_SIZE {
        return Err(DecoderError::InsufficientBytes("eph-key-size"));
    }
    let eph_key_size = buffer.get_u8() as usize;

    // id-signature
    if buffer.len() < sig_size {
        return Err(DecoderError::InsufficientBytes("id-signature"));
    }
    let id_signature = buffer.split_to(sig_size);

    // ephemeral-pubkey
    if buffer.len() < eph_key_size {
        return Err(DecoderError::InsufficientBytes("ephemeral-pubkey"));
    }
    let ephemeral_pubkey = buffer.split_to(eph_key_size);

    // record
    let record = buffer;

    Ok(HandshakeAuthdata {
        src_id,
        id_signature,
        ephemeral_pubkey,
        record,
    })
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use bytes::Bytes;
    use hex_literal::hex;

    use super::{parse_handshake_authdata, parse_message_authdata, parse_whoareyou_authdata};

    #[test]
    fn test_valid_message_authdata() {
        //// Given
        // Test vector: MESSAGE<Ping> packet (flag = 0)
        // From: https://github.com/ethereum/devp2p/blob/master/discv5/discv5-wire-test-vectors.md
        const AUTHDATA: [u8; 32] =
            hex!("aaaa8419e9f49d0083561b48287df592939a8d19947d8c0ef88f2a4856a69fbb");

        let mut buffer = Bytes::from_static(&AUTHDATA[..]);

        //// When
        let authdata = parse_message_authdata(buffer);

        //// Then
        assert_matches!(authdata, Ok(auth) => {
           assert_eq!(auth.src_id, hex!("aaaa8419e9f49d0083561b48287df592939a8d19947d8c0ef88f2a4856a69fbb"))
        });
    }

    #[test]
    fn test_valid_whouareyou_authdata() {
        //// Given
        // Test vector: WHOAREYOU packet (flag = 1)
        // From: https://github.com/ethereum/devp2p/blob/master/discv5/discv5-wire-test-vectors.md
        const AUTHDATA: [u8; 24] = hex!("0102030405060708090a0b0c0d0e0f100000000000000000");

        let mut buffer = Bytes::from_static(&AUTHDATA[..]);

        //// When
        let authdata = parse_whoareyou_authdata(buffer);

        //// Then
        assert_matches!(authdata, Ok(auth) => {
            assert_eq!(auth.id_nonce, 0x0102030405060708090a0b0c0d0e0f10);
            assert_eq!(auth.enr_seq, 0)
        });
    }

    #[test]
    fn test_valid_handshake_authdata() {
        //// Given
        // Test vector: HANDSHAKE<Ping> packet (flag = 2)
        // From: https://github.com/ethereum/devp2p/blob/master/discv5/discv5-wire-test-vectors.md
        const AUTHDATA: [u8; 131] = hex!(
            "aaaa8419e9f49d0083561b48287df592939a8d19947d8c0ef88f2a4856a69fbb
             4021c0a04b36f276172afc66a62848eb0769800c670c4edbefab8f26785e7fda
             6b56506a3f27ca72a75b106edd392a2cbf8a69272f5c1785c36d1de9d98a0894
             b2db039a003ba6517b473fa0cd74aefe99dadfdb34627f90fec6362df8580390
             8f53a5"
        );

        let mut buffer = Bytes::from_static(&AUTHDATA[..]);

        //// When
        let authdata = parse_handshake_authdata(buffer);

        //// Then
        assert_matches!(authdata, Ok(auth) => {
            assert_eq!(auth.src_id, hex!("aaaa8419e9f49d0083561b48287df592939a8d19947d8c0ef88f2a4856a69fbb"));
            assert_eq!(*auth.id_signature, hex!("c0a04b36f276172afc66a62848eb0769800c670c4edbefab8f26785e7fda6b56506a3f27ca72a75b106edd392a2cbf8a69272f5c1785c36d1de9d98a0894b2db"));
            assert_eq!(*auth.ephemeral_pubkey, hex!("039a003ba6517b473fa0cd74aefe99dadfdb34627f90fec6362df85803908f53a5"));
            assert!(auth.record.is_empty())
        });
    }

    #[test]
    fn test_valid_handshake_authdata_with_enr() {
        //// Given
        // Test vector: HANDSHAKE<Ping> packet (flag = 2)
        // From: https://github.com/ethereum/devp2p/blob/master/discv5/discv5-wire-test-vectors.md
        const AUTHDATA: [u8; 258] = hex!(
            "aaaa8419e9f49d0083561b48287df592939a8d19947d8c0ef88f2a4856a69fbb
             4021a439e69918e3f53f555d8ca4838fbe8abeab56aa55b056a2ac4d49c157ee
             719240a93f56c9fccfe7742722a92b3f2dfa27a5452f5aca8adeeab8c4d5d87d
             f555039a003ba6517b473fa0cd74aefe99dadfdb34627f90fec6362df8580390
             8f53a5f87db84017e1b073918da32d640642c762c0e2781698e4971f8ab39a77
             746adad83f01e76ffc874c5924808bbe7c50890882c2b8a01287a0b08312d1d5
             3a17d517f5eb2701826964827634826970847f00000189736563703235366b31
             a10313d14211e0287b2361a1615890a9b5212080546d0a257ae4cff96cf53499
             2cb9"
        );

        let mut buffer = Bytes::from_static(&AUTHDATA[..]);

        //// When
        let authdata = parse_handshake_authdata(buffer);

        //// Then
        assert_matches!(authdata, Ok(auth) => {
            assert_eq!(auth.src_id, hex!("aaaa8419e9f49d0083561b48287df592939a8d19947d8c0ef88f2a4856a69fbb"));
            assert_eq!(*auth.id_signature, hex!("a439e69918e3f53f555d8ca4838fbe8abeab56aa55b056a2ac4d49c157ee719240a93f56c9fccfe7742722a92b3f2dfa27a5452f5aca8adeeab8c4d5d87df555"));
            assert_eq!(*auth.ephemeral_pubkey, hex!("039a003ba6517b473fa0cd74aefe99dadfdb34627f90fec6362df85803908f53a5"));
            assert!(auth.record.len() > 0)
        });
    }
}
