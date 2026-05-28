//! Recovery file: portable, user-held export of the master key.
//!
//! Format (two lines):
//! ```text
//! LIFETIME-RECOVERY-V1 ABCD-EFGH-IJKL
//! <base64-encoded payload>
//! ```
//!
//! Payload bytes:
//! - 1   version (u8)
//! - 8   created_at (i64, big-endian Unix timestamp seconds)
//! - 32  master key
//! - 4   CRC32 of preceding bytes (big-endian)

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use time::OffsetDateTime;

use crate::CryptoError;
use crate::master_key::MasterKey;

const HEADER_PREFIX: &str = "LIFETIME-RECOVERY-V1";
const PAYLOAD_VERSION: u8 = 1;
const PAYLOAD_LEN: usize = 1 + 8 + 32 + 4;

pub struct RecoveryFile {
    master_key: MasterKey,
    fingerprint: String,
    created_at: OffsetDateTime,
}

impl RecoveryFile {
    pub fn new(master_key: MasterKey) -> Self {
        let fingerprint = master_key.fingerprint();
        Self {
            master_key,
            fingerprint,
            created_at: OffsetDateTime::now_utc(),
        }
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }

    /// Consume the recovery file and yield the master key it carries.
    pub fn into_master_key(self) -> MasterKey {
        self.master_key
    }

    pub fn to_text(&self) -> String {
        let mut payload = Vec::with_capacity(PAYLOAD_LEN);
        payload.push(PAYLOAD_VERSION);
        payload.extend_from_slice(&self.created_at.unix_timestamp().to_be_bytes());
        payload.extend_from_slice(self.master_key.as_bytes());
        let crc = crc32fast::hash(&payload);
        payload.extend_from_slice(&crc.to_be_bytes());

        format!("{} {}\n{}", HEADER_PREFIX, self.fingerprint, B64.encode(&payload))
    }

    pub fn from_text(text: &str) -> Result<Self, CryptoError> {
        let mut lines = text.trim().lines();
        let header = lines.next().ok_or(CryptoError::InvalidRecoveryFormat)?;
        let body = lines.next().ok_or(CryptoError::InvalidRecoveryFormat)?;
        if lines.next().is_some() {
            return Err(CryptoError::InvalidRecoveryFormat);
        }

        let fingerprint = header
            .strip_prefix(HEADER_PREFIX)
            .and_then(|rest| rest.trim().split_whitespace().next())
            .ok_or(CryptoError::InvalidRecoveryFormat)?
            .to_string();

        let payload = B64.decode(body.trim())?;
        if payload.len() != PAYLOAD_LEN {
            return Err(CryptoError::InvalidRecoveryFormat);
        }

        let version = payload[0];
        if version != PAYLOAD_VERSION {
            return Err(CryptoError::UnsupportedVersion);
        }

        let ts_bytes: [u8; 8] = payload[1..9].try_into().expect("slice length");
        let ts = i64::from_be_bytes(ts_bytes);
        let created_at = OffsetDateTime::from_unix_timestamp(ts)
            .map_err(|_| CryptoError::InvalidTimestamp)?;

        let key_bytes: [u8; 32] = payload[9..41].try_into().expect("slice length");

        let stored_crc_bytes: [u8; 4] = payload[41..45].try_into().expect("slice length");
        let stored_crc = u32::from_be_bytes(stored_crc_bytes);
        let computed_crc = crc32fast::hash(&payload[..41]);
        if stored_crc != computed_crc {
            return Err(CryptoError::ChecksumMismatch);
        }

        let master_key = MasterKey::from_bytes(key_bytes);
        if master_key.fingerprint() != fingerprint {
            return Err(CryptoError::FingerprintMismatch);
        }

        Ok(Self {
            master_key,
            fingerprint,
            created_at,
        })
    }
}
