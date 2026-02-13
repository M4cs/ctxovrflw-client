use anyhow::Result;

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

/// Compare current version against latest. Returns true if an update is available.
fn is_newer(latest: &str) -> bool {
    let latest_clean = latest.trim_start_matches('v');
    let current_clean = CURRENT_VERSION.trim_start_matches('v');

    // Simple semver comparison
    let parse = |s: &str| -> (u32, u32, u32) {
        let parts: Vec<u32> = s.split('.').filter_map(|p| p.parse().ok()).collect();
        (
            parts.first().copied().unwrap_or(0),
            parts.get(1).copied().unwrap_or(0),
            parts.get(2).copied().unwrap_or(0),
        )
    };

    parse(latest_clean) > parse(current_clean)
}

/// Just print version + check for updates.
pub async fn version() -> Result<()> {
    println!("ctxovrflw v{CURRENT_VERSION}");

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

    let bytes = resp.bytes().await?;
    println!("Downloaded {} bytes", bytes.len());

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

    // Replace current binary
    let current_exe = std::env::current_exe()?;
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
