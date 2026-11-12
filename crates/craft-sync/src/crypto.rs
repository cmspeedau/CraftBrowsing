use argon2::Argon2;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

pub struct SyncCipher {
    cipher: ChaCha20Poly1305,
}

impl SyncCipher {
    /// Derive a 256-bit key from a user passphrase and salt using Argon2id
    pub fn from_passphrase(passphrase: &[u8], salt: &[u8]) -> Result<Self, String> {
        let mut key_bytes = [0u8; 32];
        let argon2 = Argon2::default();
        argon2
            .hash_password_into(passphrase, salt, &mut key_bytes)
            .map_err(|e| format!("Argon2 derivation error: {e}"))?;

        let key = Key::from_slice(&key_bytes);
        Ok(Self {
            cipher: ChaCha20Poly1305::new(key),
        })
    }

    pub fn encrypt(&self, nonce: &[u8; 12], plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let nonce = Nonce::from_slice(nonce);
        self.cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| format!("Encryption failure: {e}"))
    }

    pub fn decrypt(&self, nonce: &[u8; 12], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        let nonce = Nonce::from_slice(nonce);
        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| format!("Decryption failure: {e}"))
    }
}
