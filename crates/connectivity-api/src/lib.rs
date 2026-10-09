use std::pin::Pin;

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::BoxStream;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct PeerTransportIdentity(pub Vec<u8>);

#[derive(Clone, Debug)]
pub struct DialTarget {
    pub peer: PeerTransportIdentity,
    pub relay_urls: Vec<String>,
    pub direct_addrs: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum PathKind {
    DirectIp,
    Relay,
    Custom,
    Unknown,
}

#[derive(Clone, Debug)]
pub enum PathEventKind {
    Opened,
    Selected,
    Closed,
    Lagged { missed: u64 },
}

#[derive(Clone, Debug)]
pub struct PathObservation {
    pub kind: PathKind,
    pub event: PathEventKind,
    pub remote: Option<String>,
    pub local: Option<String>,
    pub rtt_ms: Option<u64>,
}

#[derive(Debug, Error)]
pub enum DriverError {
    #[error("underlay unavailable")]
    UnderlayUnavailable,
    #[error("relay unavailable")]
    RelayUnavailable,
    #[error("peer unreachable")]
    PeerUnreachable,
    #[error("authentication rejected")]
    AuthRejected,
    #[error("protocol mismatch")]
    ProtocolMismatch,
    #[error("transport connection lost")]
    ConnectionLost,
    #[error("backpressure")]
    Backpressure,
    #[error("datagrams unsupported")]
    DatagramUnsupported,
    #[error("invalid target: {0}")]
    InvalidTarget(String),
    #[error("driver internal error: {0}")]
    Internal(String),
}

pub type BoxAsyncRead = Pin<Box<dyn AsyncRead + Send>>;
pub type BoxAsyncWrite = Pin<Box<dyn AsyncWrite + Send>>;

pub struct ControlIo {
    pub send: BoxAsyncWrite,
    pub recv: BoxAsyncRead,
}

#[async_trait]
pub trait ConnectivityDriver: Send + Sync {
    async fn dial(&self, target: DialTarget) -> Result<Box<dyn TransportConnection>, DriverError>;
    async fn accept(&self) -> Result<Box<dyn TransportConnection>, DriverError>;
}

#[async_trait]
pub trait TransportConnection: Send + Sync {
    fn peer_identity(&self) -> PeerTransportIdentity;
    async fn open_control(&self) -> Result<ControlIo, DriverError>;
    async fn accept_control(&self) -> Result<ControlIo, DriverError>;
    async fn send_packet(&self, frame: Bytes) -> Result<(), DriverError>;
    async fn recv_packets(&self, out: &mut [Bytes]) -> Result<usize, DriverError>;
    fn max_packet_payload(&self) -> Option<usize>;
    fn path_snapshot(&self) -> Vec<PathObservation>;
    fn path_events(&self) -> BoxStream<'static, PathObservation>;
    async fn closed(&self) -> DriverError;
    fn close(&self, reason: &'static [u8]);
}
