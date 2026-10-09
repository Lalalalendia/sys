use minicbor::{Decode, Encode};
use thiserror::Error;

pub const MAX_CONTROL_FRAME: usize = 64 * 1024;

pub const KIND_CLIENT_HELLO: u16 = 0x0001;
pub const KIND_SERVER_ACCEPT: u16 = 0x0002;
pub const KIND_SERVER_REJECT: u16 = 0x0003;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlFlags(u16);

impl ControlFlags {
    pub const NONE: Self = Self(0);
    pub const CRITICAL: Self = Self(1 << 0);
    pub const RESPONSE_REQUIRED: Self = Self(1 << 1);

    pub const fn bits(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct ControlEnvelope {
    #[n(0)]
    pub kind: u16,
    #[n(1)]
    pub flags: u16,
    #[n(2)]
    pub message_id: u64,
    #[n(3)]
    pub reply_to: Option<u64>,
    #[n(4)]
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct LabClientHello {
    #[n(0)]
    pub protocol_minor: u16,
    #[n(1)]
    pub device_id: [u8; 16],
    #[n(2)]
    pub transport_identity: Vec<u8>,
    #[n(3)]
    pub client_nonce: [u8; 16],
}

#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode)]
#[cbor(map)]
pub struct LabServerAccept {
    #[n(0)]
    pub product_session_id: [u8; 16],
    #[n(1)]
    pub attachment_generation: u32,
    #[n(2)]
    pub server_nonce: [u8; 16],
}

#[derive(Debug, Error)]
pub enum ControlFrameError {
    #[error("control frame is shorter than the 4-byte length prefix")]
    TruncatedPrefix,
    #[error("control frame length {0} exceeds maximum")]
    TooLarge(usize),
    #[error("control frame length prefix {declared} does not match actual payload {actual}")]
    LengthMismatch { declared: usize, actual: usize },
    #[error("CBOR encode failed: {0}")]
    Encode(String),
    #[error("CBOR decode failed: {0}")]
    Decode(String),
}

pub fn encode_control_frame(env: &ControlEnvelope) -> Result<Vec<u8>, ControlFrameError> {
    let payload = minicbor::to_vec(env).map_err(|e| ControlFrameError::Encode(e.to_string()))?;
    if payload.len() > MAX_CONTROL_FRAME {
        return Err(ControlFrameError::TooLarge(payload.len()));
    }
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

pub fn decode_control_frame(frame: &[u8]) -> Result<ControlEnvelope, ControlFrameError> {
    if frame.len() < 4 {
        return Err(ControlFrameError::TruncatedPrefix);
    }
    let declared = u32::from_be_bytes(frame[0..4].try_into().unwrap()) as usize;
    if declared > MAX_CONTROL_FRAME {
        return Err(ControlFrameError::TooLarge(declared));
    }
    let actual = frame.len() - 4;
    if declared != actual {
        return Err(ControlFrameError::LengthMismatch { declared, actual });
    }
    minicbor::decode(&frame[4..]).map_err(|e| ControlFrameError::Decode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_control_frame() {
        let env = ControlEnvelope {
            kind: KIND_CLIENT_HELLO,
            flags: ControlFlags::RESPONSE_REQUIRED.bits(),
            message_id: 9,
            reply_to: None,
            body: vec![1, 2, 3],
        };
        let frame = encode_control_frame(&env).unwrap();
        assert_eq!(decode_control_frame(&frame).unwrap(), env);
    }
}
