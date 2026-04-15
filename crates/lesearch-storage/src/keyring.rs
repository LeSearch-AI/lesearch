//! Ed25519 keyring for signing session events.
//!
//! Keys are stored as raw 32-byte files in `$LESEARCH_HOME/.keys/`.
//! Permissions are set to `0o600` on Unix.

use std::path::Path;

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

/// Ed25519 keyring for the daemon.
///
/// Holds the signing key in memory. The key is zeroized on drop
/// (provided by `ed25519-dalek`).
#[derive(Clone)]
pub struct Keyring {
    signing_key: SigningKey,
}

impl std::fmt::Debug for Keyring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keyring")
            .field("pubkey", &self.public_key_hex())
            .finish()
    }
}

impl Keyring {
    /// Generate a fresh Ed25519 keypair.
    #[must_use]
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    /// Load the keypair from `keys_dir/signing.key`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Io`] if the file is missing or unreadable,
    /// or [`crate::StorageError::Signing`] if the key bytes are invalid.
    pub fn load(keys_dir: &Path) -> Result<Self, crate::StorageError> {
        let key_path = keys_dir.join("signing.key");
        let bytes = std::fs::read(&key_path)?;
        let key_bytes: [u8; 32] = bytes.try_into().map_err(|_| {
            crate::StorageError::Signing(format!(
                "signing.key must be 32 bytes, got {}",
                std::fs::metadata(&key_path)
                    .map(|m| m.len())
                    .unwrap_or(0)
            ))
        })?;
        Ok(Self {
            signing_key: SigningKey::from_bytes(&key_bytes),
        })
    }

    /// Save the keypair to `keys_dir/signing.key` and `signing.pub`.
    ///
    /// Creates the directory and sets file permissions to `0o600` on Unix.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Io`] on filesystem failures.
    pub fn save(&self, keys_dir: &Path) -> Result<(), crate::StorageError> {
        std::fs::create_dir_all(keys_dir)?;

        let key_path = keys_dir.join("signing.key");
        let pub_path = keys_dir.join("signing.pub");

        std::fs::write(&key_path, self.signing_key.to_bytes())?;
        std::fs::write(&pub_path, self.signing_key.verifying_key().to_bytes())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&key_path, perms.clone())?;
            std::fs::set_permissions(&pub_path, perms)?;
        }

        tracing::info!(?keys_dir, "keyring saved");
        Ok(())
    }

    /// Load from `keys_dir` if keys exist, otherwise generate and save.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError`] on I/O or key format errors.
    pub fn load_or_generate(keys_dir: &Path) -> Result<Self, crate::StorageError> {
        if keys_dir.join("signing.key").exists() {
            Self::load(keys_dir)
        } else {
            let kr = Self::generate();
            kr.save(keys_dir)?;
            Ok(kr)
        }
    }

    // -- Signing / verification -------------------------------------------

    /// Sign arbitrary bytes. Returns `"ed25519:{hex}"`.
    #[must_use]
    pub fn sign(&self, data: &[u8]) -> String {
        let sig = self.signing_key.sign(data);
        format!("ed25519:{}", hex::encode(sig.to_bytes()))
    }

    /// Hex-encoded public key prefixed with `"ed25519:"`.
    #[must_use]
    pub fn public_key_hex(&self) -> String {
        format!(
            "ed25519:{}",
            hex::encode(self.signing_key.verifying_key().to_bytes())
        )
    }

    /// Compute `SHA-256` over `data` and return `"sha256:{hex}"`.
    #[must_use]
    pub fn hash(data: &[u8]) -> String {
        let h = Sha256::digest(data);
        format!("sha256:{}", hex::encode(h))
    }

    /// Verify an `"ed25519:{hex}"` signature against `data` and `pubkey_hex`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::VerificationFailed`] on mismatch or
    /// malformed inputs.
    pub fn verify(data: &[u8], signature_str: &str, pubkey_str: &str) -> Result<(), crate::StorageError> {
        let sig_hex = signature_str
            .strip_prefix("ed25519:")
            .ok_or_else(|| crate::StorageError::VerificationFailed("bad signature prefix".into()))?;
        let sig_bytes: [u8; 64] = hex::decode(sig_hex)
            .map_err(|e| crate::StorageError::VerificationFailed(format!("sig hex: {e}")))?
            .try_into()
            .map_err(|_| crate::StorageError::VerificationFailed("sig must be 64 bytes".into()))?;

        let pub_hex = pubkey_str
            .strip_prefix("ed25519:")
            .ok_or_else(|| crate::StorageError::VerificationFailed("bad pubkey prefix".into()))?;
        let pub_bytes: [u8; 32] = hex::decode(pub_hex)
            .map_err(|e| crate::StorageError::VerificationFailed(format!("pubkey hex: {e}")))?
            .try_into()
            .map_err(|_| crate::StorageError::VerificationFailed("pubkey must be 32 bytes".into()))?;

        let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);
        let verifying_key = VerifyingKey::from_bytes(&pub_bytes)
            .map_err(|e| crate::StorageError::VerificationFailed(format!("pubkey: {e}")))?;

        verifying_key
            .verify(data, &signature)
            .map_err(|e| crate::StorageError::VerificationFailed(format!("signature invalid: {e}")))
    }

    /// Verify a `"sha256:{hex}"` hash against computed hash of `data`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::VerificationFailed`] on mismatch.
    pub fn verify_hash(data: &[u8], expected: &str) -> Result<(), crate::StorageError> {
        let actual = Self::hash(data);
        if actual == expected {
            Ok(())
        } else {
            Err(crate::StorageError::VerificationFailed(format!(
                "hash mismatch: expected {expected}, got {actual}"
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_sign_verify() {
        let kr = Keyring::generate();
        let data = b"hello world";
        let sig = kr.sign(data);
        let pubkey = kr.public_key_hex();

        assert!(sig.starts_with("ed25519:"));
        assert!(pubkey.starts_with("ed25519:"));

        Keyring::verify(data, &sig, &pubkey).expect("verification should pass");
    }

    #[test]
    fn verify_wrong_data_fails() {
        let kr = Keyring::generate();
        let sig = kr.sign(b"correct");
        let pubkey = kr.public_key_hex();

        let result = Keyring::verify(b"wrong", &sig, &pubkey);
        assert!(result.is_err());
    }

    #[test]
    fn hash_deterministic() {
        let a = Keyring::hash(b"test");
        let b = Keyring::hash(b"test");
        assert_eq!(a, b);
        assert!(a.starts_with("sha256:"));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join("lesearch-keyring-test");
        let _ = std::fs::remove_dir_all(&dir);

        let kr1 = Keyring::generate();
        kr1.save(&dir).unwrap();

        let kr2 = Keyring::load(&dir).unwrap();
        assert_eq!(kr1.public_key_hex(), kr2.public_key_hex());

        // Sign with kr1, verify with kr2's pubkey
        let sig = kr1.sign(b"data");
        Keyring::verify(b"data", &sig, &kr2.public_key_hex()).unwrap();

        let _ = std::fs::remove_dir_all(&dir);
    }
}
