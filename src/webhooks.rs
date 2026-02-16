use serde_json::Value;

use crate::db;

/// Fire webhooks for a given event. Non-blocking — spawns tasks for each hook.
pub fn fire(event: &str, payload: Value) {
    let conn = match db::open() {
        Ok(c) => c,
        Err(_) => return,
    };

    let hooks = match db::webhooks::get_for_event(&conn, event) {
        Ok(h) => h,
        Err(_) => return,
    };

    if hooks.is_empty() {
        return;
    }

    let event = event.to_string();
    let payload = serde_json::json!({
        "event": event,
        "data": payload,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    for hook in hooks {
        let payload = payload.clone();
        let url = hook.url.clone();
        let secret = hook.secret.clone();

        tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default();

            let mut req = client.post(&url).json(&payload);

            // Add HMAC signature if secret is set
            if let Some(ref secret) = secret {
                let body = serde_json::to_string(&payload).unwrap_or_default();
                let signature = hmac_sha256(secret.as_bytes(), body.as_bytes());
                req = req.header("X-Ctxovrflw-Signature", format!("sha256={signature}"));
            }

            req = req.header("User-Agent", format!("ctxovrflw/{}", env!("CARGO_PKG_VERSION")));

            match req.send().await {
                Ok(resp) => {
                    if !resp.status().is_success() {
                        tracing::warn!(
                            "Webhook {} returned {}: {}",
                            url,
                            resp.status(),
                            resp.text().await.unwrap_or_default()
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!("Webhook {} failed: {}", url, e);
                }
            }
        });
    }
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> String {
    use ring::hmac;
    let key = hmac::Key::new(hmac::HMAC_SHA256, key);
    let tag = hmac::sign(&key, data);
    tag.as_ref().iter().map(|b| format!("{:02x}", b)).collect()
}
