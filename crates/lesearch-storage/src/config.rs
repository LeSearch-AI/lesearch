//! `LeSearch` daemon configuration.
//!
//! Loads from `$LESEARCH_HOME/config.toml` or `~/.lesearch/config.toml`,
//! falling back to sensible defaults when no file exists.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Top-level daemon configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LesearchConfig {
    /// Daemon-level settings.
    pub daemon: DaemonConfig,
    /// Transport mode selection.
    pub transport: TransportConfig,
    /// Resource limits.
    pub limits: LimitsConfig,
    /// Per-provider configuration.
    pub providers: HashMap<String, ProviderConfig>,
    /// A2A gateway settings.
    pub a2a: A2aConfig,
    /// Security settings (bearer token, origin allowlist).
    pub security: SecurityConfig,
}

/// Daemon settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    /// `$LESEARCH_HOME` directory (default `~/.lesearch`).
    pub home: String,
    /// Log level filter (default `"info"`).
    pub log_level: String,
}

/// Transport mode (v0.1.0 only supports `"direct"`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TransportConfig {
    /// One of `"direct"`, `"ziti"`, `"noise-ws"`. v0.1.0: `"direct"` only.
    pub mode: String,
}

/// Resource ceilings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LimitsConfig {
    /// Maximum concurrent agents (default 10).
    pub max_concurrent_agents: u32,
    /// Daemon RSS ceiling in MB (default 500).
    pub max_daemon_memory_mb: u32,
    /// Per-agent RSS ceiling in MB (default 2048).
    pub max_agent_memory_mb: u32,
    /// Days before session log compression (default 90).
    pub session_retention_days: u32,
    /// Disk usage warning threshold in GB (default 20).
    pub storage_warn_gb: u32,
}

/// Single provider entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Path to the provider binary on `$PATH`.
    pub path: String,
    /// Whether this provider is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

const fn default_true() -> bool {
    true
}

/// A2A gateway settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct A2aConfig {
    /// Whether the A2A HTTP surface is enabled.
    pub enabled: bool,
    /// Env var name holding the bearer token.
    pub bearer_token_env: String,
}

/// Security settings for loopback hardening.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    /// Additional allowed origins beyond the built-in loopback set.
    pub additional_origins: Vec<String>,
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

impl Default for LesearchConfig {
    fn default() -> Self {
        Self {
            daemon: DaemonConfig::default(),
            transport: TransportConfig::default(),
            limits: LimitsConfig::default(),
            providers: default_providers(),
            a2a: A2aConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            home: "~/.lesearch".into(),
            log_level: "info".into(),
        }
    }
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            mode: "direct".into(),
        }
    }
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_concurrent_agents: 10,
            max_daemon_memory_mb: 500,
            max_agent_memory_mb: 2048,
            session_retention_days: 90,
            storage_warn_gb: 20,
        }
    }
}

impl Default for A2aConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bearer_token_env: "LESEARCH_A2A_TOKEN".into(),
        }
    }
}

fn default_providers() -> HashMap<String, ProviderConfig> {
    let mut m = HashMap::new();
    m.insert(
        "claude".into(),
        ProviderConfig {
            path: "claude".into(),
            enabled: true,
        },
    );
    m.insert(
        "codex".into(),
        ProviderConfig {
            path: "codex".into(),
            enabled: true,
        },
    );
    m
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

impl LesearchConfig {
    /// Resolve the effective `LESEARCH_HOME` directory.
    ///
    /// Priority: `$LESEARCH_HOME` env var > config value > `~/.lesearch`.
    #[must_use]
    pub fn resolve_home(&self) -> PathBuf {
        if let Ok(env) = std::env::var("LESEARCH_HOME") {
            return PathBuf::from(env);
        }
        let raw = &self.daemon.home;
        if let Some(stripped) = raw.strip_prefix("~/") {
            if let Some(home) = dirs_home() {
                return home.join(stripped);
            }
        }
        PathBuf::from(raw)
    }

    /// Load configuration from a TOML file, falling back to defaults.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Config`] if the file exists but is invalid.
    pub fn load(path: &Path) -> Result<Self, crate::StorageError> {
        if !path.exists() {
            tracing::debug!(?path, "config file not found, using defaults");
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path).map_err(|e| {
            crate::StorageError::Config(format!("reading {}: {e}", path.display()))
        })?;
        toml::from_str(&text).map_err(|e| {
            crate::StorageError::Config(format!("parsing {}: {e}", path.display()))
        })
    }

    /// Load from the default location (`~/.lesearch/config.toml`).
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Config`] on invalid TOML.
    pub fn load_default() -> Result<Self, crate::StorageError> {
        let path = dirs_home()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(super::DEFAULT_HOME)
            .join("config.toml");
        Self::load(&path)
    }
}

/// Best-effort home directory resolution.
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_two_providers() {
        let cfg = LesearchConfig::default();
        assert!(cfg.providers.contains_key("claude"));
        assert!(cfg.providers.contains_key("codex"));
    }

    #[test]
    fn deserialize_minimal_toml() {
        let toml = r#"
[daemon]
log_level = "debug"
"#;
        let cfg: LesearchConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.daemon.log_level, "debug");
        // Limits should be defaults
        assert_eq!(cfg.limits.max_concurrent_agents, 10);
    }

    #[test]
    fn deserialize_empty_toml() {
        let cfg: LesearchConfig = toml::from_str("").unwrap();
        assert_eq!(cfg.daemon.home, "~/.lesearch");
    }

    #[test]
    fn resolve_home_with_tilde() {
        let cfg = LesearchConfig::default();
        let home = cfg.resolve_home();
        // Should not literally contain ~
        assert!(!home.to_string_lossy().contains('~'));
    }
}
