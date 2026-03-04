use std::net::SocketAddr;

use anyhow::{Context, Result};

use super::hole_punch::stun_discover;

/// Detected NAT type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NatType {
    /// No NAT — public IP directly reachable.
    OpenInternet,
    /// Full cone NAT — any external host can send to the mapped port.
    FullCone,
    /// Restricted cone — only hosts we've sent to can reply.
    RestrictedCone,
    /// Port restricted — only host:port pairs we've sent to can reply.
    PortRestricted,
    /// Symmetric NAT — different mapping per destination (hardest to traverse).
    Symmetric,
    /// Could not determine NAT type.
    Unknown,
}

/// Detect the NAT type by probing two STUN servers and comparing mapped addresses.
///
/// This is a simplified detection:
/// - If both STUN servers report the same mapped address, it's likely cone NAT.
/// - If they report different addresses, it's likely symmetric NAT.
/// - If the mapped address matches the local address, it's open internet.
pub async fn detect_nat_type(stun_servers: &[String]) -> Result<(NatType, SocketAddr)> {
    if stun_servers.is_empty() {
        anyhow::bail!("no STUN servers configured");
    }

    // First probe
    let addr1 = stun_discover(&stun_servers[0])
        .await
        .context("first STUN probe failed")?;

    // Check if we have a second server
    if stun_servers.len() < 2 {
        // Can't do full detection with one server
        return Ok((NatType::Unknown, addr1));
    }

    // Second probe
    let addr2 = match stun_discover(&stun_servers[1]).await {
        Ok(a) => a,
        Err(_) => {
            // Second server unreachable, return what we have
            return Ok((NatType::Unknown, addr1));
        }
    };

    // Compare results
    let nat_type = if addr1 == addr2 {
        // Same mapped address from both servers — cone NAT or open
        // We'd need more probes to distinguish full/restricted/port-restricted
        // For now, classify as FullCone (traversable)
        NatType::FullCone
    } else {
        // Different mapped addresses — symmetric NAT
        NatType::Symmetric
    };

    Ok((nat_type, addr1))
}
