//! Zero-knowledge encryption for cloud sync.
//!
//! Memories are encrypted client-side before push and decrypted after pull.
//! The server never sees plaintext. Key is derived from a user-chosen sync PIN
//! via PBKDF2-HMAC-SHA256 (600,000 iterations).
//!
//! Each memory is encrypted with AES-256-GCM using a unique nonce.
//! Format: [12-byte nonce][ciphertext+tag]

use anyhow::{Context, Result};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use std::num::NonZeroU32;

const PBKDF2_ITERATIONS: u32 = 600_000;
const SALT_PREFIX: &[u8] = b"ctxovrflw-zk-v1-";
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

/// Derives a 256-bit encryption key from a sync PIN + user email (as salt).
/// The email ensures different users with the same PIN get different keys.
pub fn derive_key(pin: &str, email: &str) -> [u8; KEY_LEN] {
    let mut salt = Vec::with_capacity(SALT_PREFIX.len() + email.len());
    salt.extend_from_slice(SALT_PREFIX);
    salt.extend_from_slice(email.as_bytes());

    let mut key = [0u8; KEY_LEN];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(PBKDF2_ITERATIONS).unwrap(),
        &salt,
        pin.as_bytes(),
        &mut key,
    );
    key
}

/// Encrypts plaintext with AES-256-GCM. Returns [nonce || ciphertext || tag].
pub fn encrypt(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<Vec<u8>> {
    let rng = SystemRandom::new();
    let unbound = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|_| anyhow::anyhow!("Failed to create encryption key"))?;
    let sealing_key = LessSafeKey::new(unbound);

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut nonce_bytes)
        .map_err(|_| anyhow::anyhow!("Failed to generate nonce"))?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut in_out = plaintext.to_vec();
    sealing_key
        .seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| anyhow::anyhow!("Encryption failed"))?;

    // Prepend nonce
    let mut result = Vec::with_capacity(NONCE_LEN + in_out.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&in_out);
    Ok(result)
}

/// Decrypts [nonce || ciphertext || tag] with AES-256-GCM.
pub fn decrypt(key: &[u8; KEY_LEN], data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < NONCE_LEN + AES_256_GCM.tag_len() {
        anyhow::bail!("Encrypted data too short");
    }

    let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
    let nonce = Nonce::assume_unique_for_key(nonce_bytes.try_into().unwrap());

    let unbound = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|_| anyhow::anyhow!("Failed to create decryption key"))?;
    let opening_key = LessSafeKey::new(unbound);

    let mut in_out = ciphertext.to_vec();
    let plaintext = opening_key
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| anyhow::anyhow!("Decryption failed — wrong sync PIN?"))?;

    Ok(plaintext.to_vec())
}

/// Encrypts a string, returns base64-encoded ciphertext.
pub fn encrypt_string(key: &[u8; KEY_LEN], plaintext: &str) -> Result<String> {
    use base64::Engine;
    let encrypted = encrypt(key, plaintext.as_bytes())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&encrypted))
}

/// Decrypts a base64-encoded ciphertext, returns plaintext string.
pub fn decrypt_string(key: &[u8; KEY_LEN], encoded: &str) -> Result<String> {
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("Invalid base64 in encrypted memory")?;
    let plaintext = decrypt(key, &data)?;
    String::from_utf8(plaintext).context("Decrypted data is not valid UTF-8")
}

/// Computes a SHA-256 content hash for sync verification.
/// This lets the server verify sync consistency without seeing content.
pub fn content_hash(plaintext: &str) -> String {
    use ring::digest;
    let hash = digest::digest(&digest::SHA256, plaintext.as_bytes());
    hex_encode(hash.as_ref())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Verify a sync PIN produces the same key by checking against a stored verifier.
/// The verifier is: encrypt("ctxovrflw-pin-verify", key) — stored in config.
pub fn create_pin_verifier(key: &[u8; KEY_LEN]) -> Result<String> {
    encrypt_string(key, "ctxovrflw-pin-verify")
}

/// Check if a PIN produces the correct key by decrypting the verifier.
pub fn verify_pin(key: &[u8; KEY_LEN], verifier: &str) -> bool {
    match decrypt_string(key, verifier) {
        Ok(plaintext) => plaintext == "ctxovrflw-pin-verify",
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let key = derive_key("test1234", "user@example.com");
        let plaintext = "Max prefers pizza over sushi";
        let encrypted = encrypt_string(&key, plaintext).unwrap();
        let decrypted = decrypt_string(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_wrong_pin_fails() {
        let key1 = derive_key("correct-pin", "user@example.com");
        let key2 = derive_key("wrong-pin", "user@example.com");
        let encrypted = encrypt_string(&key1, "secret").unwrap();
        assert!(decrypt_string(&key2, &encrypted).is_err());
    }

    #[test]
    fn test_different_emails_different_keys() {
        let key1 = derive_key("same-pin", "alice@example.com");
        let key2 = derive_key("same-pin", "bob@example.com");
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_content_hash_deterministic() {
        let h1 = content_hash("hello world");
        let h2 = content_hash("hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_pin_verifier() {
        let key = derive_key("mypin", "user@example.com");
        let verifier = create_pin_verifier(&key).unwrap();
        assert!(verify_pin(&key, &verifier));

        let wrong_key = derive_key("wrongpin", "user@example.com");
        assert!(!verify_pin(&wrong_key, &verifier));
    }
}
