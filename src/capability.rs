use ed25519_dalek::{Signature, VerifyingKey, Verifier};
use serde::Deserialize;

/// Cloud's Ed25519 public key (embedded at compile time)
const CLOUD_PUBLIC_KEY_HEX: &str = "dd4137d20c68eb5283eabeda1225a3cbb45c35e808ef1e1aacb96eaf7d0e9c6c";

#[derive(Debug, Deserialize, Clone)]
pub struct CapabilityPayload {
    pub sub: String,
    pub tier: String,
    pub features: Vec<String>,
    pub max_memories: Option<usize>,
    pub max_devices: Option<usize>,
    pub cloud_sync: bool,
    pub iat: u64,
    pub exp: u64,
}

impl CapabilityPayload {
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now > self.exp
    }

    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }
}

/// Verify and decode a capability token.
/// Token format: base64url(payload_json).base64url(ed25519_signature)
pub fn verify_capability_token(token: &str) -> Result<CapabilityPayload, String> {
    let parts: Vec<&str> = token.splitn(2, '.').collect();
    if parts.len() != 2 {
        return Err("Invalid token format".into());
    }

    let payload_bytes = base64url_decode(parts[0]).map_err(|e| format!("Invalid payload encoding: {e}"))?;
    let sig_bytes = base64url_decode(parts[1]).map_err(|e| format!("Invalid signature encoding: {e}"))?;

    // Verify signature
    let pub_key_bytes = hex_decode(CLOUD_PUBLIC_KEY_HEX).map_err(|e| format!("Invalid public key: {e}"))?;
    let pub_key_array: [u8; 32] = pub_key_bytes.try_into().map_err(|_| "Public key wrong length")?;
    let verifying_key = VerifyingKey::from_bytes(&pub_key_array)
        .map_err(|e| format!("Invalid public key: {e}"))?;
    let sig_array: [u8; 64] = sig_bytes.try_into().map_err(|_| "Signature wrong length")?;
    let signature = Signature::from_bytes(&sig_array);
    verifying_key.verify(&payload_bytes, &signature).map_err(|_| "Invalid signature")?;

    // Decode payload
    let payload: CapabilityPayload = serde_json::from_slice(&payload_bytes)
        .map_err(|e| format!("Invalid payload JSON: {e}"))?;

    // Check expiry
    if payload.is_expired() {
        return Err("Token expired".into());
    }

    Ok(payload)
}

fn base64url_decode(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(input)
        .map_err(|e| e.to_string())
}

fn hex_decode(input: &str) -> Result<Vec<u8>, String> {
    (0..input.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&input[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}
