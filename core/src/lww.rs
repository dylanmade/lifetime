//! Last-write-wins resolution for annotations.
//!
//! For each `(target, field)` pair, the annotation with the latest
//! `edited_at` wins. Ties are broken by `device_id` ordering so the
//! result is deterministic regardless of input order.

use std::collections::HashMap;

use crate::model::{Annotation, AnnotationTarget};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnnotationKey {
    pub target: AnnotationTarget,
    pub field: String,
}

pub fn resolve(
    annotations: impl IntoIterator<Item = Annotation>,
) -> HashMap<AnnotationKey, Annotation> {
    let mut winners: HashMap<AnnotationKey, Annotation> = HashMap::new();
    for candidate in annotations {
        let key = AnnotationKey {
            target: candidate.target.clone(),
            field: candidate.field.clone(),
        };
        match winners.get(&key) {
            None => {
                winners.insert(key, candidate);
            }
            Some(current) if is_newer(&candidate, current) => {
                winners.insert(key, candidate);
            }
            _ => {}
        }
    }
    winners
}

fn is_newer(candidate: &Annotation, current: &Annotation) -> bool {
    match candidate.edited_at.cmp(&current.edited_at) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => candidate.device_id > current.device_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use time::OffsetDateTime;
    use time::macros::datetime;
    use uuid::Uuid;

    fn ann(
        target: AnnotationTarget,
        field: &str,
        value: serde_json::Value,
        device_id: Uuid,
        edited_at: OffsetDateTime,
    ) -> Annotation {
        Annotation {
            id: Uuid::from_u128(rand_u128()),
            target,
            field: field.to_string(),
            value,
            device_id,
            edited_at,
        }
    }

    fn rand_u128() -> u128 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        COUNTER.fetch_add(1, Ordering::Relaxed) as u128
    }

    #[test]
    fn later_edit_wins() {
        let target = AnnotationTarget::Activity(Uuid::from_u128(100));
        let device_a = Uuid::from_u128(1);
        let device_b = Uuid::from_u128(2);

        let earlier = ann(
            target.clone(),
            "category",
            json!("old"),
            device_a,
            datetime!(2026-01-01 12:00 UTC),
        );
        let later = ann(
            target.clone(),
            "category",
            json!("new"),
            device_b,
            datetime!(2026-01-02 12:00 UTC),
        );

        let resolved = resolve(vec![earlier, later.clone()]);
        let key = AnnotationKey {
            target,
            field: "category".to_string(),
        };
        assert_eq!(resolved.get(&key).map(|a| a.id), Some(later.id));
    }

    #[test]
    fn ties_broken_deterministically_by_device_id() {
        let target = AnnotationTarget::Activity(Uuid::from_u128(100));
        let device_a = Uuid::from_u128(1);
        let device_b = Uuid::from_u128(2);
        let same_time = datetime!(2026-01-01 12:00 UTC);

        let from_a = ann(
            target.clone(),
            "category",
            json!("a"),
            device_a,
            same_time,
        );
        let from_b = ann(
            target.clone(),
            "category",
            json!("b"),
            device_b,
            same_time,
        );

        let key = AnnotationKey {
            target,
            field: "category".to_string(),
        };

        let r1 = resolve(vec![from_a.clone(), from_b.clone()]);
        let r2 = resolve(vec![from_b.clone(), from_a.clone()]);

        assert_eq!(r1.get(&key).map(|a| a.id), Some(from_b.id));
        assert_eq!(r2.get(&key).map(|a| a.id), Some(from_b.id));
    }

    #[test]
    fn different_fields_on_same_target_are_independent() {
        let target = AnnotationTarget::Observation(Uuid::from_u128(200));
        let device = Uuid::from_u128(1);
        let t = datetime!(2026-01-01 12:00 UTC);

        let category = ann(target.clone(), "category", json!("Work"), device, t);
        let note = ann(target.clone(), "note", json!("focused session"), device, t);

        let resolved = resolve(vec![category.clone(), note.clone()]);
        assert_eq!(resolved.len(), 2);
        assert!(resolved.contains_key(&AnnotationKey {
            target: target.clone(),
            field: "category".to_string(),
        }));
        assert!(resolved.contains_key(&AnnotationKey {
            target,
            field: "note".to_string(),
        }));
    }
}
