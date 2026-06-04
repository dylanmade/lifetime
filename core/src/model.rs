//! Core data model for Lifetime.
//!
//! Three record kinds:
//! - [`Observation`] — immutable, append-only samples from passive tracking.
//! - [`Activity`]    — user-created or persisted time-ranged entries.
//! - [`Annotation`]  — last-write-wins edits to observations or activities.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

pub type DeviceId = Uuid;
pub type ObservationId = Uuid;
pub type ActivityId = Uuid;
pub type AnnotationId = Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub id: ObservationId,
    pub device_id: DeviceId,
    #[serde(with = "time::serde::rfc3339")]
    pub recorded_at: OffsetDateTime,
    #[serde(flatten)]
    pub kind: ObservationKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObservationKind {
    AppUsage(AppUsageSample),
    Location(LocationSample),
    Idle(IdleSample),
    DeviceState(DeviceStateSample),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppUsageSample {
    pub bundle_id: Option<String>,
    pub app_name: String,
    pub window_title: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocationSample {
    pub latitude: f64,
    pub longitude: f64,
    pub accuracy_meters: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdleSample {
    pub idle_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceStateSample {
    pub state: DeviceState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    Awake,
    Asleep,
    Locked,
    ScreenOff,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    pub id: ActivityId,
    pub device_id: DeviceId,
    #[serde(with = "time::serde::rfc3339")]
    pub starts_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub ends_at: Option<OffsetDateTime>,
    #[serde(flatten)]
    pub kind: ActivityKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivityKind {
    Manual(ManualActivity),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualActivity {
    pub title: String,
    pub description: Option<String>,
}

/// Whether an activity was tracked automatically (derived from observations) or
/// defined by the user. Carried on the resolved read-model, not stored — auto
/// activities are derived, not persisted as rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivitySource {
    Manual,
    Auto,
}

/// Annotation `field` names used for last-write-wins edits to activities.
/// Centralized so storage, resolution, and the command layer agree on the
/// exact strings. An absent annotation means "unedited" (e.g. no `CATEGORY`
/// annotation ⇒ Unassigned); a `DELETED` annotation with value `true` is a
/// tombstone that hides the activity.
pub mod activity_fields {
    pub const TITLE: &str = "title";
    pub const DESCRIPTION: &str = "description";
    pub const CATEGORY: &str = "category";
    pub const STARTS_AT: &str = "starts_at";
    pub const ENDS_AT: &str = "ends_at";
    pub const DELETED: &str = "deleted";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: AnnotationId,
    #[serde(flatten)]
    pub target: AnnotationTarget,
    pub field: String,
    pub value: serde_json::Value,
    pub device_id: DeviceId,
    #[serde(with = "time::serde::rfc3339")]
    pub edited_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "target_kind", content = "target_id", rename_all = "snake_case")]
pub enum AnnotationTarget {
    Observation(ObservationId),
    Activity(ActivityId),
}
