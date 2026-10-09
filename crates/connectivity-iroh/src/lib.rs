use std::{str::FromStr, sync::Arc};

use async_trait::async_trait;
use bytes::Bytes;
use connectivity_api::{
    ConnectivityDriver, ControlIo, DialTarget, DriverError, PathObservation, PeerTransportIdentity,
    TransportConnection,
};
use futures_util::{StreamExt, stream::BoxStream};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayUrl, TransportAddr};

pub const PERSONAL_EXIT_ALPN: &[u8] = b"ourproduct/personal-exit/0";

pub struct IrohDriver {
    endpoint: Endpoint,
}

impl IrohDriver {
    pub fn new(endpoint: Endpoint) -> Self {
        Self { endpoint }
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}

struct IrohConnection {
    inner: Arc<iroh::endpoint::Connection>,
}

#[async_trait]
impl ConnectivityDriver for IrohDriver {
    async fn dial(&self, target: DialTarget) -> Result<Box<dyn TransportConnection>, DriverError> {
        let endpoint_id = parse_endpoint_id(&target.peer)?;
        let mut addrs = Vec::new();
        for relay in target.relay_urls {
            let relay = RelayUrl::from_str(&relay)
                .map_err(|e| DriverError::InvalidTarget(e.to_string()))?;
            addrs.push(TransportAddr::Relay(relay));
        }
        for direct in target.direct_addrs {
            let addr = direct
                .parse()
                .map_err(|e: std::net::AddrParseError| DriverError::InvalidTarget(e.to_string()))?;
            addrs.push(TransportAddr::Ip(addr));
        }
        let remote = EndpointAddr::from_parts(endpoint_id, addrs);
        let conn = self
            .endpoint
            .connect(remote, PERSONAL_EXIT_ALPN)
            .await
            .map_err(|_| DriverError::PeerUnreachable)?;
        Ok(Box::new(IrohConnection {
            inner: Arc::new(conn),
        }))
    }

    async fn accept(&self) -> Result<Box<dyn TransportConnection>, DriverError> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or(DriverError::ConnectionLost)?;
        let mut accepting = incoming
            .accept()
            .map_err(|e| DriverError::Internal(e.to_string()))?;
        let alpn = accepting
            .alpn()
            .await
            .map_err(|_| DriverError::ProtocolMismatch)?;
        if alpn.as_slice() != PERSONAL_EXIT_ALPN {
            return Err(DriverError::ProtocolMismatch);
        }
        let conn = accepting
            .await
            .map_err(|e| DriverError::Internal(e.to_string()))?;
        Ok(Box::new(IrohConnection {
            inner: Arc::new(conn),
        }))
    }
}

#[async_trait]
impl TransportConnection for IrohConnection {
    fn peer_identity(&self) -> PeerTransportIdentity {
        PeerTransportIdentity(self.inner.remote_id().as_bytes().to_vec())
    }

    async fn open_control(&self) -> Result<ControlIo, DriverError> {
        let (send, recv) = self
            .inner
            .open_bi()
            .await
            .map_err(|_| DriverError::ConnectionLost)?;
        Ok(ControlIo {
            send: Box::pin(send),
            recv: Box::pin(recv),
        })
    }

    async fn accept_control(&self) -> Result<ControlIo, DriverError> {
        let (send, recv) = self
            .inner
            .accept_bi()
            .await
            .map_err(|_| DriverError::ConnectionLost)?;
        Ok(ControlIo {
            send: Box::pin(send),
            recv: Box::pin(recv),
        })
    }

    async fn send_packet(&self, frame: Bytes) -> Result<(), DriverError> {
        self.inner
            .send_datagram_wait(frame)
            .await
            .map_err(|_| DriverError::Backpressure)
    }

    async fn recv_packets(&self, out: &mut [Bytes]) -> Result<usize, DriverError> {
        self.inner
            .read_many_datagrams(out)
            .await
            .map_err(|_| DriverError::ConnectionLost)
    }

    fn max_packet_payload(&self) -> Option<usize> {
        self.inner.max_datagram_size()
    }

    fn path_snapshot(&self) -> Vec<PathObservation> {
        Vec::new()
    }

    fn path_events(&self) -> BoxStream<'static, PathObservation> {
        futures_util::stream::empty().boxed()
    }

    async fn closed(&self) -> DriverError {
        let _ = self.inner.closed().await;
        DriverError::ConnectionLost
    }

    fn close(&self, reason: &'static [u8]) {
        self.inner.close(0u8.into(), reason);
    }
}

fn parse_endpoint_id(identity: &PeerTransportIdentity) -> Result<EndpointId, DriverError> {
    let bytes: [u8; 32] = identity
        .0
        .as_slice()
        .try_into()
        .map_err(|_| DriverError::InvalidTarget("Iroh EndpointId must be 32 bytes".into()))?;
    EndpointId::from_bytes(&bytes).map_err(|e| DriverError::InvalidTarget(e.to_string()))
}
