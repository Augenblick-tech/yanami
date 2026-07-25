use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::URL_SAFE, Engine};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};

use crate::entity::cap::CryptoProvider;

pub struct AesCryptoProvider {
    key: Key<Aes256Gcm>,
}

impl AesCryptoProvider {
    pub fn new(secret: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(secret.as_bytes());
        let hash = hasher.finalize();
        let key = hash;
        Self { key }
    }
}

impl CryptoProvider for AesCryptoProvider {
    fn encrypt(&self, plain: &str) -> Result<String> {
        let cipher = Aes256Gcm::new(&self.key);
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from(nonce_bytes);

        let ciphertext = cipher
            .encrypt(&nonce, plain.as_bytes())
            .map_err(|e| anyhow!("encryption failed: {}", e))?;

        let encoded_nonce = URL_SAFE.encode(nonce_bytes);
        let encoded_cipher = URL_SAFE.encode(ciphertext);

        Ok(format!("{}:{}", encoded_nonce, encoded_cipher))
    }

    fn decrypt(&self, cipher: &str) -> Result<String> {
        let parts: Vec<&str> = cipher.split(':').collect();
        if parts.len() != 2 {
            return Err(anyhow!("invalid cipher format"));
        }

        let nonce_bytes = URL_SAFE
            .decode(parts[0])
            .map_err(|e| anyhow!("decode nonce failed: {}", e))?;
        let cipher_bytes = URL_SAFE
            .decode(parts[1])
            .map_err(|e| anyhow!("decode cipher failed: {}", e))?;

        let cipher_alg = Aes256Gcm::new(&self.key);
        let nonce = Nonce::try_from(nonce_bytes.as_slice())
            .map_err(|_| anyhow!("invalid nonce length"))?;

        let plaintext_bytes = cipher_alg
            .decrypt(&nonce, cipher_bytes.as_ref())
            .map_err(|e| anyhow!("decryption failed: {}", e))?;

        String::from_utf8(plaintext_bytes).map_err(|e| anyhow!("invalid utf8 in plaintext: {}", e))
    }
}
