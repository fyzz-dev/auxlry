use std::net::SocketAddr;

use anyhow::{Context, Result};
use tokio::net::UdpSocket;

/// Discover our public address using a STUN server.
pub async fn stun_discover(stun_server: &str) -> Result<SocketAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .context("failed to bind UDP socket")?;

    // Simple STUN binding request (RFC 5389)
    // Type: 0x0001 (Binding Request), Length: 0, Magic: 0x2112A442, Transaction ID: 12 random bytes
    let mut request = vec![
        0x00, 0x01, // Type: Binding Request
        0x00, 0x00, // Length: 0
        0x21, 0x12, 0xA4, 0x42, // Magic Cookie
    ];
    // Transaction ID (12 bytes)
    let tx_id: [u8; 12] = {
        use rand::Rng;
        rand::thread_rng().r#gen()
    };
    request.extend_from_slice(&tx_id);

    let stun_addr: SocketAddr = tokio::net::lookup_host(stun_server)
        .await
        .context("STUN DNS lookup failed")?
        .next()
        .context("no STUN server address resolved")?;

    socket
        .send_to(&request, stun_addr)
        .await
        .context("failed to send STUN request")?;

    let mut buf = vec![0u8; 512];
    let timeout = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        socket.recv_from(&mut buf),
    )
    .await
    .context("STUN response timeout")?
    .context("failed to receive STUN response")?;

    let (len, _from) = timeout;
    let response = &buf[..len];

    // Parse XOR-MAPPED-ADDRESS from STUN response
    parse_xor_mapped_address(response).context("failed to parse STUN response")
}

fn parse_xor_mapped_address(response: &[u8]) -> Result<SocketAddr> {
    if response.len() < 20 {
        anyhow::bail!("STUN response too short");
    }

    // Skip header (20 bytes), iterate attributes
    let mut pos = 20;
    while pos + 4 <= response.len() {
        let attr_type = u16::from_be_bytes([response[pos], response[pos + 1]]);
        let attr_len = u16::from_be_bytes([response[pos + 2], response[pos + 3]]) as usize;
        pos += 4;

        if attr_type == 0x0020 {
            // XOR-MAPPED-ADDRESS
            if attr_len >= 8 && pos + attr_len <= response.len() {
                let family = response[pos + 1];
                let port =
                    u16::from_be_bytes([response[pos + 2], response[pos + 3]]) ^ 0x2112;

                if family == 0x01 {
                    // IPv4
                    let ip = [
                        response[pos + 4] ^ 0x21,
                        response[pos + 5] ^ 0x12,
                        response[pos + 6] ^ 0xA4,
                        response[pos + 7] ^ 0x42,
                    ];
                    return Ok(SocketAddr::from((ip, port)));
                }
            }
        }

        // Align to 4-byte boundary
        pos += (attr_len + 3) & !3;
    }

    anyhow::bail!("XOR-MAPPED-ADDRESS not found in STUN response")
}
