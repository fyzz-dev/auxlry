use anyhow::{Context, Result, bail};

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
