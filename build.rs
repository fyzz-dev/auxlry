use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    let dist = Path::new("dashboard/dist");
    let node_modules = Path::new("dashboard/node_modules");

    // Auto-build frontend if dist/ doesn't exist but node_modules/ does
    if !dist.exists() && node_modules.exists() {
        if std::env::var("AUXLRY_SKIP_FRONTEND_BUILD").is_ok() {
            eprintln!("cargo:warning=Skipping frontend build (AUXLRY_SKIP_FRONTEND_BUILD set)");
        } else if let Ok(output) = Command::new("bun")
            .args(["run", "build"])
            .current_dir("dashboard")
            .output()
        {
            if !output.status.success() {
                eprintln!(
                    "cargo:warning=Frontend build failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        } else {
            eprintln!("cargo:warning=bun not found, skipping frontend build");
        }
    }

    // Ensure dashboard/dist/index.html exists so rust-embed compiles
    // even when the frontend hasn't been built yet.
    if !dist.exists() {
        fs::create_dir_all(dist).expect("failed to create dashboard/dist");
        fs::write(
            dist.join("index.html"),
            "<!doctype html><html><body>run `bun run build` in dashboard/</body></html>",
        )
        .expect("failed to write placeholder index.html");
    }

    println!("cargo:rerun-if-changed=dashboard/src");
    println!("cargo:rerun-if-changed=dashboard/index.html");
    println!("cargo:rerun-if-changed=dashboard/vite.config.ts");
}
