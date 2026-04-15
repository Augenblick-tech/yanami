use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use domain::shared::error::DomainError;
use sha2::{Digest, Sha256};

const SECRET_PREFIX: &str = "enc:v1:";
const NONCE_LEN: usize = 12;

/// 使用应用配置密钥保护外部系统凭证。
#[derive(Clone)]
pub struct SecretProtector {
    cipher: Aes256Gcm,
}

impl SecretProtector {
    pub fn new(application_key: &str) -> Result<Self, DomainError> {
        if application_key.trim().is_empty() {
            return Err(DomainError::InvariantViolation(
                "application key cannot be empty",
            ));
        }

        let digest = Sha256::digest(format!("yanami:external-secret:{application_key}"));
        let cipher = Aes256Gcm::new_from_slice(&digest)
            .map_err(|error| DomainError::external("secret protector init failed", error))?;

        Ok(Self { cipher })
    }

    /// 将明文保护成可持久化的密文。
    pub fn seal(&self, plaintext: &str) -> Result<String, DomainError> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|error| {
                DomainError::external("secret encryption failed", anyhow::anyhow!("{error}"))
            })?;

        let mut payload = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        payload.extend_from_slice(nonce.as_slice());
        payload.extend_from_slice(&ciphertext);
        Ok(format!("{SECRET_PREFIX}{}", STANDARD.encode(payload)))
    }

    /// 读取持久化值；新格式解密，旧格式按历史明文兼容。
    pub fn open(&self, persisted: &str) -> Result<String, DomainError> {
        if !persisted.starts_with(SECRET_PREFIX) {
            return Ok(persisted.to_string());
        }

        let payload = STANDARD
            .decode(&persisted[SECRET_PREFIX.len()..])
            .map_err(|error| DomainError::external("secret payload decode failed", error))?;
        if payload.len() < NONCE_LEN {
            return Err(DomainError::InvariantViolation(
                "encrypted secret payload is invalid",
            ));
        }

        let (nonce_bytes, ciphertext) = payload.split_at(NONCE_LEN);
        let plaintext = self
            .cipher
            .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
            .map_err(|error| {
                DomainError::external("secret decryption failed", anyhow::anyhow!("{error}"))
            })?;

        String::from_utf8(plaintext)
            .map_err(|error| DomainError::external("secret plaintext decode failed", error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_round_trip_encrypts_and_decrypts() {
        let protector = SecretProtector::new("test-key").expect("protector");
        let sealed = protector.seal("secret").expect("seal");
        let opened = protector.open(&sealed).expect("open");

        assert_ne!(sealed, "secret");
        assert!(sealed.starts_with(SECRET_PREFIX));
        assert_eq!(opened, "secret");
    }

    #[test]
    fn secret_open_accepts_legacy_plaintext() {
        let protector = SecretProtector::new("test-key").expect("protector");

        let opened = protector.open("legacy-secret").expect("open");

        assert_eq!(opened, "legacy-secret");
    }

    #[test]
    fn secret_new_rejects_empty_key_and_open_rejects_bad_payload() {
        let key_error = SecretProtector::new("  ").err().expect("empty key");
        let protector = SecretProtector::new("test-key").expect("protector");
        let payload_error = protector
            .open("enc:v1:not-base64")
            .expect_err("invalid payload");

        assert_eq!(
            key_error.to_string(),
            "domain invariant violation: application key cannot be empty"
        );
        assert!(payload_error
            .to_string()
            .contains("secret payload decode failed"));
    }

    #[test]
    fn secret_open_rejects_wrong_key_ciphertext() {
        let first = SecretProtector::new("test-key").expect("protector");
        let second = SecretProtector::new("other-key").expect("protector");
        let sealed = first.seal("secret").expect("seal");

        let error = second.open(&sealed).expect_err("wrong key must fail");

        assert!(error.to_string().contains("secret decryption failed"));
    }
}
