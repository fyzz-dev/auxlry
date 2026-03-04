use anyhow::{Context, Result};
use quinn::{RecvStream, SendStream};

use crate::node::protocol::ProtocolMessage;

/// Send a protocol message over a QUIC stream.
pub async fn send_message(send: &mut SendStream, msg: &ProtocolMessage) -> Result<()> {
    let data = bincode::serde::encode_to_vec(msg, bincode::config::standard())
        .context("failed to encode message")?;

    // Length-prefixed framing
    let len = (data.len() as u32).to_be_bytes();
    send.write_all(&len)
        .await
        .context("failed to write message length")?;
    send.write_all(&data)
        .await
        .context("failed to write message data")?;

    Ok(())
}

/// Receive a protocol message from a QUIC stream.
pub async fn recv_message(recv: &mut RecvStream) -> Result<ProtocolMessage> {
    // Read length prefix
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf)
        .await
        .context("failed to read message length")?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > 64 * 1024 * 1024 {
        anyhow::bail!("message too large: {len} bytes");
    }

    // Read message body
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf)
        .await
        .context("failed to read message data")?;

    let (msg, _) = bincode::serde::decode_from_slice(&buf, bincode::config::standard())
        .context("failed to decode message")?;

    Ok(msg)
}
