//! Random password generator for the Tools page.
use anyhow::{Context as _, Result};

pub const PASSWORD_LENGTH: usize = 24;

/// 64 distinct characters: every six-bit value is equally likely, so there is
/// no modulo bias. No character-class repair, predictable seed or logging.
pub fn encode_password(random: &[u8; PASSWORD_LENGTH]) -> [u8; PASSWORD_LENGTH] {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    random.map(|byte| ALPHABET[(byte & 63) as usize])
}

fn wipe(bytes: &mut [u8]) {
    for byte in bytes {
        // SAFETY: `byte` is a valid, exclusive reference.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
}

pub struct Secret([u8; PASSWORD_LENGTH]);

impl Secret {
    pub fn generate() -> Result<Self> {
        use rand::RngCore;
        let mut random = [0u8; PASSWORD_LENGTH];
        let filled = rand::rngs::OsRng
            .try_fill_bytes(&mut random)
            .map_err(|e| anyhow::anyhow!("{e}"));
        let encoded = encode_password(&random);
        wipe(&mut random);
        filled.context("Password generation failed")?;
        Ok(Self(encoded))
    }
    pub fn reveal(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<hidden>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_encoding_has_at_least_24_unbiased_characters() {
        const { assert!(PASSWORD_LENGTH >= 24) };
        let mut counts = std::collections::HashMap::new();
        for byte in 0..=u8::MAX {
            let encoded = encode_password(&[byte; PASSWORD_LENGTH]);
            assert_eq!(encoded.len(), PASSWORD_LENGTH);
            assert!(encoded
                .iter()
                .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(c)));
            *counts.entry(encoded[0]).or_insert(0) += 1;
        }
        assert_eq!(counts.len(), 64);
        assert!(counts.values().all(|count| *count == 4));
    }

    #[test]
    fn generated_passwords_differ_and_never_debug_print() {
        let a = Secret::generate().unwrap();
        let b = Secret::generate().unwrap();
        assert_eq!(a.reveal().len(), PASSWORD_LENGTH);
        assert_ne!(a.reveal(), b.reveal());
        assert!(!format!("{a:?}").contains(a.reveal()));
    }
}
