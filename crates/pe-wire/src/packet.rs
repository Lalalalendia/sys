use thiserror::Error;

pub const PACKET_HEADER_LEN: usize = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PacketHeader {
    pub version: u8,
    pub flags: u8,
    pub attachment_generation: u32,
    pub packet_seq: u64,
    pub fragment_index: u16,
    pub fragment_count: u16,
    pub original_packet_len: u16,
    pub fragment_payload_len: u16,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum PacketWireError {
    #[error("datagram shorter than packet header")]
    Truncated,
    #[error("unsupported packet wire version {0}")]
    UnsupportedVersion(u8),
    #[error("invalid header length {0}")]
    InvalidHeaderLength(u16),
    #[error("reserved packet flags are set: {0:#04x}")]
    ReservedFlags(u8),
    #[error("fragment count must be non-zero")]
    EmptyFragmentSet,
    #[error("fragment index {index} out of range for count {count}")]
    FragmentIndex { index: u16, count: u16 },
    #[error("payload length mismatch: header={header}, actual={actual}")]
    PayloadLength { header: u16, actual: usize },
    #[error("fragment metadata exceeds original packet length")]
    InvalidOriginalLength,
}

impl PacketHeader {
    pub const VERSION: u8 = 0;
    pub const ALLOWED_FLAGS: u8 = 0;

    pub fn encode(self, out: &mut [u8; PACKET_HEADER_LEN]) {
        out[0] = self.version;
        out[1] = self.flags;
        out[2..4].copy_from_slice(&(PACKET_HEADER_LEN as u16).to_be_bytes());
        out[4..8].copy_from_slice(&self.attachment_generation.to_be_bytes());
        out[8..16].copy_from_slice(&self.packet_seq.to_be_bytes());
        out[16..18].copy_from_slice(&self.fragment_index.to_be_bytes());
        out[18..20].copy_from_slice(&self.fragment_count.to_be_bytes());
        out[20..22].copy_from_slice(&self.original_packet_len.to_be_bytes());
        out[22..24].copy_from_slice(&self.fragment_payload_len.to_be_bytes());
    }

    pub fn decode(datagram: &[u8]) -> Result<(Self, &[u8]), PacketWireError> {
        if datagram.len() < PACKET_HEADER_LEN {
            return Err(PacketWireError::Truncated);
        }
        let version = datagram[0];
        if version != Self::VERSION {
            return Err(PacketWireError::UnsupportedVersion(version));
        }
        let flags = datagram[1];
        if flags & !Self::ALLOWED_FLAGS != 0 {
            return Err(PacketWireError::ReservedFlags(flags));
        }
        let header_len = u16::from_be_bytes([datagram[2], datagram[3]]);
        if header_len as usize != PACKET_HEADER_LEN {
            return Err(PacketWireError::InvalidHeaderLength(header_len));
        }

        let header = Self {
            version,
            flags,
            attachment_generation: u32::from_be_bytes(datagram[4..8].try_into().unwrap()),
            packet_seq: u64::from_be_bytes(datagram[8..16].try_into().unwrap()),
            fragment_index: u16::from_be_bytes(datagram[16..18].try_into().unwrap()),
            fragment_count: u16::from_be_bytes(datagram[18..20].try_into().unwrap()),
            original_packet_len: u16::from_be_bytes(datagram[20..22].try_into().unwrap()),
            fragment_payload_len: u16::from_be_bytes(datagram[22..24].try_into().unwrap()),
        };

        if header.fragment_count == 0 {
            return Err(PacketWireError::EmptyFragmentSet);
        }
        if header.fragment_index >= header.fragment_count {
            return Err(PacketWireError::FragmentIndex {
                index: header.fragment_index,
                count: header.fragment_count,
            });
        }

        let payload = &datagram[PACKET_HEADER_LEN..];
        if payload.len() != header.fragment_payload_len as usize {
            return Err(PacketWireError::PayloadLength {
                header: header.fragment_payload_len,
                actual: payload.len(),
            });
        }
        if header.original_packet_len == 0
            || header.fragment_payload_len > header.original_packet_len
        {
            return Err(PacketWireError::InvalidOriginalLength);
        }

        Ok((header, payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_header() {
        let h = PacketHeader {
            version: 0,
            flags: 0,
            attachment_generation: 7,
            packet_seq: 42,
            fragment_index: 1,
            fragment_count: 3,
            original_packet_len: 1500,
            fragment_payload_len: 500,
        };
        let mut buf = [0u8; PACKET_HEADER_LEN];
        h.encode(&mut buf);
        let mut datagram = buf.to_vec();
        datagram.extend(std::iter::repeat_n(0x55, 500));
        let (decoded, payload) = PacketHeader::decode(&datagram).unwrap();
        assert_eq!(decoded, h);
        assert_eq!(payload.len(), 500);
    }

    #[test]
    fn rejects_reserved_flags() {
        let mut buf = [0u8; PACKET_HEADER_LEN];
        PacketHeader {
            version: 0,
            flags: 0x80,
            attachment_generation: 1,
            packet_seq: 1,
            fragment_index: 0,
            fragment_count: 1,
            original_packet_len: 1,
            fragment_payload_len: 1,
        }
        .encode(&mut buf);
        let mut datagram = buf.to_vec();
        datagram.push(1);
        assert_eq!(
            PacketHeader::decode(&datagram),
            Err(PacketWireError::ReservedFlags(0x80))
        );
    }
}
