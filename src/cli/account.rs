use anyhow::Result;
use crate::config::{Config, Tier};

#[derive(serde::Deserialize)]
struct ProfileResponse {
    user: UserProfile,
}

#[derive(serde::Deserialize)]
struct UserProfile {
    email: String,
    tier: String,
    is_admin: Option<bool>,
    email_verified: Option<bool>,
    has_subscription: Option<bool>,
    tier_expires_at: Option<String>,
    memory_count: u64,
    device_count: u64,
    limits: Limits,
}

#[derive(serde::Deserialize)]
struct Limits {
    max_memories: i64,
    max_devices: i64,
    cloud_sync: bool,
    context_synthesis: bool,
    consolidation: bool,
}

pub async fn run(cfg: &Config) -> Result<()> {
    println!("ctxovrflw account\n");

    if !cfg.is_logged_in() {
        println!("  Not logged in.");
        println!("  Run: ctxovrflw login\n");

        // Still show local stats
        let conn = crate::db::open()?;
        let count = crate::db::memories::count(&conn)?;
        let max = cfg.effective_max_memories()
            .map(|m| m.to_string())
            .unwrap_or_else(|| "unlimited".to_string());

        println!("  Local tier:      {:?}", cfg.tier);
        println!("  Local memories:  {}/{}", count, max);
        return Ok(());
    }

    let api_key = cfg.api_key.as_ref().unwrap();
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/v1/auth/profile", cfg.cloud_url))
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        println!("  Failed to fetch account info (HTTP {status})");
        println!("  Your API key may be invalid. Try: ctxovrflw logout && ctxovrflw login");
        return Ok(());
    }

    let body: serde_json::Value = resp.json().await?;
    let cap_token = body.get("capability_token").and_then(|v| v.as_str()).map(String::from);
    let profile: ProfileResponse = serde_json::from_value(body)?;
    let u = &profile.user;

    // Sync tier from cloud → local config if it changed
    let cloud_tier = match u.tier.as_str() {
        "standard" => Tier::Standard,
        "pro" => Tier::Pro,
        _ => Tier::Free,
    };
    if cfg.tier != cloud_tier || cap_token.is_some() {
        let mut updated_cfg = Config::load()?;
        updated_cfg.tier = cloud_tier.clone();
        if let Some(ct) = cap_token {
            updated_cfg.capability_token = Some(ct);
        }
        updated_cfg.save()?;
        if cfg.tier != cloud_tier {
            println!("  ✓ Tier updated locally: {:?} → {:?}\n", cfg.tier, updated_cfg.tier);
        }
    }

    // Tier display
    let tier_label = match u.tier.as_str() {
        "pro" => "Pro ⭐",
        "standard" => "Standard",
        "free" => "Free",
        other => other,
    };

    let memories_limit = if u.limits.max_memories < 0 { "unlimited".to_string() } else { u.limits.max_memories.to_string() };
    let devices_limit = if u.limits.max_devices < 0 { "unlimited".to_string() } else { u.limits.max_devices.to_string() };

    println!("  Email:           {}", u.email);
    println!("  Tier:            {tier_label}");

    if u.is_admin.unwrap_or(false) {
        println!("  Admin:           yes");
    }

    let verified = u.email_verified.unwrap_or(false);
    println!("  Email verified:  {}", if verified { "yes ✓" } else { "no — check your inbox" });

    if let Some(expires) = &u.tier_expires_at {
        println!("  Expires:         {expires}");
    } else if u.has_subscription.unwrap_or(false) {
        println!("  Billing:         Stripe subscription (active)");
    }

    println!();
    println!("  Memories:        {} / {}", u.memory_count, memories_limit);
    println!("  Devices:         {} / {}", u.device_count, devices_limit);
    println!();
    println!("  Cloud sync:      {}", if u.limits.cloud_sync { "enabled ✓" } else { "disabled" });
    println!("  Synthesis:       {}", if u.limits.context_synthesis { "enabled ✓" } else { "—" });
    println!("  Consolidation:   {}", if u.limits.consolidation { "enabled ✓" } else { "—" });

    // Local state
    println!();
    let conn = crate::db::open()?;
    let local_count = crate::db::memories::count(&conn)?;
    println!("  Local memories:  {}", local_count);
    println!("  Device ID:       {}", cfg.device_id.as_deref().unwrap_or("—"));

    if cfg.is_encrypted() {
        let key_status = if cfg.get_cached_key().is_some() { "cached ✓" } else { "expired — re-enter PIN" };
        println!("  Encryption:      {key_status}");
    }

    if u.tier == "free" && !u.limits.cloud_sync {
        println!("\n  💡 Upgrade for cloud sync: https://ctxovrflw.dev/pricing");
    }

    Ok(())
}
