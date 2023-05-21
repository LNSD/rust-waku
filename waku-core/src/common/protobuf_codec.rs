use std::io;
use std::marker::PhantomData;

use asynchronous_codec::{Decoder, Encoder};
pub use asynchronous_codec::{FramedRead, FramedWrite};
use bytes::BytesMut;
use prost::Message;
use unsigned_varint::codec::UviBytes;

/// [`Codec`] implements [`Encoder`] and [`Decoder`], uses [`unsigned_varint`]
/// to prefix messages with their length and uses [`prost`] and a provided
/// `struct` implementing [`Message`] to do the encoding.
pub struct Codec<In, Out = In> {
    uvi: UviBytes,
    phantom: PhantomData<(In, Out)>,
}

impl<In, Out> Codec<In, Out> {
    /// Create new [`Codec`].
    ///
    /// Parameter `max_message_len_bytes` determines the maximum length of the
    /// Protobuf message. The limit does not include the bytes needed for the
    /// [`unsigned_varint`].
    pub fn new(max_message_len_bytes: usize) -> Self {
        let mut uvi = UviBytes::default();
        uvi.set_max_len(max_message_len_bytes);
        Self {
            uvi,
            phantom: PhantomData::default(),
        }
    }
}

impl<In: Message, Out> Encoder for Codec<In, Out> {
    type Item = In;
    type Error = io::Error;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let mut encoded_msg = BytesMut::with_capacity(item.encoded_len());
        item.encode(&mut encoded_msg)
            .expect("BytesMut to have sufficient capacity.");
        self.uvi.encode(encoded_msg.freeze(), dst)
    }
}

impl<In, Out: Message + Default> Decoder for Codec<In, Out> {
    type Item = Out;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        Ok(self
            .uvi
            .decode(src)?
            .map(|msg| Message::decode(msg))
            .transpose()?)
    }
}
