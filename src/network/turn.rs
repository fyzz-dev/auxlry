use std::net::SocketAddr;

use anyhow::{Context, Result};
use tokio::net::UdpSocket;

/// A TURN relay allocation.
#[derive(Debug, Clone)]
pub struct TurnRelay {
    pub relay_addr: SocketAddr,
    pub server_addr: SocketAddr,
}

/// Minimal TURN client implementing RFC 5766 Allocate.
pub struct TurnClient {
    socket: UdpSocket,
    server_addr: SocketAddr,
    username: String,
    credential: String,
}

// STUN/TURN message types
const ALLOCATE_REQUEST: u16 = 0x0003;
const ALLOCATE_RESPONSE: u16 = 0x0103;
const ALLOCATE_ERROR: u16 = 0x0113;
const MAGIC_COOKIE: u32 = 0x2112A442;

// Attribute types
const ATTR_XOR_RELAYED_ADDRESS: u16 = 0x0016;
const ATTR_REQUESTED_TRANSPORT: u16 = 0x0019;
const ATTR_USERNAME: u16 = 0x0006;
const ATTR_REALM: u16 = 0x0014;
const ATTR_NONCE: u16 = 0x0015;
const ATTR_MESSAGE_INTEGRITY: u16 = 0x0008;
const _ATTR_ERROR_CODE: u16 = 0x0009;

impl TurnClient {
    /// Create a new TURN client.
    pub async fn new(
        server: &str,
        username: &str,
        credential: &str,
    ) -> Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .context("failed to bind UDP socket")?;

        let server_addr: SocketAddr = tokio::net::lookup_host(server)
            .await
            .context("TURN DNS lookup failed")?
            .next()
            .context("no TURN server address resolved")?;

        Ok(Self {
            socket,
            server_addr,
            username: username.to_string(),
            credential: credential.to_string(),
        })
    }

    /// Request a TURN allocation (relay address).
    pub async fn allocate(&self) -> Result<TurnRelay> {
        // Build Allocate request
        let tx_id: [u8; 12] = {
            use rand::Rng;
            rand::thread_rng().r#gen()
        };

        // First attempt without auth (will get 401 with realm+nonce)
        let request = self.build_allocate_request(&tx_id, None);
        self.socket.send_to(&request, self.server_addr).await?;

        let mut buf = vec![0u8; 1024];
        let (len, _) = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.socket.recv_from(&mut buf),
        )
        .await
        .context("TURN allocate timeout")?
        .context("failed to receive TURN response")?;

        let response = &buf[..len];

        // Check if we got a 401 (need auth)
        if self.get_message_type(response) == ALLOCATE_ERROR {
            let (realm, nonce) = self.extract_auth_challenge(response)?;

            // Second attempt with auth
            let tx_id2: [u8; 12] = {
                use rand::Rng;
                rand::thread_rng().r#gen()
            };

            let auth_request = self.build_allocate_request(
                &tx_id2,
                Some((&realm, &nonce)),
            );
            self.socket.send_to(&auth_request, self.server_addr).await?;

            let (len2, _) = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                self.socket.recv_from(&mut buf),
            )
            .await
            .context("TURN authenticated allocate timeout")?
            .context("failed to receive authenticated TURN response")?;

            let response2 = &buf[..len2];
            if self.get_message_type(response2) != ALLOCATE_RESPONSE {
                anyhow::bail!("TURN allocation failed after authentication");
            }

            return self.parse_allocate_response(response2);
        }

        if self.get_message_type(response) == ALLOCATE_RESPONSE {
            return self.parse_allocate_response(response);
        }

        anyhow::bail!("unexpected TURN response type")
    }

    fn get_message_type(&self, msg: &[u8]) -> u16 {
        if msg.len() >= 2 {
            u16::from_be_bytes([msg[0], msg[1]])
        } else {
            0
        }
    }

    fn build_allocate_request(&self, tx_id: &[u8; 12], auth: Option<(&str, &str)>) -> Vec<u8> {
        let mut attrs = Vec::new();

        // REQUESTED-TRANSPORT: UDP (17)
        let transport_value = [0x11, 0x00, 0x00, 0x00]; // protocol 17 (UDP) + 3 bytes reserved
        attrs.extend_from_slice(&ATTR_REQUESTED_TRANSPORT.to_be_bytes());
        attrs.extend_from_slice(&4u16.to_be_bytes());
        attrs.extend_from_slice(&transport_value);

        if let Some((realm, nonce)) = auth {
            // USERNAME
            let username_bytes = self.username.as_bytes();
            let username_padded_len = (username_bytes.len() + 3) & !3;
            attrs.extend_from_slice(&ATTR_USERNAME.to_be_bytes());
            attrs.extend_from_slice(&(username_bytes.len() as u16).to_be_bytes());
            attrs.extend_from_slice(username_bytes);
            attrs.resize(attrs.len() + username_padded_len - username_bytes.len(), 0);

            // REALM
            let realm_bytes = realm.as_bytes();
            let realm_padded_len = (realm_bytes.len() + 3) & !3;
            attrs.extend_from_slice(&ATTR_REALM.to_be_bytes());
            attrs.extend_from_slice(&(realm_bytes.len() as u16).to_be_bytes());
            attrs.extend_from_slice(realm_bytes);
            attrs.resize(attrs.len() + realm_padded_len - realm_bytes.len(), 0);

            // NONCE
            let nonce_bytes = nonce.as_bytes();
            let nonce_padded_len = (nonce_bytes.len() + 3) & !3;
            attrs.extend_from_slice(&ATTR_NONCE.to_be_bytes());
            attrs.extend_from_slice(&(nonce_bytes.len() as u16).to_be_bytes());
            attrs.extend_from_slice(nonce_bytes);
            attrs.resize(attrs.len() + nonce_padded_len - nonce_bytes.len(), 0);

            // MESSAGE-INTEGRITY (HMAC-SHA1) — simplified: compute over header+attrs
            // The key is MD5(username:realm:password) per RFC 5389
            let key_input = format!("{}:{}:{}", self.username, realm, self.credential);
            let key = md5::compute(key_input.as_bytes());

            // Build temporary message for HMAC computation
            let integrity_msg_len = (attrs.len() + 4 + 20) as u16; // +4 for integrity attr header, +20 for HMAC
            let mut temp_msg = Vec::new();
            temp_msg.extend_from_slice(&ALLOCATE_REQUEST.to_be_bytes());
            temp_msg.extend_from_slice(&integrity_msg_len.to_be_bytes());
            temp_msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
            temp_msg.extend_from_slice(tx_id);
            temp_msg.extend_from_slice(&attrs);

            let hmac = hmac_sha1(&key.0, &temp_msg);
            attrs.extend_from_slice(&ATTR_MESSAGE_INTEGRITY.to_be_bytes());
            attrs.extend_from_slice(&20u16.to_be_bytes());
            attrs.extend_from_slice(&hmac);
        }

        // Build header
        let mut msg = Vec::new();
        msg.extend_from_slice(&ALLOCATE_REQUEST.to_be_bytes());
        msg.extend_from_slice(&(attrs.len() as u16).to_be_bytes());
        msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        msg.extend_from_slice(tx_id);
        msg.extend_from_slice(&attrs);
        msg
    }

    fn extract_auth_challenge(&self, response: &[u8]) -> Result<(String, String)> {
        let mut realm = None;
        let mut nonce = None;

        let mut pos = 20; // Skip header
        while pos + 4 <= response.len() {
            let attr_type = u16::from_be_bytes([response[pos], response[pos + 1]]);
            let attr_len = u16::from_be_bytes([response[pos + 2], response[pos + 3]]) as usize;
            pos += 4;

            if pos + attr_len > response.len() {
                break;
            }

            match attr_type {
                ATTR_REALM => {
                    realm = Some(String::from_utf8_lossy(&response[pos..pos + attr_len]).to_string());
                }
                ATTR_NONCE => {
                    nonce = Some(String::from_utf8_lossy(&response[pos..pos + attr_len]).to_string());
                }
                _ => {}
            }

            pos += (attr_len + 3) & !3;
        }

        Ok((
            realm.context("no realm in 401 response")?,
            nonce.context("no nonce in 401 response")?,
        ))
    }

    fn parse_allocate_response(&self, response: &[u8]) -> Result<TurnRelay> {
        let mut pos = 20; // Skip header
        while pos + 4 <= response.len() {
            let attr_type = u16::from_be_bytes([response[pos], response[pos + 1]]);
            let attr_len = u16::from_be_bytes([response[pos + 2], response[pos + 3]]) as usize;
            pos += 4;

            if attr_type == ATTR_XOR_RELAYED_ADDRESS && attr_len >= 8 && pos + attr_len <= response.len() {
                let family = response[pos + 1];
                let port = u16::from_be_bytes([response[pos + 2], response[pos + 3]]) ^ 0x2112;

                if family == 0x01 {
                    // IPv4
                    let ip = [
                        response[pos + 4] ^ 0x21,
                        response[pos + 5] ^ 0x12,
                        response[pos + 6] ^ 0xA4,
                        response[pos + 7] ^ 0x42,
                    ];
                    let relay_addr = SocketAddr::from((ip, port));
                    return Ok(TurnRelay {
                        relay_addr,
                        server_addr: self.server_addr,
                    });
                }
            }

            pos += (attr_len + 3) & !3;
        }

        anyhow::bail!("XOR-RELAYED-ADDRESS not found in Allocate response")
    }
}

/// Simple HMAC-SHA1 implementation.
fn hmac_sha1(key: &[u8], message: &[u8]) -> [u8; 20] {
    use sha1::{Digest, Sha1};

    let mut padded_key = [0u8; 64];
    if key.len() > 64 {
        let hash = Sha1::digest(key);
        padded_key[..20].copy_from_slice(&hash);
    } else {
        padded_key[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= padded_key[i];
        opad[i] ^= padded_key[i];
    }

    let mut inner = Sha1::new();
    inner.update(&ipad);
    inner.update(message);
    let inner_hash = inner.finalize();

    let mut outer = Sha1::new();
    outer.update(&opad);
    outer.update(&inner_hash);
    let result = outer.finalize();

    let mut out = [0u8; 20];
    out.copy_from_slice(&result);
    out
}
