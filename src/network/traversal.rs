use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{info, warn};

use super::hole_punch::stun_discover;
use super::nat;
use super::turn::{TurnClient, TurnRelay};
use crate::config::types::CoreConfig;

/// Result of a NAT traversal attempt.
#[derive(Debug)]
pub enum TraversalResult {
    /// Direct connection succeeded.
    Direct(SocketAddr),
    /// Hole-punched connection via STUN-discovered address.
    HolePunched(SocketAddr),
    /// Relayed through a TURN server.
    Relayed(TurnRelay),
}

impl TraversalResult {
    /// Get the effective address to connect to.
    pub fn addr(&self) -> SocketAddr {
        match self {
            Self::Direct(a) => *a,
            Self::HolePunched(a) => *a,
            Self::Relayed(r) => r.relay_addr,
        }
    }
}

/// Attempt to establish connectivity to a target using a cascading strategy:
/// 1. Direct connection (3s timeout)
/// 2. STUN hole-punch
/// 3. TURN relay (if configured)
pub async fn establish_connection(
    target: SocketAddr,
    config: &CoreConfig,
) -> Result<TraversalResult> {
    // Strategy 1: Try direct connection
    info!(%target, "attempting direct connection");
    match try_direct(target, Duration::from_secs(3)).await {
        Ok(addr) => {
            info!(%addr, "direct connection succeeded");
            return Ok(TraversalResult::Direct(addr));
        }
        Err(e) => {
            warn!(error = %e, "direct connection failed, trying STUN hole-punch");
        }
    }

    // Strategy 2: STUN hole-punch
    if !config.stun_servers.is_empty() {
        match try_hole_punch(target, &config.stun_servers).await {
            Ok(addr) => {
                info!(%addr, "hole-punch succeeded");
                return Ok(TraversalResult::HolePunched(addr));
            }
            Err(e) => {
                warn!(error = %e, "hole-punch failed");
            }
        }
    }

    // Strategy 3: TURN relay
    if let (Some(server), Some(username), Some(credential)) = (
        &config.turn_server,
        &config.turn_username,
        &config.turn_credential,
    ) {
        info!("attempting TURN relay allocation");
        let client = TurnClient::new(server, username, credential).await?;
        let relay = client.allocate().await.context("TURN allocation failed")?;
        info!(relay_addr = %relay.relay_addr, "TURN relay allocated");
        return Ok(TraversalResult::Relayed(relay));
    }

    anyhow::bail!(
        "all NAT traversal strategies failed for {target} — \
        configure a TURN server for symmetric NAT environments"
    )
}

/// Detect NAT type and public address.
pub async fn detect_and_log_nat(config: &CoreConfig) -> Option<SocketAddr> {
    if config.stun_servers.is_empty() {
        return None;
    }

    match nat::detect_nat_type(&config.stun_servers).await {
        Ok((nat_type, public_addr)) => {
            info!(?nat_type, %public_addr, "NAT detection complete");
            Some(public_addr)
        }
        Err(e) => {
            warn!(error = %e, "NAT detection failed");
            None
        }
    }
}

/// Try a direct UDP probe to the target.
async fn try_direct(target: SocketAddr, timeout: Duration) -> Result<SocketAddr> {
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
    socket.send_to(b"probe", target).await?;

    let mut buf = [0u8; 64];
    tokio::time::timeout(timeout, socket.recv_from(&mut buf))
        .await
        .context("direct probe timeout")?
        .context("direct probe failed")?;

    Ok(target)
}

/// Try STUN-based hole punching.
async fn try_hole_punch(target: SocketAddr, stun_servers: &[String]) -> Result<SocketAddr> {
    // Discover our public address
    let _public = stun_discover(&stun_servers[0]).await?;

    // In a real implementation, both peers would exchange their STUN-discovered
    // addresses via a signaling channel, then send simultaneous probes.
    // For now, we use the STUN-discovered address as a candidate.
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;

    // Send probe packets to punch through NAT
    for _ in 0..3 {
        socket.send_to(b"punch", target).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Wait for response
    let mut buf = [0u8; 64];
    tokio::time::timeout(Duration::from_secs(3), socket.recv_from(&mut buf))
        .await
        .context("hole-punch timeout")?
        .context("hole-punch failed")?;

    Ok(target)
}
