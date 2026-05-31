//! Theme profiles — saved snapshots of the appearance editor's state.
//!
//! The `data` field is opaque on the Rust side: we store it as a JSON string
//! and let the frontend own the schema. That keeps the editor's data model
//! free to evolve without touching this crate or its tests.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeProfile {
    pub id: Uuid,
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    /// JSON-serialized appearance state (mode, fonts, radius, color overrides).
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeProfileSummary {
    pub id: Uuid,
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}
