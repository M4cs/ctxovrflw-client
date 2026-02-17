use anyhow::Result;
use sha2::{Sha256, Digest};

use crate::config::Config;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(serde::Deserialize)]
struct LatestResponse {
    version: String,
}

/// Check the API for the latest available version.
pub async fn check_latest(cfg: &Config) -> Result<Option<String>> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/v1/releases/latest", cfg.cloud_url))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    let latest: LatestResponse = resp.json().await?;
    Ok(Some(latest.version))
}

/// Compare semver versions. Supports pre-release (e.g., 0.5.0-rc.1).
/// Pre-release versions are OLDER than their release counterpart (0.5.0-rc.1 < 0.5.0).
fn is_newer(latest: &str) -> bool {
    let latest_clean = latest.trim_start_matches('v');
    let current_clean = CURRENT_VERSION.trim_start_matches('v');

    match (parse_semver(latest_clean), parse_semver(current_clean)) {
        (Some(l), Some(c)) => l > c,
        _ => false, // Can't parse, don't update
    }
}

#[derive(Debug, PartialEq, Eq)]
struct SemVer {
    major: u32,
    minor: u32,
    patch: u32,
    pre: Option<String>,
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.major.cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
        {
            std::cmp::Ordering::Equal => {
                match (&self.pre, &other.pre) {
                    (None, None) => std::cmp::Ordering::Equal,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (Some(a), Some(b)) => a.cmp(b),
                }
            }
            ord => ord,
        }
    }
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn parse_semver(s: &str) -> Option<SemVer> {
    let (version_part, pre) = if let Some((v, p)) = s.split_once('-') {
        (v, Some(p.to_string()))
    } else {
        (s, None)
    };

    let parts: Vec<u32> = version_part.split('.').filter_map(|p| p.parse().ok()).collect();
    if parts.len() < 3 { return None; }

    Some(SemVer {
        major: parts[0],
        minor: parts[1],
        patch: parts[2],
        pre,
    })
}

/// Just print version + check for updates.
pub async fn version() -> Result<()> {
    println!("ctxovrflw v{CURRENT_VERSION}");

    // Show binary hash
    if let Ok(exe_path) = std::env::current_exe() {
        if let Ok(bytes) = std::fs::read(&exe_path) {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let hash = format!("{:x}", hasher.finalize());
            println!("Binary SHA256: {}...", &hash[..16]);
        }
    }

    let cfg = Config::load().unwrap_or_default();
    match check_latest(&cfg).await {
        Ok(Some(latest)) => {
            if is_newer(&latest) {
                println!("⬆ Update available: {latest} (current: v{CURRENT_VERSION})");
                println!("  Run `ctxovrflw update` to install");
            } else {
                println!("✓ Up to date");
            }
        }
        Ok(None) => println!("  Could not check for updates"),
        Err(_) => println!("  Could not check for updates (offline?)"),
    }

    Ok(())
}

/// Check for updates, optionally download and replace the binary.
pub async fn run(check_only: bool) -> Result<()> {
    let cfg = Config::load().unwrap_or_default();

    println!("Current version: v{CURRENT_VERSION}");
    print!("Checking for updates... ");

    let latest = match check_latest(&cfg).await? {
        Some(v) => v,
        None => {
            println!("no releases found.");
            return Ok(());
        }
    };

    if !is_newer(&latest) {
        println!("already up to date (latest: {latest})");
        return Ok(());
    }

    println!("update available: {latest}");

    if check_only {
        println!("\nRun `ctxovrflw update` to install.");
        return Ok(());
    }

    // Detect platform
    let (os, arch) = detect_platform();
    println!("Downloading {latest} for {os}-{arch}...");

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/v1/download?os={os}&arch={arch}",
            cfg.cloud_url
        ))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Download failed ({status}): {body}");
    }

    // Capture hash header before consuming body
    let expected_hash = resp.headers()
        .get("x-sha256")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let bytes = resp.bytes().await?;
    println!("Downloaded {} bytes", bytes.len());

    // Verify SHA256
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual_hash = format!("{:x}", hasher.finalize());

    if let Some(expected) = &expected_hash {
        if actual_hash != *expected {
            anyhow::bail!(
                "SHA256 mismatch! Expected: {}, Got: {}. Download may be corrupted or tampered with.",
                expected, actual_hash
            );
        }
        println!("✓ SHA256 verified: {}", &actual_hash[..16]);
    } else {
        // Fallback: try to fetch checksums from API
        let checksums_url = format!("{}/v1/releases/{}/checksums", cfg.cloud_url, latest);
        if let Ok(checksums_resp) = client.get(&checksums_url)
            .timeout(std::time::Duration::from_secs(5))
            .send().await
        {
            if checksums_resp.status().is_success() {
                if let Ok(checksums_text) = checksums_resp.text().await {
                    let (dl_os, dl_arch) = detect_platform();
                    let artifact = format!("ctxovrflw-{}-{}", dl_os, dl_arch);
                    let expected_line = checksums_text.lines()
                        .find(|l| l.contains(&artifact));
                    if let Some(line) = expected_line {
                        let expected = line.split_whitespace().next().unwrap_or("");
                        if actual_hash != expected {
                            anyhow::bail!(
                                "SHA256 mismatch! Expected: {}, Got: {}",
                                expected, actual_hash
                            );
                        }
                        println!("✓ SHA256 verified: {}", &actual_hash[..16]);
                    }
                }
            }
        }
    }

    // Extract tarball to temp dir
    let tmp_dir = tempfile::tempdir()?;
    let tarball_path = tmp_dir.path().join("update.tar.gz");
    std::fs::write(&tarball_path, &bytes)?;

    // Extract
    let status = std::process::Command::new("tar")
        .args(["xzf", tarball_path.to_str().unwrap(), "-C", tmp_dir.path().to_str().unwrap()])
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to extract update archive");
    }

    let new_binary = tmp_dir.path().join("ctxovrflw");
    if !new_binary.exists() {
        anyhow::bail!("Binary not found in archive");
    }

    // Verify the new binary runs
    let check = std::process::Command::new(&new_binary)
        .args(["--version"])
        .output();

    match check {
        Ok(output) if output.status.success() => {
            let ver = String::from_utf8_lossy(&output.stdout);
            println!("Verified: {}", ver.trim());
        }
        _ => {
            anyhow::bail!("New binary failed verification. Update aborted.");
        }
    }

    // Replace binary at canonical location (~/.ctxovrflw/bin/ctxovrflw)
    // Falls back to current exe location if canonical doesn't exist
    let canonical_bin = Config::data_dir()?.join("bin").join("ctxovrflw");
    let current_exe = if canonical_bin.exists() {
        canonical_bin
    } else {
        std::env::current_exe()?
    };
    let backup = current_exe.with_extension("old");

    // Backup current binary
    std::fs::rename(&current_exe, &backup)?;

    // Move new binary into place
    match std::fs::copy(&new_binary, &current_exe) {
        Ok(_) => {
            // Set executable permissions
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&current_exe, std::fs::Permissions::from_mode(0o755))?;
            }

            // Remove backup
            let _ = std::fs::remove_file(&backup);

            println!("✓ Updated to {latest}");
        }
        Err(e) => {
            // Restore backup on failure
            let _ = std::fs::rename(&backup, &current_exe);
            anyhow::bail!("Failed to install update: {e}. Restored previous version.");
        }
    }

    // Update ONNX runtime + model if present in archive
    let data_dir = Config::data_dir()?;

    let new_model = tmp_dir.path().join("models/all-MiniLM-L6-v2-q8.onnx");
    if new_model.exists() {
        let model_dir = data_dir.join("models");
        std::fs::create_dir_all(&model_dir)?;
        let _ = std::fs::copy(&new_model, model_dir.join("all-MiniLM-L6-v2-q8.onnx"));
        let tokenizer = tmp_dir.path().join("models/tokenizer.json");
        if tokenizer.exists() {
            let _ = std::fs::copy(&tokenizer, model_dir.join("tokenizer.json"));
        }
        println!("✓ Models updated");
    }

    // Copy ONNX runtime lib if present
    for lib_name in &["libonnxruntime.so", "libonnxruntime.dylib"] {
        let lib_path = tmp_dir.path().join(lib_name);
        if lib_path.exists() {
            let dest = current_exe.parent().unwrap_or(std::path::Path::new("/usr/local/bin"));
            let _ = std::fs::copy(&lib_path, dest.join(lib_name));
        }
    }

    // Restart daemon if running
    if crate::daemon::is_service_running() {
        println!("Restarting daemon...");
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "restart", "ctxovrflw"])
            .status();
        println!("✓ Daemon restarted");
    }

    Ok(())
}

fn detect_platform() -> (&'static str, &'static str) {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    };

    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };

    (os, arch)
}
