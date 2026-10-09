use std::{env, str::FromStr};

use bytes::Bytes;
use connectivity_api::{ConnectivityDriver, DialTarget, PeerTransportIdentity};
use connectivity_iroh::IrohDriver;
use iroh::{Endpoint, EndpointId, RelayMap, RelayMode, RelayUrl, SecretKey, endpoint::presets};
use pe_wire::{
    ControlEnvelope, ControlFlags, KIND_CLIENT_HELLO, KIND_SERVER_ACCEPT, LabClientHello,
    LabServerAccept, MAX_CONTROL_FRAME, PACKET_HEADER_LEN, PacketHeader, decode_control_frame,
    encode_control_frame,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let mut args = env::args().skip(1);
    let exit_id = args
        .next()
        .ok_or("usage: pe-client-lab <exit-endpoint-id> [relay-url]")?;
    let relay_url = args
        .next()
        .unwrap_or_else(|| "http://127.0.0.1:3340".to_string());

    let exit_id = EndpointId::from_str(&exit_id)?;
    let relay = RelayUrl::from_str(&relay_url)?;
    let relay_mode = RelayMode::Custom(RelayMap::from(relay.clone()));

    let secret = SecretKey::generate();
    let endpoint = Endpoint::builder(presets::Minimal)
        .secret_key(secret)
        .relay_mode(relay_mode)
        .clear_ip_transports()
        .bind()
        .await?;
    endpoint.online().await;

    let my_id = endpoint.id();
    println!("client endpoint id: {my_id}");

    let driver = IrohDriver::new(endpoint);
    let conn = driver
        .dial(DialTarget {
            peer: PeerTransportIdentity(exit_id.as_bytes().to_vec()),
            relay_urls: vec![relay.to_string()],
            direct_addrs: vec![],
        })
        .await?;

    let mut control = conn.open_control().await?;
    let hello = LabClientHello {
        protocol_minor: 0,
        device_id: rand::random(),
        transport_identity: my_id.as_bytes().to_vec(),
        client_nonce: rand::random(),
    };
    let body = minicbor::to_vec(hello)?;
    let frame = encode_control_frame(&ControlEnvelope {
        kind: KIND_CLIENT_HELLO,
        flags: ControlFlags::RESPONSE_REQUIRED.bits(),
        message_id: 1,
        reply_to: None,
        body,
    })?;
    control.send.write_all(&frame).await?;
    control.send.flush().await?;

    let response = read_control_frame(&mut control.recv).await?;
    if response.kind != KIND_SERVER_ACCEPT {
        return Err(format!("unexpected control response kind {:#x}", response.kind).into());
    }
    let accepted: LabServerAccept = minicbor::decode(&response.body)?;
    println!(
        "product session accepted: {:02x?}, generation={}",
        accepted.product_session_id, accepted.attachment_generation
    );

    let payload = b"personal-exit-m1-datagram-echo";
    let mut header_bytes = [0u8; PACKET_HEADER_LEN];
    PacketHeader {
        version: PacketHeader::VERSION,
        flags: 0,
        attachment_generation: accepted.attachment_generation,
        packet_seq: 0,
        fragment_index: 0,
        fragment_count: 1,
        original_packet_len: payload.len() as u16,
        fragment_payload_len: payload.len() as u16,
    }
    .encode(&mut header_bytes);

    let mut packet = Vec::with_capacity(PACKET_HEADER_LEN + payload.len());
    packet.extend_from_slice(&header_bytes);
    packet.extend_from_slice(payload);
    conn.send_packet(Bytes::from(packet)).await?;

    let mut batch = vec![Bytes::new(); 8];
    let n = conn.recv_packets(&mut batch).await?;
    if n == 0 {
        return Err("received empty datagram batch".into());
    }
    let (header, echoed) = PacketHeader::decode(&batch[0])?;
    if header.attachment_generation != accepted.attachment_generation || echoed != payload {
        return Err("M1 packet echo mismatch".into());
    }

    println!("M1 PASS: authenticated Iroh peer + control hello + packet datagram echo over relay");
    conn.close(b"m1 complete");
    Ok(())
}

async fn read_control_frame(
    reader: &mut connectivity_api::BoxAsyncRead,
) -> Result<ControlEnvelope, Box<dyn std::error::Error>> {
    let mut prefix = [0u8; 4];
    reader.read_exact(&mut prefix).await?;
    let len = u32::from_be_bytes(prefix) as usize;
    if len > MAX_CONTROL_FRAME {
        return Err(format!("control frame too large: {len}").into());
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await?;
    let mut framed = Vec::with_capacity(4 + len);
    framed.extend_from_slice(&prefix);
    framed.extend_from_slice(&payload);
    Ok(decode_control_frame(&framed)?)
}
