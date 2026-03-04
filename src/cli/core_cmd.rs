use anyhow::{Context, Result, bail};

use crate::config::loader::load_config;
use crate::node::linking::generate_link_code;
use crate::storage::database::Database;
use crate::storage::paths::AuxlryPaths;

/// Start the core daemon.
pub async fn start(foreground: bool) -> Result<()> {
    let paths = AuxlryPaths::new()?;

    if let Some(pid) = read_pid(&paths)? {
        if is_process_running(pid) {
            bail!("core is already running (PID {pid})");
        }
        // Stale PID file — remove it
        let _ = std::fs::remove_file(&paths.core_pid);
    }

    if foreground {
        crate::core::daemon::run(paths).await
    } else {
        // Spawn as background process
        let exe = std::env::current_exe().context("failed to get executable path")?;
        let child = std::process::Command::new(exe)
            .args(["core", "start", "--foreground"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .stdin(std::process::Stdio::null())
            .spawn()
            .context("failed to spawn daemon")?;

        println!("auxlry core started (PID {})", child.id());
        Ok(())
    }
}

/// Stop the core daemon.
pub async fn stop() -> Result<()> {
    let paths = AuxlryPaths::new()?;

    let pid = read_pid(&paths)?
        .context("core is not running (no PID file)")?;

    if !is_process_running(pid) {
        let _ = std::fs::remove_file(&paths.core_pid);
        bail!("core is not running (stale PID file removed)");
    }

    // Send SIGTERM
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }

    #[cfg(not(unix))]
    bail!("stop is only supported on Unix");

    // Wait briefly for graceful shutdown
    for _ in 0..30 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if !is_process_running(pid) {
            break;
        }
    }

    if paths.core_pid.exists() {
        let _ = std::fs::remove_file(&paths.core_pid);
    }
    println!("auxlry core stopped");
    Ok(())
}

/// Show core daemon status.
pub async fn status() -> Result<()> {
    let paths = AuxlryPaths::new()?;

    match read_pid(&paths)? {
        Some(pid) if is_process_running(pid) => {
            println!("auxlry core is running (PID {pid})");
            println!("  API: http://{}:{}", "localhost", 8400);
        }
        Some(_pid) => {
            let _ = std::fs::remove_file(&paths.core_pid);
            println!("auxlry core is not running (stale PID cleaned up)");
        }
        None => {
            println!("auxlry core is not running");
        }
    }
    Ok(())
}

/// Restart the core daemon.
pub async fn restart(foreground: bool) -> Result<()> {
    let paths = AuxlryPaths::new()?;
    if let Some(pid) = read_pid(&paths)? {
        if is_process_running(pid) {
            stop().await?;
        }
    }
    start(foreground).await
}

/// Generate a one-time link code and print address + code.
pub async fn link() -> Result<()> {
    let paths = AuxlryPaths::new()?;

    // Verify core is running
    let pid = read_pid(&paths)?.context("core is not running — start it first")?;
    if !is_process_running(pid) {
        let _ = std::fs::remove_file(&paths.core_pid);
        bail!("core is not running (stale PID file removed)");
    }

    // Open the same database the daemon uses
    let db = Database::open(&paths.database.to_string_lossy()).await?;

    // Generate and store one-time code
    let code = generate_link_code();
    db.store_pending_code(&code).await?;

    // Read config to get quic_port
    let config = load_config(&paths.config_file)?;
    let quic_port = config.core.quic_port;

    // Determine local IP (UDP socket trick — doesn't actually send anything)
    let local_ip = get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());

    println!("link code generated — give this to the remote node:\n");
    println!("  auxlry node link {local_ip}:{quic_port} {code}");
    println!();

    Ok(())
}

/// Get the machine's local IP by binding a UDP socket toward a public address.
fn get_local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

fn read_pid(paths: &AuxlryPaths) -> Result<Option<u32>> {
    if !paths.core_pid.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&paths.core_pid)?;
    let pid: u32 = content.trim().parse().context("invalid PID file")?;
    Ok(Some(pid))
}

fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // Signal 0 checks if process exists
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}
