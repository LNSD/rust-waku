use std::io;

use async_trait::async_trait;
use asynchronous_codec::{FramedRead, FramedWrite};
use futures::{AsyncRead, AsyncWrite, SinkExt, StreamExt};
use libp2p::request_response::RequestResponseCodec;

use crate::common::protobuf_codec;
use crate::protocol::WakuStoreProtocol;
use crate::request::HistoryRequest;
use crate::response::HistoryResponse;
use crate::rpc::{HistoryRpc, MAX_PROTOBUF_SIZE};

/// The `WakuStoreCodec` defines the request and response types
/// for the [`RequestResponse`](crate::RequestResponse) protocol for
/// retrieving stored messages.
#[derive(Clone)]
pub struct WakuStoreCodec();

#[async_trait]
impl RequestResponseCodec for WakuStoreCodec {
    type Protocol = WakuStoreProtocol;
    type Request = HistoryRequest;
    type Response = HistoryResponse;

    async fn read_request<T>(
        &mut self,
        _: &WakuStoreProtocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
        where
            T: AsyncRead + Unpin + Send,
    {
        let rpc: HistoryRpc = FramedRead::new(
            io,
            protobuf_codec::Codec::<HistoryRpc>::new(MAX_PROTOBUF_SIZE),
        )
            .next()
            .await
            .ok_or(io::Error::from(io::ErrorKind::UnexpectedEof))??;

        let request: Result<HistoryRequest, io::Error> = rpc.try_into();
        if request.is_err() {
            return Err(io::ErrorKind::InvalidData.into());
        }

        Ok(request.unwrap())
    }

    async fn read_response<T>(
        &mut self,
        _: &WakuStoreProtocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
        where
            T: AsyncRead + Unpin + Send,
    {
        let rpc: HistoryRpc = FramedRead::new(
            io,
            protobuf_codec::Codec::<HistoryRpc>::new(MAX_PROTOBUF_SIZE),
        )
            .next()
            .await
            .ok_or(io::Error::from(io::ErrorKind::UnexpectedEof))??;

        let response: Result<HistoryResponse, io::Error> = rpc.try_into();
        if response.is_err() {
            return Err(io::ErrorKind::InvalidData.into());
        }

        Ok(response.unwrap())
    }

    async fn write_request<T>(
        &mut self,
        _: &WakuStoreProtocol,
        io: &mut T,
        request: HistoryRequest,
    ) -> io::Result<()>
        where
            T: AsyncWrite + Unpin + Send,
    {
        let rpc: HistoryRpc = request.into();

        let mut framed_io = FramedWrite::new(
            io,
            protobuf_codec::Codec::<HistoryRpc>::new(MAX_PROTOBUF_SIZE),
        );

        framed_io.send(rpc).await?;
        framed_io.close().await?;
        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _: &WakuStoreProtocol,
        io: &mut T,
        response: HistoryResponse,
    ) -> io::Result<()>
        where
            T: AsyncWrite + Unpin + Send,
    {
        let rpc: HistoryRpc = response.into();

        let mut framed_io = FramedWrite::new(
            io,
            protobuf_codec::Codec::<HistoryRpc>::new(MAX_PROTOBUF_SIZE),
        );

        framed_io.send(rpc).await?;
        framed_io.close().await?;
        Ok(())
    }
}
