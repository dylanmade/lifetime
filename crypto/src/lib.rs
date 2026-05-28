//! Cryptographic primitives for Lifetime.
//!
//! The model is two-layered:
//! - A 32-byte [`MasterKey`] is the user's root of trust. All purpose-specific
//!   keys (SQLCipher, sync envelopes, pairing) derive from it via HKDF.
//! - The MasterKey is sealed inside a [`vault::SealedVault`] using a key
//!   derived from the user's passphrase via Argon2id. The encrypted vault is
//!   what lives on disk on every device.
//! - A [`RecoveryFile`] holds the raw MasterKey in a portable text format and
//!   is the only escape hatch if the passphrase is lost.

pub mod master_key;
pub mod recovery;
pub mod vault;

pub use master_key::MasterKey;
pub use recovery::RecoveryFile;
pub use vault::SealedVault;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("unsupported version")]
    UnsupportedVersion,

    #[error("passphrase derivation failed")]
    KdfFailure,

    #[error("decryption failed (wrong passphrase or corrupted data)")]
    DecryptionFailed,

    #[error("invalid base64 encoding")]
    InvalidEncoding,

    #[error("vault data is malformed")]
    CorruptVault,

    #[error("fingerprint mismatch: key does not match expected identity")]
    FingerprintMismatch,

    #[error("invalid recovery file format")]
    InvalidRecoveryFormat,

    #[error("recovery file checksum mismatch")]
    ChecksumMismatch,

    #[error("recovery file timestamp is invalid")]
    InvalidTimestamp,
}

impl From<chacha20poly1305::Error> for CryptoError {
    fn from(_: chacha20poly1305::Error) -> Self {
        Self::DecryptionFailed
    }
}

impl From<base64::DecodeError> for CryptoError {
    fn from(_: base64::DecodeError) -> Self {
        Self::InvalidEncoding
    }
}
