use std::{env, str::FromStr};

use bytes::Bytes;
use connectivity_api::{ConnectivityDriver, TransportConnection};
use connectivity_iroh::{IrohDriver, PERSONAL_EXIT_ALPN};
use iroh::{Endpoint, RelayMap, RelayMode, RelayUrl, SecretKey, endpoint::presets};
use pe_wire::{
    ControlEnvelope, ControlFlags, KIND_CLIENT_HELLO, KIND_SERVER_ACCEPT, LabClientHello,
    LabServerAccept, MAX_CONTROL_FRAME, decode_control_frame, encode_control_frame,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let relay_url = env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:3340".to_string());
    let relay = RelayUrl::from_str(&relay_url)?;
    let relay_mode = RelayMode::Custom(RelayMap::from(relay));

    let secret = SecretKey::generate();
    let endpoint = Endpoint::builder(presets::Minimal)
        .secret_key(secret)
        .alpns(vec![PERSONAL_EXIT_ALPN.to_vec()])
        .relay_mode(relay_mode)
        .clear_ip_transports()
        .bind()
        .await?;
    endpoint.online().await;
    println!("exit endpoint id: {}", endpoint.id());

    let driver = IrohDriver::new(endpoint);
    let conn = driver.accept().await?;
    run_one_session(conn).await
}

async fn run_one_session(
    conn: Box<dyn TransportConnection>,
) -> Result<(), Box<dyn std::error::Error>> {
    let peer = conn.peer_identity();
    let mut control = conn.accept_control().await?;
    let request = read_control_frame(&mut control.recv).await?;
    if request.kind != KIND_CLIENT_HELLO {
        return Err(format!("expected ClientHello, got kind {:#x}", request.kind).into());
    }
    let hello: LabClientHello = minicbor::decode(&request.body)?;
    if hello.transport_identity != peer.0 {
        return Err("ClientHello transport identity does not match authenticated Iroh peer".into());
    }

    let accepted = LabServerAccept {
        product_session_id: rand::random(),
        attachment_generation: 1,
        server_nonce: rand::random(),
    };
    let body = minicbor::to_vec(accepted)?;
    let response = encode_control_frame(&ControlEnvelope {
        kind: KIND_SERVER_ACCEPT,
        flags: ControlFlags::NONE.bits(),
        message_id: 2,
        reply_to: Some(request.message_id),
        body,
    })?;
    control.send.write_all(&response).await?;
    control.send.flush().await?;

    let mut batch = vec![Bytes::new(); 32];
    loop {
        let n = match conn.recv_packets(&mut batch).await {
            Ok(n) => n,
            Err(_) => return Ok(()),
        };
        for item in batch.iter().take(n) {
            conn.send_packet(item.clone()).await?;
        }
    }
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
