//! Multi-device sync over the append-only event log.
//!
//! Records (observations, activities, annotations) are immutable once created and
//! globally unique (`Uuid::now_v7`, time-ordered). Each node tracks, per origin
//! device, the highest id it holds — a [`VersionVector`]. Sync exchanges vectors,
//! streams the records each side is missing, and applies them idempotently. The
//! merge is additive and provenance-preserving (every record keeps its origin
//! `device_id`); convergence needs no conflict logic, because field-level edit
//! conflicts are resolved at read time by [`crate::lww`].

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::{Activity, Annotation, DeviceId, Observation};
use crate::storage::{Result, Store};

/// Highest record id held from each origin device.
pub type VersionVector = HashMap<DeviceId, Uuid>;

/// One transferable event. Externally tagged so each inner record's own
/// flatten/tag serialization is left intact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SyncRecord {
    Observation(Observation),
    Activity(Activity),
    Annotation(Annotation),
}

/// Records applied in each direction during a sync round.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncStats {
    pub a_to_b: usize,
    pub b_to_a: usize,
}

/// Bidirectional in-process sync between two stores. Phase B's network transport
/// will reuse the same `version_vector` / `records_since` / `ingest` primitives,
/// exchanging the vectors and record batches over the wire instead of in memory.
pub fn sync_once(a: &Store, b: &Store) -> Result<SyncStats> {
    let vv_a = a.version_vector()?;
    let vv_b = b.version_vector()?;
    // Compute deltas from the pre-sync vectors before either side mutates.
    let to_b = a.records_since(&vv_b)?;
    let to_a = b.records_since(&vv_a)?;
    Ok(SyncStats {
        a_to_b: b.ingest(&to_b)?,
        b_to_a: a.ingest(&to_a)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::resolve_activities;
    use crate::model::{
        ActivityKind, AnnotationTarget, AppUsageSample, ManualActivity, ObservationKind,
        activity_fields,
    };
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    use time::OffsetDateTime;
    use time::macros::datetime;

    fn id() -> Uuid {
        // Monotonic ids so "id >" ordering is deterministic in tests.
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Uuid::from_u128(COUNTER.fetch_add(1, Ordering::Relaxed) as u128)
    }

    fn obs(device: Uuid, app: &str) -> Observation {
        Observation {
            id: id(),
            device_id: device,
            recorded_at: datetime!(2026-05-25 12:00 UTC),
            kind: ObservationKind::AppUsage(AppUsageSample {
                bundle_id: None,
                app_name: app.into(),
                window_title: None,
                is_active: true,
            }),
        }
    }

    fn activity(device: Uuid, title: &str, start: OffsetDateTime) -> Activity {
        Activity {
            id: id(),
            device_id: device,
            starts_at: start,
            ends_at: Some(start + time::Duration::hours(1)),
            kind: ActivityKind::Manual(ManualActivity {
                title: title.into(),
                description: None,
            }),
        }
    }

    fn ann(
        device: Uuid,
        target: Uuid,
        field: &str,
        value: serde_json::Value,
        at: OffsetDateTime,
    ) -> Annotation {
        Annotation {
            id: id(),
            target: AnnotationTarget::Activity(target),
            field: field.into(),
            value,
            device_id: device,
            edited_at: at,
        }
    }

    fn resolved(
        store: &Store,
        start: OffsetDateTime,
        end: OffsetDateTime,
    ) -> Vec<crate::activity::ResolvedActivity> {
        let manual = store.activities_between(start, end).unwrap();
        let ids: Vec<Uuid> = manual.iter().map(|a| a.id).collect();
        let anns = store.annotations_for_activities(&ids).unwrap();
        resolve_activities(manual, vec![], anns)
    }

    #[test]
    fn bidirectional_convergence_and_idempotency() {
        let a = Store::open_in_memory().unwrap();
        let b = Store::open_in_memory().unwrap();
        let dev_a = id();
        let dev_b = id();

        a.insert_observation(&obs(dev_a, "Safari")).unwrap();
        a.insert_activity(&activity(dev_a, "Lunch", datetime!(2026-05-25 12:00 UTC)))
            .unwrap();
        b.insert_observation(&obs(dev_b, "Code")).unwrap();

        let stats = sync_once(&a, &b).unwrap();
        assert!(stats.a_to_b >= 2 && stats.b_to_a >= 1);

        // Both stores now hold the same version vector ⇒ converged.
        assert_eq!(a.version_vector().unwrap(), b.version_vector().unwrap());
        assert_eq!(a.all_observations().unwrap().len(), 2);
        assert_eq!(b.all_observations().unwrap().len(), 2);
        assert_eq!(b.all_activities().unwrap().len(), 1);

        // Second round is a no-op.
        assert_eq!(sync_once(&a, &b).unwrap(), SyncStats::default());
    }

    #[test]
    fn category_annotation_propagates() {
        let a = Store::open_in_memory().unwrap();
        let b = Store::open_in_memory().unwrap();
        let dev_a = id();

        let act = activity(dev_a, "Standup", datetime!(2026-05-25 09:00 UTC));
        a.insert_activity(&act).unwrap();
        a.insert_annotation(&ann(
            dev_a,
            act.id,
            activity_fields::CATEGORY,
            json!("Meetings"),
            datetime!(2026-05-25 09:05 UTC),
        ))
        .unwrap();

        sync_once(&a, &b).unwrap();

        let day = resolved(
            &b,
            datetime!(2026-05-25 00:00 UTC),
            datetime!(2026-05-26 00:00 UTC),
        );
        assert_eq!(day.len(), 1);
        assert_eq!(day[0].category.as_deref(), Some("Meetings"));
    }

    #[test]
    fn tombstone_propagates() {
        let a = Store::open_in_memory().unwrap();
        let b = Store::open_in_memory().unwrap();
        let dev_a = id();

        let act = activity(dev_a, "Mistake", datetime!(2026-05-25 09:00 UTC));
        a.insert_activity(&act).unwrap();
        a.insert_annotation(&ann(
            dev_a,
            act.id,
            activity_fields::DELETED,
            json!(true),
            datetime!(2026-05-25 09:05 UTC),
        ))
        .unwrap();

        sync_once(&a, &b).unwrap();

        // The activity row syncs, but resolution hides it.
        assert_eq!(b.all_activities().unwrap().len(), 1);
        assert!(
            resolved(
                &b,
                datetime!(2026-05-25 00:00 UTC),
                datetime!(2026-05-26 00:00 UTC),
            )
            .is_empty()
        );
    }

    #[test]
    fn time_edit_reconciles_window_on_peer() {
        let a = Store::open_in_memory().unwrap();
        let b = Store::open_in_memory().unwrap();
        let dev_a = id();

        // Created on day 1...
        let act = activity(dev_a, "Moved", datetime!(2026-05-20 12:00 UTC));
        a.insert_activity(&act).unwrap();
        // ...edited to day 2 via time annotations.
        a.insert_annotation(&ann(
            dev_a,
            act.id,
            activity_fields::STARTS_AT,
            json!("2026-05-25T09:00:00Z"),
            datetime!(2026-05-25 08:00 UTC),
        ))
        .unwrap();
        a.insert_annotation(&ann(
            dev_a,
            act.id,
            activity_fields::ENDS_AT,
            json!("2026-05-25T10:00:00Z"),
            datetime!(2026-05-25 08:00 UTC),
        ))
        .unwrap();

        sync_once(&a, &b).unwrap();

        // On B, the index columns were reconciled to day 2.
        assert_eq!(
            b.activities_between(
                datetime!(2026-05-25 00:00 UTC),
                datetime!(2026-05-26 00:00 UTC),
            )
            .unwrap()
            .len(),
            1
        );
        assert!(
            b.activities_between(
                datetime!(2026-05-20 00:00 UTC),
                datetime!(2026-05-21 00:00 UTC),
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn three_way_convergence_through_a_hub() {
        // A and C never sync directly; both sync with B, then B carries C's record to A.
        let a = Store::open_in_memory().unwrap();
        let b = Store::open_in_memory().unwrap();
        let c = Store::open_in_memory().unwrap();

        a.insert_observation(&obs(id(), "A-app")).unwrap();
        c.insert_observation(&obs(id(), "C-app")).unwrap();

        sync_once(&c, &b).unwrap(); // B learns C's record
        sync_once(&a, &b).unwrap(); // A and B exchange (A gets C's via B)

        assert_eq!(a.all_observations().unwrap().len(), 2);
        assert_eq!(b.all_observations().unwrap().len(), 2);
    }
}
