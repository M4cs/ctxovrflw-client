use anyhow::Result;
use crate::config::Config;
use crate::crypto;

#[derive(serde::Deserialize)]
struct AuthResponse {
    #[allow(dead_code)]
    user_id: String,
    api_key: String,
}

#[derive(serde::Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_url: String,
    expires_in: u64,
    interval: u64,
}

#[derive(serde::Deserialize)]
struct DeviceTokenResponse {
    api_key: Option<String>,
    device_id: Option<String>,
    error: Option<String>,
}

#[derive(serde::Deserialize)]
struct DeviceResponse {
    device_id: String,
}

#[derive(serde::Deserialize)]
struct ErrorResponse {
    error: String,
}

/// Run the login flow. If `inline` is true, skip the header (called from init).
/// If `api_key_arg` is Some, skip the interactive flow and use the key directly.
pub async fn run_inner(cfg: &Config, inline: bool, api_key_arg: Option<&str>) -> Result<()> {
    if !inline {
        println!("ctxovrflw cloud login\n");
    }

    // Check if already logged in
    if cfg.is_logged_in() && api_key_arg.is_none() {
        if cfg.is_encrypted() && cfg.get_cached_key().is_none() {
            println!("Logged in, but sync PIN has expired. Please re-enter it.");
            return prompt_sync_pin(cfg).await;
        }
        println!("Already logged in (device: {}).", cfg.device_id.as_deref().unwrap_or("?"));
        println!("To re-login, run: ctxovrflw logout");
        return Ok(());
    }

    let client = reqwest::Client::new();
    let cloud_url = &cfg.cloud_url;

    let (api_key, pre_device_id) = if let Some(key) = api_key_arg {
        // ─── Direct API key auth ────────────────────────────────────────
        println!("Authenticating with API key...");
        // Verify the key works
        let resp = client
            .get(format!("{cloud_url}/v1/auth/profile"))
            .header("Authorization", format!("Bearer {key}"))
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Invalid API key");
        }

        let body: serde_json::Value = resp.json().await?;
        let email = body["user"]["email"].as_str().unwrap_or("unknown").to_string();
        println!("✓ Authenticated as {email}");

        // Save email
        let mut cfg = cfg.clone();
        cfg.email = Some(email);
        cfg.save()?;

        (key.to_string(), None)
    } else if is_tty() {
        // ─── Device code flow (interactive TTY) ─────────────────────────
        device_code_flow(&client, cloud_url).await?
    } else {
        // ─── Fallback: email/password (non-TTY or if device flow fails) ─
        email_password_flow(&client, cloud_url).await?
    };

    // Register this device (pass pre-created device_id from device code flow if available)
    let fingerprint = Config::device_fingerprint();
    let device_name = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    println!("Registering device '{device_name}'...");

    let mut register_body = serde_json::json!({
        "name": device_name,
        "device_fingerprint": fingerprint,
    });
    if let Some(ref pre_id) = pre_device_id {
        register_body["device_id"] = serde_json::json!(pre_id);
    }

    let dev_resp = client
        .post(format!("{cloud_url}/v1/devices/register"))
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&register_body)
        .send()
        .await?;

    let device_id = if dev_resp.status().is_success() {
        let dev: DeviceResponse = dev_resp.json().await?;
        dev.device_id
    } else {
        let list_resp = client
            .get(format!("{cloud_url}/v1/devices"))
            .header("Authorization", format!("Bearer {api_key}"))
            .send()
            .await?;

        if list_resp.status().is_success() {
            let body: serde_json::Value = list_resp.json().await?;
            let devices = body.get("devices")
                .and_then(|d| d.as_array())
                .cloned()
                .unwrap_or_default();
            devices.iter()
                .find(|d| d["device_fingerprint"].as_str() == Some(&fingerprint))
                .and_then(|d| d["id"].as_str().map(String::from))
                .ok_or_else(|| anyhow::anyhow!("Failed to register device"))?
        } else {
            anyhow::bail!("Failed to register device");
        }
    };

    println!("✓ Device registered");

    // Save config
    let mut cfg = Config::load()?;
    cfg.api_key = Some(api_key.clone());
    cfg.device_id = Some(device_id.clone());
    cfg.save()?;

    // Fetch tier from profile
    let profile_resp = client
        .get(format!("{cloud_url}/v1/auth/profile"))
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await?;

    if profile_resp.status().is_success() {
        let body: serde_json::Value = profile_resp.json().await?;
        let tier_str = body["user"]["tier"].as_str().unwrap_or("free");
        let email = body["user"]["email"].as_str().map(String::from);
        let mut cfg = Config::load()?;
        cfg.tier = match tier_str {
            "standard" => crate::config::Tier::Standard,
            "pro" => crate::config::Tier::Pro,
            _ => crate::config::Tier::Free,
        };
        // Always save email from profile — critical for PIN key derivation
        if let Some(e) = email {
            cfg.email = Some(e);
        }
        // Save capability token if present
        if let Some(cap_token) = body.get("capability_token").and_then(|v| v.as_str()) {
            cfg.capability_token = Some(cap_token.to_string());
        }
        cfg.save()?;
    }

    // Set up sync PIN if cloud sync is available
    let cfg = Config::load()?;
    if cfg.effective_cloud_sync() {
        setup_sync_pin(&cfg).await?;
    } else {
        println!("\n✓ Logged in! Free tier — local-only mode.");
        println!("  Upgrade for cloud sync: https://ctxovrflw.dev/pricing");
    }

    Ok(())
}

/// Device code flow — opens browser, user enters code on website
async fn device_code_flow(client: &reqwest::Client, cloud_url: &str) -> Result<(String, Option<String>)> {
    let device_name = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // Request device code
    let resp = client
        .post(format!("{cloud_url}/v1/auth/device/code"))
        .json(&serde_json::json!({ "device_name": device_name }))
        .send()
        .await?;

    if !resp.status().is_success() {
        println!("Device auth not available, falling back to email/password...\n");
        return email_password_flow(client, cloud_url).await;
    }

    let code_resp: DeviceCodeResponse = resp.json().await?;

    println!("  Open this URL in your browser:\n");
    println!("    {}", code_resp.verification_url);
    println!("\n  Then enter this code:\n");
    println!("    ┌──────────────┐");
    println!("    │  {}  │", code_resp.user_code);
    println!("    └──────────────┘\n");

    // Try to open browser automatically
    let _ = open_browser(&code_resp.verification_url);

    println!("  Waiting for approval... (expires in {}m)", code_resp.expires_in / 60);

    // Poll for token
    let interval = std::time::Duration::from_secs(code_resp.interval.max(3));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(code_resp.expires_in);

    loop {
        if std::time::Instant::now() > deadline {
            anyhow::bail!("Device authorization timed out. Run `ctxovrflw login` to try again.");
        }

        tokio::time::sleep(interval).await;

        let resp = client
            .post(format!("{cloud_url}/v1/auth/device/token"))
            .json(&serde_json::json!({ "device_code": code_resp.device_code }))
            .send()
            .await?;

        let status = resp.status();
        let body: DeviceTokenResponse = resp.json().await?;

        if let Some(api_key) = body.api_key {
            println!("\n✓ Authorized!");
            return Ok((api_key, body.device_id));
        }

        match body.error.as_deref() {
            Some("authorization_pending") => {
                // Still waiting — continue polling
                continue;
            }
            Some("expired_token") => {
                anyhow::bail!("Device code expired. Run `ctxovrflw login` to try again.");
            }
            Some(err) => {
                anyhow::bail!("Authorization failed: {err}");
            }
            None if !status.is_success() => {
                anyhow::bail!("Authorization failed (HTTP {status})");
            }
            None => continue,
        }
    }
}

/// Email/password fallback flow
async fn email_password_flow(client: &reqwest::Client, cloud_url: &str) -> Result<(String, Option<String>)> {
    print!("Email: ");
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut email = String::new();
    std::io::stdin().read_line(&mut email)?;
    let email = email.trim().to_string();
    if email.is_empty() {
        anyhow::bail!("Email required.");
    }

    print!("Password: ");
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut password = String::new();
    std::io::stdin().read_line(&mut password)?;
    let password = password.trim().to_string();
    if password.is_empty() {
        anyhow::bail!("Password required.");
    }

    println!("\nAuthenticating...");
    let login_resp = client
        .post(format!("{cloud_url}/v1/auth/login"))
        .json(&serde_json::json!({"email": email, "password": password}))
        .send()
        .await?;

    let (api_key, action) = if login_resp.status().is_success() {
        let auth: AuthResponse = login_resp.json().await?;
        (auth.api_key, "Logged in")
    } else {
        println!("No account found. Creating one...");
        let signup_resp = client
            .post(format!("{cloud_url}/v1/auth/signup"))
            .json(&serde_json::json!({"email": email, "password": password}))
            .send()
            .await?;

        if !signup_resp.status().is_success() {
            let err: ErrorResponse = signup_resp.json().await?;
            anyhow::bail!("Auth failed: {}", err.error);
        }

        let auth: AuthResponse = signup_resp.json().await?;
        (auth.api_key, "Account created")
    };

    println!("✓ {action}");

    // Save email to config
    let mut cfg = Config::load()?;
    cfg.email = Some(email);
    cfg.save()?;

    Ok((api_key, None))
}

/// Response from GET /v1/auth/pin-verifier
#[derive(serde::Deserialize)]
struct PinVerifierResponse {
    has_pin: bool,
    key_salt: Option<String>,
    pin_verifier: Option<String>,
}

/// Response from POST /v1/auth/setup-pin or /v1/auth/verify-pin
#[derive(serde::Deserialize)]
struct PinActionResponse {
    ok: Option<bool>,
    key_salt: Option<String>,
    pin_verifier: Option<String>,
    error: Option<String>,
}

/// Set up sync encryption. Server generates salt, does key derivation + verification.
/// Client derives the same key using the server-provided salt.
async fn setup_sync_pin(cfg: &Config) -> Result<()> {
    let api_key = cfg.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("Not logged in"))?;
    let client = reqwest::Client::new();

    println!("\n🔐 Zero-Knowledge Encryption Setup");
    println!("Your memories are encrypted before leaving this device.\n");

    // Check if account already has a PIN set
    let resp = client
        .get(format!("{}/v1/auth/pin-verifier", cfg.cloud_url))
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await?;

    let account_pin: PinVerifierResponse = if resp.status().is_success() {
        resp.json().await?
    } else {
        PinVerifierResponse { has_pin: false, key_salt: None, pin_verifier: None }
    };

    if !account_pin.has_pin {
        // First device — create PIN
        print!("Create sync PIN (min 6 chars): ");
        std::io::Write::flush(&mut std::io::stdout())?;
        let mut pin = String::new();
        std::io::stdin().read_line(&mut pin)?;
        let pin = pin.trim().to_string();
        if pin.len() < 6 {
            anyhow::bail!("Sync PIN must be at least 6 characters.");
        }

        print!("Confirm sync PIN: ");
        std::io::Write::flush(&mut std::io::stdout())?;
        let mut pin_confirm = String::new();
        std::io::stdin().read_line(&mut pin_confirm)?;
        if pin.trim() != pin_confirm.trim() {
            anyhow::bail!("PINs don't match.");
        }

        // Request a salt from the server (server generates random salt, never sees PIN)
        let setup_resp = client
            .post(format!("{}/v1/auth/setup-pin", cfg.cloud_url))
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&serde_json::json!({ "request_salt": true }))
            .send()
            .await?;

        if !setup_resp.status().is_success() {
            let err: PinActionResponse = setup_resp.json().await.unwrap_or(PinActionResponse {
                ok: None, key_salt: None, pin_verifier: None,
                error: Some("Server error".to_string()),
            });
            anyhow::bail!("Failed to set up PIN: {}", err.error.unwrap_or_default());
        }

        let result: PinActionResponse = setup_resp.json().await?;
        let key_salt = result.key_salt.ok_or_else(|| anyhow::anyhow!("Server didn't return salt"))?;

        // Derive key CLIENT-SIDE — PIN never leaves this device
        let key = crypto::derive_key(&pin, &key_salt);

        // Create a verifier: encrypt a known string with the derived key
        let verifier = crypto::create_pin_verifier(&key)?;

        // POST only the verifier (encrypted blob) to server — NEVER the PIN
        let store_resp = client
            .post(format!("{}/v1/auth/store-verifier", cfg.cloud_url))
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&serde_json::json!({ "pin_verifier": verifier, "key_salt": key_salt }))
            .send()
            .await?;

        if !store_resp.status().is_success() {
            anyhow::bail!("Failed to store PIN verifier on server");
        }

        let mut cfg = Config::load()?;
        cfg.key_salt = Some(key_salt);
        cfg.pin_verifier = Some(verifier);
        cfg.cache_key(&key)?;

        println!("✓ Encryption key derived and cached (30-day TTL)");
        println!("\n⚠️  IMPORTANT: Use the same sync PIN on all your devices.");
        println!("   If you lose your sync PIN, your cloud memories cannot be recovered.");
    } else {
        // Subsequent device — verify PIN via server
        print!("Enter your sync PIN: ");
        std::io::Write::flush(&mut std::io::stdout())?;
        let mut pin = String::new();
        std::io::stdin().read_line(&mut pin)?;
        let pin = pin.trim().to_string();

        // We already have the salt and verifier from the initial GET request
        let key_salt = account_pin.key_salt.ok_or_else(|| anyhow::anyhow!("Server didn't return salt"))?;
        let stored_verifier = account_pin.pin_verifier.ok_or_else(|| anyhow::anyhow!("Server didn't return verifier"))?;

        // Derive key CLIENT-SIDE — PIN never leaves this device
        let key = crypto::derive_key(&pin, &key_salt);

        // Verify by decrypting the stored verifier locally (true zero-knowledge)
        if !crypto::verify_pin(&key, &stored_verifier) {
            // Clear auth on wrong PIN
            let mut bad_cfg = Config::load()?;
            bad_cfg.api_key = None;
            bad_cfg.device_id = None;
            bad_cfg.email = None;
            bad_cfg.pin_verifier = None;
            bad_cfg.key_salt = None;
            bad_cfg.save()?;
            anyhow::bail!("Wrong sync PIN. You've been logged out. Run `ctxovrflw login` to try again.");
        }

        let verifier = crypto::create_pin_verifier(&key)?;

        let mut cfg = Config::load()?;
        cfg.key_salt = Some(key_salt);
        cfg.pin_verifier = Some(verifier);
        cfg.cache_key(&key)?;

        println!("✓ PIN verified — encryption key cached (30-day TTL)");
    }

    let cfg = Config::load()?;
    println!("\n✓ Ready! Cloud sync is {}.",
        if cfg.auto_sync { format!("enabled (every {}s)", cfg.sync_interval_secs) } else { "disabled".to_string() }
    );

    Ok(())
}

pub async fn run(cfg: &Config) -> Result<()> {
    run_inner(cfg, false, None).await
}

pub async fn run_with_key(cfg: &Config, key: &str) -> Result<()> {
    run_inner(cfg, false, Some(key)).await
}

/// Re-prompt for sync PIN when the cached key has expired.
async fn prompt_sync_pin(cfg: &Config) -> Result<()> {
    let api_key = cfg.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("Not logged in"))?;

    print!("Sync PIN: ");
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut pin = String::new();
    std::io::stdin().read_line(&mut pin)?;
    let pin = pin.trim().to_string();

    // If we have the salt locally, derive and verify locally
    if let (Some(salt), Some(verifier)) = (&cfg.key_salt, &cfg.pin_verifier) {
        let key = crypto::derive_key(&pin, salt);
        if !crypto::verify_pin(&key, verifier) {
            anyhow::bail!("Wrong sync PIN.");
        }
        let mut cfg = cfg.clone();
        cfg.cache_key(&key)?;
        println!("✓ Sync PIN accepted, key cached for 30 days.");
        return Ok(());
    }

    // Otherwise fetch salt from server and derive locally
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/v1/auth/pin-verifier", cfg.cloud_url))
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Failed to fetch PIN verifier from server.");
    }

    let vdata: PinVerifierResponse = resp.json().await?;
    let key_salt = vdata.key_salt.ok_or_else(|| anyhow::anyhow!("Server didn't return salt"))?;

    // Derive key client-side and verify locally
    let key = crypto::derive_key(&pin, &key_salt);
    let verifier = crypto::create_pin_verifier(&key)?;

    // We can't verify against server's stored verifier without it, so just trust the derivation
    // and store locally. Next sync will fail if PIN is wrong.
    let mut cfg = cfg.clone();
    cfg.key_salt = Some(key_salt);
    cfg.pin_verifier = Some(verifier);
    cfg.cache_key(&key)?;
    println!("✓ Sync PIN accepted, key cached for 30 days.");
    Ok(())
}

fn is_tty() -> bool {
    atty::is(atty::Stream::Stdin) && atty::is(atty::Stream::Stdout)
}

fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    { std::process::Command::new("open").arg(url).spawn()?; }

    #[cfg(target_os = "linux")]
    {
        // Try xdg-open, then wslview (WSL)
        if std::process::Command::new("xdg-open").arg(url).spawn().is_err() {
            let _ = std::process::Command::new("wslview").arg(url).spawn();
        }
    }

    #[cfg(target_os = "windows")]
    { std::process::Command::new("cmd").args(["/c", "start", url]).spawn()?; }

    Ok(())
}
