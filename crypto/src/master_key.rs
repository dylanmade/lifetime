use hkdf::Hkdf;
use rand::RngCore;
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct MasterKey([u8; 32]);

impl MasterKey {
    /// Generate a fresh master key from the OS RNG.
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        Self(bytes)
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// User-facing identity for this key: 12 hex digits of `SHA-256(key)`,
    /// formatted as `ABCD-EFGH-IJKL`. Suitable for sanity-checking that two
    /// devices hold the same key.
    pub fn fingerprint(&self) -> String {
        let hash = Sha256::digest(self.0);
        let hex: String = hash[..6].iter().map(|b| format!("{b:02X}")).collect();
        format!("{}-{}-{}", &hex[..4], &hex[4..8], &hex[8..12])
    }

    /// HKDF-SHA256 subkey for a specific purpose.
    ///
    /// `info` should be a stable bytestring naming the use case
    /// (e.g. `b"lifetime/sqlcipher/v1"`, `b"lifetime/sync-envelope/v1"`).
    pub fn derive_subkey(&self, info: &[u8], length: usize) -> Vec<u8> {
        let hk = Hkdf::<Sha256>::new(None, &self.0);
        let mut out = vec![0u8; length];
        hk.expand(info, &mut out)
            .expect("HKDF expand length within bounds");
        out
    }
}
