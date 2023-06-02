// Copyright 2020 Sigma Prime Pty Ltd.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use std::convert::Infallible;
use std::pin::Pin;

use asynchronous_codec::Framed;
use futures::future;
use futures::prelude::*;
use libp2p::core::{InboundUpgrade, OutboundUpgrade, ProtocolName, UpgradeInfo};

use crate::gossipsub::codec::Codec;
use crate::gossipsub::Config;
use crate::gossipsub::config::Version;
use crate::gossipsub::types::PeerKind;

/// The protocol ID
#[derive(Clone, Debug)]
pub struct ProtocolId {
    /// The RPC message type/name.
    pub protocol_id: Vec<u8>,
    /// The type of protocol we support
    pub kind: PeerKind,
}

/// An RPC protocol ID.
impl ProtocolId {
    pub fn new(id: &str, kind: PeerKind, prefix: bool) -> Self {
        let protocol_id = match kind {
            PeerKind::Gossipsubv1_1 => match prefix {
                true => format!("/{}/{}", id, "1.1.0"),
                false => id.to_string(),
            },
            PeerKind::Gossipsub => match prefix {
                true => format!("/{}/{}", id, "1.0.0"),
                false => id.to_string(),
            },
            PeerKind::Floodsub => format!("/{}/{}", "floodsub", "1.0.0"),
            // NOTE: This is used for informing the behaviour of unsupported peers. We do not
            // advertise this variant.
            PeerKind::NotSupported => unreachable!("Should never advertise NotSupported"),
        }
        .into_bytes();
        ProtocolId { protocol_id, kind }
    }
}

impl ProtocolName for ProtocolId {
    fn protocol_name(&self) -> &[u8] {
        &self.protocol_id
    }
}

/// Implementation of [`InboundUpgrade`] and [`OutboundUpgrade`] for the Gossipsub protocol.
#[derive(Debug, Clone)]
pub struct ProtocolUpgrade {
    /// The Gossipsub protocol id to listen on.
    protocol_ids: Vec<ProtocolId>,
    /// The maximum transmit size for a packet.
    max_transmit_size: usize,
}

impl ProtocolUpgrade {
    /// Builds a new [`ProtocolUpgrade`].
    ///
    /// Sets the maximum gossip transmission size.
    pub fn new(gossipsub_config: &Config) -> ProtocolUpgrade {
        let mut protocol_ids = match gossipsub_config.custom_id_version() {
            Some(v) => match v {
                Version::V1_0 => vec![ProtocolId::new(
                    gossipsub_config.protocol_id(),
                    PeerKind::Gossipsub,
                    false,
                )],
                Version::V1_1 => vec![ProtocolId::new(
                    gossipsub_config.protocol_id(),
                    PeerKind::Gossipsubv1_1,
                    false,
                )],
            },
            None => {
                vec![
                    ProtocolId::new(
                        gossipsub_config.protocol_id(),
                        PeerKind::Gossipsubv1_1,
                        true,
                    ),
                    ProtocolId::new(gossipsub_config.protocol_id(), PeerKind::Gossipsub, true),
                ]
            }
        };

        // add floodsub support if enabled.
        if gossipsub_config.support_floodsub() {
            protocol_ids.push(ProtocolId::new("", PeerKind::Floodsub, false));
        }

        ProtocolUpgrade {
            protocol_ids,
            max_transmit_size: gossipsub_config.max_transmit_size(),
        }
    }
}

impl UpgradeInfo for ProtocolUpgrade {
    type Info = ProtocolId;
    type InfoIter = Vec<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        self.protocol_ids.clone()
    }
}

impl<TSocket> InboundUpgrade<TSocket> for ProtocolUpgrade
where
    TSocket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = (Framed<TSocket, Codec>, PeerKind);
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, socket: TSocket, protocol_id: Self::Info) -> Self::Future {
        Box::pin(future::ok((
            Framed::new(socket, Codec::new(self.max_transmit_size)),
            protocol_id.kind,
        )))
    }
}

impl<TSocket> OutboundUpgrade<TSocket> for ProtocolUpgrade
where
    TSocket: AsyncWrite + AsyncRead + Unpin + Send + 'static,
{
    type Output = (Framed<TSocket, Codec>, PeerKind);
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, socket: TSocket, protocol_id: Self::Info) -> Self::Future {
        Box::pin(future::ok((
            Framed::new(socket, Codec::new(self.max_transmit_size)),
            protocol_id.kind,
        )))
    }
}
