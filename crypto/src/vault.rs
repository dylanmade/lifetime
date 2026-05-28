//! Sealed vault: master key encrypted with a passphrase-derived key.

use argon2::Argon2;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::CryptoError;
use crate::master_key::MasterKey;

const VAULT_VERSION: u8 = 1;
const SALT_LEN: usize = 32;
const NONCE_LEN: usize = 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealedVault {
    pub version: u8,
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
    pub fingerprint: String,
}

/// Encrypt `master_key` under a key derived from `passphrase`.
pub fn seal(master_key: &MasterKey, passphrase: &str) -> Result<SealedVault, CryptoError> {
    let mut salt = [0u8; SALT_LEN];
    rand::thread_rng().fill_bytes(&mut salt);

    let derived = derive_key(passphrase, &salt)?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);

    let cipher = XChaCha20Poly1305::new(Key::from_slice(derived.as_slice()));
    let ciphertext = cipher.encrypt(XNonce::from_slice(&nonce_bytes), master_key.as_bytes().as_slice())?;

    Ok(SealedVault {
        version: VAULT_VERSION,
        salt: B64.encode(salt),
        nonce: B64.encode(nonce_bytes),
        ciphertext: B64.encode(ciphertext),
        fingerprint: master_key.fingerprint(),
    })
}

/// Decrypt a sealed vault with the given passphrase, returning the master key.
pub fn unlock_with_passphrase(
    sealed: &SealedVault,
    passphrase: &str,
) -> Result<MasterKey, CryptoError> {
    if sealed.version != VAULT_VERSION {
        return Err(CryptoError::UnsupportedVersion);
    }

    let salt = B64.decode(&sealed.salt)?;
    let nonce_bytes = B64.decode(&sealed.nonce)?;
    let ciphertext = B64.decode(&sealed.ciphertext)?;

    if nonce_bytes.len() != NONCE_LEN {
        return Err(CryptoError::CorruptVault);
    }

    let derived = derive_key(passphrase, &salt)?;

    let cipher = XChaCha20Poly1305::new(Key::from_slice(derived.as_slice()));
    let plaintext = cipher.decrypt(XNonce::from_slice(&nonce_bytes), ciphertext.as_slice())?;

    if plaintext.len() != 32 {
        return Err(CryptoError::CorruptVault);
    }
    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&plaintext);
    let master_key = MasterKey::from_bytes(key_bytes);

    if master_key.fingerprint() != sealed.fingerprint {
        return Err(CryptoError::FingerprintMismatch);
    }

    Ok(master_key)
}

fn derive_key(passphrase: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, CryptoError> {
    let argon2 = Argon2::default();
    let mut out = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, out.as_mut_slice())
        .map_err(|_| CryptoError::KdfFailure)?;
    Ok(out)
}
