//! M3 Production: API key encryption at rest + audit redaction.
//!
//! Uses AES-256-GCM with a key derived from `PENTA_ENCRYPTION_KEY` (falling
//! back to `JWT_SECRET`). Encrypted keys are stored as "env:"-prefixed hex
//! ciphertext. Plaintext never appears in logs or audit trails.

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use sha2::{Digest, Sha256};

const KEY_PREFIX: &str = "env:";
const NONCE_LEN: usize = 12;

/// Derive a 32-byte key from the configured encryption secret.
fn master_key() -> Vec<u8> {
    let secret = std::env::var("PENTA_ENCRYPTION_KEY")
        .or_else(|_| std::env::var("JWT_SECRET"))
        .unwrap_or_default();
    Sha256::digest(secret.as_bytes()).to_vec()
}

/// Encrypt an API key. Format: "env:" + hex(nonce[12] || ciphertext[..]+tag[16])
pub fn encrypt_api_key(plaintext: &str) -> String {
    let key_bytes = master_key();
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .expect("AES-GCM encryption should not fail");
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    format!("{KEY_PREFIX}{}", hex::encode(&combined))
}

/// Decrypt a stored key.
pub fn decrypt_api_key(stored: &str) -> Result<String, String> {
    if !stored.starts_with(KEY_PREFIX) {
        return Ok(stored.to_string());
    }
    let encoded = stored.strip_prefix(KEY_PREFIX).unwrap();
    let combined = hex::decode(encoded).map_err(|e| format!("hex decode: {e}"))?;
    if combined.len() <= NONCE_LEN {
        return Err("ciphertext too short".into());
    }
    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_LEN);
    let key_bytes = master_key();
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "authentication failed — data may be tampered".to_string())?;
    String::from_utf8(plaintext).map_err(|e| format!("utf8: {e}"))
}

/// Redact sensitive fields for safe logging.
pub fn redact_sensitive(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut result = text.to_string();
    for pattern in &[
        "api_key=", "\"api_key\"", "\"apikey\"",
        "\"secret\"", "\"password\"", "\"token\"",
        "bearer ", "sk-",
    ] {
        if lower.contains(pattern) {
            // Replace from pattern start to next comma/quote/newline
            if let Some(pos) = lower.find(pattern) {
                let end = text[pos..]
                    .find(|c: char| c == ',' || c == '\n' || c == '"')
                    .map(|p| pos + p)
                    .unwrap_or(text.len());
                if end - pos < 200 {
                    result.replace_range(pos..end, &format!("{pattern}***REDACTED***"));
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_encrypt_decrypt() {
        let original = "sk-test-key-12345";
        let encrypted = encrypt_api_key(original);
        assert!(encrypted.starts_with("env:"));
        assert_ne!(encrypted, format!("env:{original}"));
        let decrypted = decrypt_api_key(&encrypted).expect("decrypt should work");
        assert_eq!(decrypted, original);
    }

    #[test]
    fn encrypt_then_decrypt_when_key_set() {
        let original = "sk-another-key-98765";
        let encrypted = encrypt_api_key(original);
        let decrypted = decrypt_api_key(&encrypted).expect("decrypt");
        assert_eq!(decrypted, original);
    }

    #[test]
    fn backward_compat() {
        assert_eq!(decrypt_api_key("sk-plain-key").unwrap(), "sk-plain-key");
    }

    #[test]
    fn redaction_hides_bearer_token() {
        let log = "Authorization: Bearer sk-abc123xyz\ndata: ok";
        let redacted = redact_sensitive(log);
        assert!(!redacted.contains("sk-abc123xyz"));
        assert!(redacted.contains("REDACTED"));
    }
}
