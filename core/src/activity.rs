//! The unified activity read-model.
//!
//! An *activity* is the core building block surfaced to the UI. Two sources
//! feed it:
//! - **Manual** — user-created [`crate::model::Activity`] rows.
//! - **Auto** — app-usage runs derived from observations by
//!   [`crate::aggregate::aggregate_into_segments`]. These are *not* persisted;
//!   each run is identified by [`auto_activity_id`] (a stable UUIDv5 of its
//!   originating observation), so edits can attach to it.
//!
//! Edits live as [`crate::model::Annotation`] records resolved last-write-wins
//! ([`crate::lww::resolve`]). This module overlays those resolved annotations
//! onto each base activity, drops tombstoned ones, and returns a single sorted
//! [`ResolvedActivity`] list.

use std::collections::HashMap;

use serde::Serialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::aggregate::AppSegment;
use crate::lww::{AnnotationKey, resolve};
use crate::model::{
    Activity, ActivityKind, ActivitySource, Annotation, AnnotationTarget, activity_fields as f,
};

/// Namespace for deriving stable auto-activity ids (16 bytes).
const AUTO_ACTIVITY_NAMESPACE: Uuid = Uuid::from_bytes(*b"lifetime-autoact");

/// Stable id for the auto-activity derived from an observation run. Pure
/// function of the originating observation's id, so it survives re-derivation.
pub fn auto_activity_id(origin_observation_id: Uuid) -> Uuid {
    Uuid::new_v5(&AUTO_ACTIVITY_NAMESPACE, origin_observation_id.as_bytes())
}

/// A fully-resolved activity ready for the UI: base record with its
/// last-write-wins annotations overlaid. Auto-only fields are `None` for
/// manual activities and vice-versa.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResolvedActivity {
    pub id: Uuid,
    pub source: ActivitySource,
    #[serde(with = "time::serde::rfc3339")]
    pub starts_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub ends_at: Option<OffsetDateTime>,
    pub title: String,
    /// `None` ⇒ Unassigned.
    pub category: Option<String>,
    pub description: Option<String>,
    // Auto-only measured fields:
    pub app_name: Option<String>,
    pub bundle_id: Option<String>,
    pub window_title: Option<String>,
    pub is_active: Option<bool>,
}

/// Merge manual rows and derived auto runs with their resolved annotations into
/// one sorted activity list, dropping tombstoned (deleted) activities.
///
/// `annotations` should cover every relevant activity id — both the manual row
/// ids and the [`auto_activity_id`]s of the segments.
pub fn resolve_activities(
    manual: Vec<Activity>,
    segments: Vec<AppSegment>,
    annotations: Vec<Annotation>,
) -> Vec<ResolvedActivity> {
    let resolved = resolve(annotations);
    let mut out = Vec::with_capacity(manual.len() + segments.len());

    for act in manual {
        let id = act.id;
        if is_deleted(&resolved, id) {
            continue;
        }
        let ActivityKind::Manual(m) = &act.kind;
        out.push(ResolvedActivity {
            id,
            source: ActivitySource::Manual,
            starts_at: time_field(&resolved, id, f::STARTS_AT).unwrap_or(act.starts_at),
            ends_at: match field(&resolved, id, f::ENDS_AT) {
                Some(v) => parse_time(v),
                None => act.ends_at,
            },
            title: string_field(&resolved, id, f::TITLE).unwrap_or_else(|| m.title.clone()),
            category: string_field(&resolved, id, f::CATEGORY),
            description: match field(&resolved, id, f::DESCRIPTION) {
                Some(v) => v.as_str().map(str::to_string),
                None => m.description.clone(),
            },
            app_name: None,
            bundle_id: None,
            window_title: None,
            is_active: None,
        });
    }

    for seg in segments {
        let id = auto_activity_id(seg.origin_observation_id);
        if is_deleted(&resolved, id) {
            continue;
        }
        out.push(ResolvedActivity {
            id,
            source: ActivitySource::Auto,
            starts_at: seg.starts_at,
            ends_at: Some(seg.ends_at),
            title: seg.app_name.clone(),
            category: string_field(&resolved, id, f::CATEGORY),
            description: string_field(&resolved, id, f::DESCRIPTION),
            app_name: Some(seg.app_name),
            bundle_id: seg.bundle_id,
            window_title: seg.window_title,
            is_active: Some(seg.is_active),
        });
    }

    out.sort_by_key(|a| a.starts_at);
    out
}

type Resolved = HashMap<AnnotationKey, Annotation>;

fn field<'a>(resolved: &'a Resolved, id: Uuid, name: &str) -> Option<&'a serde_json::Value> {
    resolved
        .get(&AnnotationKey {
            target: AnnotationTarget::Activity(id),
            field: name.to_string(),
        })
        .map(|a| &a.value)
}

/// A resolved string field, treating JSON `null` (an explicit clear) as absent.
fn string_field(resolved: &Resolved, id: Uuid, name: &str) -> Option<String> {
    field(resolved, id, name).and_then(|v| v.as_str().map(str::to_string))
}

fn time_field(resolved: &Resolved, id: Uuid, name: &str) -> Option<OffsetDateTime> {
    field(resolved, id, name).and_then(parse_time)
}

fn is_deleted(resolved: &Resolved, id: Uuid) -> bool {
    field(resolved, id, f::DELETED)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

fn parse_time(value: &serde_json::Value) -> Option<OffsetDateTime> {
    value
        .as_str()
        .and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ManualActivity;
    use serde_json::json;
    use time::macros::datetime;

    fn manual(id: u128, title: &str) -> Activity {
        Activity {
            id: Uuid::from_u128(id),
            device_id: Uuid::nil(),
            starts_at: datetime!(2026-05-27 09:00 UTC),
            ends_at: Some(datetime!(2026-05-27 10:00 UTC)),
            kind: ActivityKind::Manual(ManualActivity {
                title: title.to_string(),
                description: None,
            }),
        }
    }

    fn ann(target_id: Uuid, field: &str, value: serde_json::Value, edited_at: OffsetDateTime) -> Annotation {
        Annotation {
            id: Uuid::new_v4(),
            target: AnnotationTarget::Activity(target_id),
            field: field.to_string(),
            value,
            device_id: Uuid::nil(),
            edited_at,
        }
    }

    fn segment(origin: Uuid, app: &str) -> AppSegment {
        AppSegment {
            app_name: app.to_string(),
            bundle_id: Some(format!("com.example.{app}")),
            starts_at: datetime!(2026-05-27 11:00 UTC),
            ends_at: datetime!(2026-05-27 11:30 UTC),
            is_active: true,
            origin_observation_id: origin,
            window_title: Some("a window".into()),
        }
    }

    #[test]
    fn manual_overlays_title_and_category() {
        let act = manual(1, "Original");
        let anns = vec![
            ann(act.id, f::TITLE, json!("Edited"), datetime!(2026-05-27 12:00 UTC)),
            ann(act.id, f::CATEGORY, json!("Work"), datetime!(2026-05-27 12:00 UTC)),
        ];
        let out = resolve_activities(vec![act], vec![], anns);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].title, "Edited");
        assert_eq!(out[0].category.as_deref(), Some("Work"));
        assert_eq!(out[0].source, ActivitySource::Manual);
    }

    #[test]
    fn newer_annotation_wins() {
        let act = manual(2, "x");
        let anns = vec![
            ann(act.id, f::CATEGORY, json!("Old"), datetime!(2026-05-27 12:00 UTC)),
            ann(act.id, f::CATEGORY, json!("New"), datetime!(2026-05-27 13:00 UTC)),
        ];
        let out = resolve_activities(vec![act], vec![], anns);
        assert_eq!(out[0].category.as_deref(), Some("New"));
    }

    #[test]
    fn null_category_clears_to_unassigned() {
        let act = manual(3, "x");
        let anns = vec![
            ann(act.id, f::CATEGORY, json!("Work"), datetime!(2026-05-27 12:00 UTC)),
            ann(act.id, f::CATEGORY, json!(null), datetime!(2026-05-27 13:00 UTC)),
        ];
        let out = resolve_activities(vec![act], vec![], anns);
        assert_eq!(out[0].category, None);
    }

    #[test]
    fn tombstone_hides_activity() {
        let act = manual(4, "x");
        let anns = vec![ann(act.id, f::DELETED, json!(true), datetime!(2026-05-27 12:00 UTC))];
        let out = resolve_activities(vec![act], vec![], anns);
        assert!(out.is_empty());
    }

    #[test]
    fn manual_time_override_applies() {
        let act = manual(5, "x");
        let anns = vec![ann(
            act.id,
            f::STARTS_AT,
            json!("2026-05-27T08:00:00Z"),
            datetime!(2026-05-27 12:00 UTC),
        )];
        let out = resolve_activities(vec![act], vec![], anns);
        assert_eq!(out[0].starts_at, datetime!(2026-05-27 08:00 UTC));
    }

    #[test]
    fn auto_id_is_stable_and_distinct() {
        let a = Uuid::from_u128(100);
        let b = Uuid::from_u128(200);
        assert_eq!(auto_activity_id(a), auto_activity_id(a));
        assert_ne!(auto_activity_id(a), auto_activity_id(b));
    }

    #[test]
    fn auto_activity_category_overlay_by_derived_id() {
        let origin = Uuid::from_u128(300);
        let seg = segment(origin, "Safari");
        let aid = auto_activity_id(origin);
        let anns = vec![ann(aid, f::CATEGORY, json!("Browsing"), datetime!(2026-05-27 12:00 UTC))];
        let out = resolve_activities(vec![], vec![seg], anns);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, ActivitySource::Auto);
        assert_eq!(out[0].id, aid);
        assert_eq!(out[0].title, "Safari");
        assert_eq!(out[0].app_name.as_deref(), Some("Safari"));
        assert_eq!(out[0].category.as_deref(), Some("Browsing"));
    }

    #[test]
    fn auto_activity_tombstone_hides() {
        let origin = Uuid::from_u128(400);
        let seg = segment(origin, "Code");
        let aid = auto_activity_id(origin);
        let anns = vec![ann(aid, f::DELETED, json!(true), datetime!(2026-05-27 12:00 UTC))];
        let out = resolve_activities(vec![], vec![seg], anns);
        assert!(out.is_empty());
    }

    #[test]
    fn merged_output_sorted_by_start() {
        let act = manual(6, "Manual at 9"); // starts 09:00
        let seg = segment(Uuid::from_u128(500), "Auto at 11"); // starts 11:00
        let out = resolve_activities(vec![act], vec![seg], vec![]);
        assert_eq!(out.len(), 2);
        assert!(out[0].starts_at < out[1].starts_at);
        assert_eq!(out[0].source, ActivitySource::Manual);
        assert_eq!(out[1].source, ActivitySource::Auto);
    }
}
