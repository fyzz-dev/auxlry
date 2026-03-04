use std::path::PathBuf;

use anyhow::{Context, Result};

/// Resolves all filesystem paths for auxlry data.
#[derive(Debug, Clone)]
pub struct AuxlryPaths {
    pub root: PathBuf,
    pub config_file: PathBuf,
    pub log_file: PathBuf,
    pub core_pid: PathBuf,
    pub database: PathBuf,
    pub token_file: PathBuf,
    pub memory_dir: PathBuf,
    pub workspace_dir: PathBuf,
    pub process_dir: PathBuf,
    pub store_dir: PathBuf,
}

impl AuxlryPaths {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("could not determine home directory")?;
        Self::from_root(home.join(".auxlry"))
    }

    pub fn from_root(root: PathBuf) -> Result<Self> {
        let store_dir = root.join("store");
        let process_dir = root.join("process");
        Ok(Self {
            config_file: root.join("config.yml"),
            log_file: root.join("auxlry.log"),
            core_pid: process_dir.join("core.pid"),
            database: store_dir.join("auxlry.db"),
            token_file: store_dir.join("token"),
            memory_dir: store_dir.join("memory"),
            workspace_dir: root.join("workspace"),
            process_dir,
            store_dir,
            root,
        })
    }

    /// Create all required directories.
    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [
            &self.root,
            &self.store_dir,
            &self.process_dir,
            &self.memory_dir,
            &self.workspace_dir,
        ] {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("failed to create directory: {}", dir.display()))?;
        }
        Ok(())
    }

    /// Returns the PID file path for a named node.
    pub fn node_pid(&self, name: &str) -> PathBuf {
        self.process_dir.join(format!("node-{name}.pid"))
    }

    /// Returns the workspace directory for a named node.
    pub fn node_workspace(&self, name: &str) -> PathBuf {
        self.workspace_dir.join(name)
    }
}

/// Expand `~` at the start of a path string to the user's home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn paths_from_root() {
        let paths = AuxlryPaths::from_root(PathBuf::from("/tmp/auxlry-test")).unwrap();
        assert_eq!(paths.database, Path::new("/tmp/auxlry-test/store/auxlry.db"));
        assert_eq!(
            paths.node_pid("myserver"),
            Path::new("/tmp/auxlry-test/process/node-myserver.pid")
        );
    }

    #[test]
    fn tilde_expansion() {
        let expanded = expand_tilde("~/foo/bar");
        assert!(!expanded.to_string_lossy().starts_with("~"));
    }
}
