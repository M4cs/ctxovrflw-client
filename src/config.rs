use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default)]
    pub tier: Tier,

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

    // Zero-knowledge encryption
    #[serde(default)]
    pub email: Option<String>,

    #[serde(default)]
    pub pin_verifier: Option<String>,

    /// Cached derived key (hex-encoded), cleared after 30 days
    #[serde(default)]
    pub cached_key: Option<String>,

    /// When the key was cached (ISO 8601)
    #[serde(default)]
    pub key_cached_at: Option<String>,
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
        matches!(self, Tier::Pro)
    }

    #[allow(dead_code)]
    pub fn consolidation_enabled(&self) -> bool {
        matches!(self, Tier::Pro)
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
        if path.exists() {
            let contents = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config at {}", path.display()))?;
            toml::from_str(&contents).context("Failed to parse config.toml")
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;
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

    /// Check if zero-knowledge encryption is set up.
    pub fn is_encrypted(&self) -> bool {
        self.pin_verifier.is_some() && self.email.is_some()
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
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: default_port(),
            tier: Tier::Free,
            cloud_url: default_cloud_url(),
            api_key: None,
            device_id: None,
            sync_interval_secs: default_sync_interval(),
            auto_sync: default_auto_sync(),
            email: None,
            pin_verifier: None,
            cached_key: None,
            key_cached_at: None,
        }
    }
}
