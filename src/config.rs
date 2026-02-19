use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default)]
    pub tier: Tier,

    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,

    // This is runtime-derived, not serialized
    #[serde(skip)]
    pub embedding_dim: usize,

    // Cloud settings
    #[serde(default = "default_cloud_url")]
    pub cloud_url: String,

    #[serde(default)]
    pub api_key: Option<String>,

    #[serde(default)]
    pub device_id: Option<String>,

    #[serde(default = "default_sync_interval")]
    pub sync_interval_secs: u64,

    #[serde(default = "default_auto_sync")]
    pub auto_sync: bool,

    /// Run periodic background consolidation passes (Pro tier)
    #[serde(default = "default_auto_consolidation")]
    pub auto_consolidation: bool,

    /// Background consolidation interval in seconds (default: 6h)
    #[serde(default = "default_consolidation_interval")]
    pub consolidation_interval_secs: u64,

    // Zero-knowledge encryption
    #[serde(default)]
    pub email: Option<String>,

    #[serde(default)]
    pub pin_verifier: Option<String>,

    /// Server-provided random salt for key derivation (hex)
    #[serde(default)]
    pub key_salt: Option<String>,

    /// Cached derived key (hex-encoded), cleared after 30 days
    #[serde(default)]
    pub cached_key: Option<String>,

    /// When the key was cached (ISO 8601)
    #[serde(default)]
    pub key_cached_at: Option<String>,

    /// Remote daemon URL — if set, this instance is a client that connects
    /// to an existing daemon instead of running its own.
    #[serde(default)]
    pub remote_daemon_url: Option<String>,

    /// Cloud-signed capability token for tier enforcement
    #[serde(default)]
    pub capability_token: Option<String>,

    /// Bearer token for localhost API authentication.
    /// Generated on first `init`, required for all non-health routes.
    #[serde(default)]
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    #[default]
    Free,
    Standard,
    Pro,
}

impl Tier {
    pub fn max_memories(&self) -> Option<usize> {
        match self {
            Tier::Free => Some(100),
            Tier::Standard => None, // Unlimited
            Tier::Pro => None,      // Unlimited
        }
    }

    pub fn semantic_search_enabled(&self) -> bool {
        true // Always on — it's the core product
    }

    #[allow(dead_code)]
    pub fn max_devices(&self) -> Option<usize> {
        match self {
            Tier::Free => Some(1),
            Tier::Standard => Some(3),
            Tier::Pro => None, // Unlimited
        }
    }

    pub fn cloud_sync_enabled(&self) -> bool {
        match self {
            Tier::Free => false,
            Tier::Standard => true,
            Tier::Pro => true,
        }
    }

    pub fn context_synthesis_enabled(&self) -> bool {
        #[cfg(feature = "pro")]
        { matches!(self, Tier::Pro) }
        #[cfg(not(feature = "pro"))]
        { false }
    }

    #[allow(dead_code)]
    pub fn consolidation_enabled(&self) -> bool {
        #[cfg(feature = "pro")]
        { matches!(self, Tier::Pro) }
        #[cfg(not(feature = "pro"))]
        { false }
    }

    pub fn knowledge_graph_enabled(&self) -> bool {
        matches!(self, Tier::Standard | Tier::Pro)
    }
}

fn default_port() -> u16 {
    7437
}

fn default_cloud_url() -> String {
    "https://api.ctxovrflw.dev".to_string()
}

fn default_sync_interval() -> u64 {
    60
}

fn default_auto_sync() -> bool {
    true
}

fn default_auto_consolidation() -> bool {
    true
}

fn default_consolidation_interval() -> u64 {
    6 * 60 * 60
}

fn default_embedding_model() -> String {
    "all-MiniLM-L6-v2".to_string()
}

impl Config {
    pub fn data_dir() -> Result<PathBuf> {
        let dir = dirs::home_dir()
            .context("Could not determine home directory")?
            .join(".ctxovrflw");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::data_dir()?.join("config.toml"))
    }

    pub fn db_path() -> Result<PathBuf> {
        Ok(Self::data_dir()?.join("memories.db"))
    }

    pub fn pid_path() -> Result<PathBuf> {
        Ok(Self::data_dir()?.join("ctxovrflw.pid"))
    }

    pub fn model_dir() -> Result<PathBuf> {
        let dir = Self::data_dir()?.join("models");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    #[allow(dead_code)]
    pub fn sync_state_path() -> Result<PathBuf> {
        Ok(Self::data_dir()?.join("sync_state.json"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        let mut config = if path.exists() {
            let contents = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config at {}", path.display()))?;
            toml::from_str(&contents).context("Failed to parse config.toml")?
        } else {
            Self::default()
        };

        // Resolve embedding_dim from model registry
        let dim = crate::embed::models::get_model(&config.embedding_model)
            .map(|m| m.dim)
            .unwrap_or(384);
        config.embedding_dim = dim;

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, &contents)?;

        // Restrict permissions to owner-only (600) — config contains API keys and encryption keys
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }

        Ok(())
    }

    pub fn is_logged_in(&self) -> bool {
        self.api_key.is_some() && self.device_id.is_some()
    }

    /// Get the encryption key, either from cache (if <30 days) or None.
    pub fn get_cached_key(&self) -> Option<[u8; 32]> {
        let cached = self.cached_key.as_ref()?;
        let cached_at = self.key_cached_at.as_ref()?;

        // Check 30-day expiry
        if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(cached_at) {
            let age = chrono::Utc::now() - ts.to_utc();
            if age.num_days() >= 30 {
                return None; // Expired
            }
        } else {
            return None;
        }

        // Decode hex key
        let bytes = hex_decode(cached)?;
        if bytes.len() != 32 {
            return None;
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Some(key)
    }

    /// Cache the encryption key for 30 days.
    pub fn cache_key(&mut self, key: &[u8; 32]) -> Result<()> {
        self.cached_key = Some(hex_encode(key));
        self.key_cached_at = Some(chrono::Utc::now().to_rfc3339());
        self.save()
    }

    /// Clear the cached key (logout or expiry).
    #[allow(dead_code)]
    pub fn clear_cached_key(&mut self) -> Result<()> {
        self.cached_key = None;
        self.key_cached_at = None;
        self.save()
    }

    /// Verify and decode the capability token, if present and valid.
    pub fn capability(&self) -> Option<crate::capability::CapabilityPayload> {
        self.capability_token.as_ref().and_then(|t| {
            let payload = crate::capability::verify_capability_token(t).ok()?;
            // Validate claims (iat, tier) but don't require subject match
            // Subject mismatch can happen legitimately (config migration, etc.)
            if let Err(e) = payload.validate(None) {
                tracing::warn!("Capability token validation failed: {}", e);
                return None;
            }
            // Warn about subject mismatch but don't fail
            if let Some(ref device_id) = self.device_id {
                if payload.sub != *device_id {
                    tracing::debug!("Capability token subject mismatch: token={}, current={}", payload.sub, device_id);
                }
            }
            Some(payload)
        })
    }

    /// Check if a feature is enabled, consulting capability token first.
    /// Falls back to local tier config if no valid token.
    pub fn feature_enabled(&self, feature: &str) -> bool {
        if let Some(cap) = self.capability() {
            return cap.has_feature(feature);
        }
        // Fallback to local tier (for offline/free users)
        match feature {
            "hybrid_search" => self.tier.cloud_sync_enabled(), // standard+
            "knowledge_graph" => self.tier.knowledge_graph_enabled(),
            "webhooks" => matches!(self.tier, Tier::Standard | Tier::Pro),
            "consolidation" => self.tier.consolidation_enabled(),
            "context_synthesis" => self.tier.context_synthesis_enabled(),
            _ => false,
        }
    }

    pub fn effective_max_memories(&self) -> Option<usize> {
        if let Some(cap) = self.capability() {
            return cap.max_memories;
        }
        self.tier.max_memories()
    }

    pub fn effective_cloud_sync(&self) -> bool {
        if let Some(cap) = self.capability() {
            return cap.cloud_sync;
        }
        self.tier.cloud_sync_enabled()
    }

    /// Check if zero-knowledge encryption is set up.
    pub fn is_encrypted(&self) -> bool {
        self.pin_verifier.is_some() && self.key_salt.is_some()
    }

    /// Generate a device fingerprint from hostname + OS
    pub fn device_fingerprint() -> String {
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;
        format!("{}-{}-{}", hostname, os, arch)
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    crate::validation::hex_encode(bytes)
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    crate::validation::hex_decode(s)
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: default_port(),
            tier: Tier::Free,
            embedding_model: default_embedding_model(),
            embedding_dim: 384, // Will be updated by load()
            cloud_url: default_cloud_url(),
            api_key: None,
            device_id: None,
            sync_interval_secs: default_sync_interval(),
            auto_sync: default_auto_sync(),
            auto_consolidation: default_auto_consolidation(),
            consolidation_interval_secs: default_consolidation_interval(),
            email: None,
            pin_verifier: None,
            key_salt: None,
            cached_key: None,
            key_cached_at: None,
            remote_daemon_url: None,
            capability_token: None,
            auth_token: None,
        }
    }
}

impl Config {
    /// Ensure an auth token exists; generate one if missing.
    pub fn ensure_auth_token(&mut self) -> Result<()> {
        if self.auth_token.is_none() {
            use rand::Rng;
            let token: String = rand::thread_rng()
                .sample_iter(&rand::distributions::Alphanumeric)
                .take(48)
                .map(char::from)
                .collect();
            self.auth_token = Some(token);
            self.save()?;
        }
        Ok(())
    }

    /// Returns true if this instance should connect to a remote daemon
    /// instead of running its own.
    pub fn is_remote_client(&self) -> bool {
        self.remote_daemon_url.is_some()
    }

    /// The base URL for the daemon API (local or remote).
    pub fn daemon_url(&self) -> String {
        self.remote_daemon_url
            .clone()
            .unwrap_or_else(|| format!("http://127.0.0.1:{}", self.port))
    }
}
