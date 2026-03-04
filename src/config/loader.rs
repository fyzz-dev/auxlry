use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;

use super::types::Config;

/// Load config from YAML file, interpolating `${ENV_VAR}` patterns.
/// If the config file does not exist, a default one is created.
pub fn load_config(path: &Path) -> Result<Config> {
    if !path.exists() {
        tracing::info!("no config file found at {}, creating default", path.display());
        create_default_config(path)?;
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config: {}", path.display()))?;

    let interpolated = interpolate_env(&raw);

    let config: Config =
        serde_yaml::from_str(&interpolated).context("failed to parse config YAML")?;

    Ok(config)
}

/// Create a default config file with all options documented.
fn create_default_config(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory: {}", parent.display()))?;
    }

    let default_content = r#"# auxlry configuration
# All fields have defaults — only specify what you want to change.
# Environment variables can be used with ${VAR_NAME} syntax.

locale: en

core:
  host: 0.0.0.0
  api_port: 8400
  quic_port: 8401
  stun_servers:
    - stun.l.google.com:19302
  # turn_server: turn.example.com:3478
  # turn_username: user
  # turn_credential: pass

models:
  provider: openrouter
  api_key: ${OPENROUTER_API_KEY}
  interface: anthropic/claude-sonnet-4-20250514
  synapse: anthropic/claude-sonnet-4-20250514
  operator: anthropic/claude-sonnet-4-20250514

# Uncomment and configure adapters to connect to chat platforms:
# interfaces:
#   - name: discord-main
#     adapter:
#       type: discord
#       token: ${DISCORD_TOKEN}
#       channels: []          # empty = all channels
#   - name: telegram-main
#     adapter:
#       type: telegram
#       token: ${TELEGRAM_TOKEN}
#   - name: webhook-main
#     adapter:
#       type: webhook
#       url: https://example.com/hook
#       secret: optional-hmac-secret

nodes:
  - name: local
    mode: workspace            # workspace = sandboxed, system = unrestricted

memory:
  embedding_model: BAAI/bge-small-en-v1.5
  store_path: ~/.auxlry/store/memory

storage:
  database: ~/.auxlry/store/auxlry.db

concurrency:
  max_synapses: 5              # max concurrent thinking tasks
  max_operators: 10            # max concurrent action tasks
  max_synapse_steps: 5         # max LLM rounds per synapse task
  max_operator_steps: 10       # max tool-use rounds per operator task
"#;

    std::fs::write(path, default_content)
        .with_context(|| format!("failed to write default config: {}", path.display()))?;

    tracing::info!("wrote default config to {}", path.display());
    Ok(())
}

/// Replace `${VAR_NAME}` with the corresponding environment variable value.
/// Unknown variables are replaced with empty string.
fn interpolate_env(input: &str) -> String {
    let re = Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").unwrap();
    re.replace_all(input, |caps: &regex::Captures| {
        let var_name = &caps[1];
        std::env::var(var_name).unwrap_or_default()
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_interpolation() {
        // SAFETY: test is single-threaded, no concurrent env access
        unsafe { std::env::set_var("AUXLRY_TEST_KEY", "secret123") };
        let result = interpolate_env("key: ${AUXLRY_TEST_KEY}");
        assert_eq!(result, "key: secret123");
    }

    #[test]
    fn missing_env_becomes_empty() {
        let result = interpolate_env("key: ${AUXLRY_NONEXISTENT_VAR_XYZ}");
        assert_eq!(result, "key: ");
    }

    #[test]
    fn default_config() {
        let config = Config::default();
        assert_eq!(config.core.api_port, 8400);
        assert_eq!(config.locale, "en");
    }
}
