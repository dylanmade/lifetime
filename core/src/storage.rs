//! Local SQLite persistence for observations, activities, and annotations.
//!
//! All three record kinds are append-only — edits to existing records flow
//! through new annotations resolved by [`crate::lww`]. JSON is the
//! source-of-truth payload per row; sibling columns exist purely for
//! indexing and time-range filtering.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::{Connection, params};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::lww::{AnnotationKey, resolve};
use crate::model::{
    Activity, ActivityKind, Annotation, AnnotationTarget, Observation, ObservationKind,
    activity_fields,
};
use crate::sync::{SyncRecord, VersionVector};
use crate::theme::{ThemeProfile, ThemeProfileSummary};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, StorageError>;

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    /// Open an encrypted database. `key` must be exactly 32 bytes; callers
    /// typically derive this via HKDF from a `MasterKey`.
    pub fn open_encrypted(path: &Path, key: &[u8]) -> Result<Self> {
        let conn = Connection::open(path)?;
        apply_key(&conn, key)?;
        Self::init(conn)
    }

    /// In-memory encrypted store. Useful for tests; not normally a real
    /// configuration since there's nothing on disk to protect.
    pub fn open_encrypted_in_memory(key: &[u8]) -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        apply_key(&conn, key)?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn insert_observation(&self, obs: &Observation) -> Result<()> {
        let kind = observation_kind_discriminant(&obs.kind);
        let data = serde_json::to_string(obs)?;
        self.conn.execute(
            "INSERT INTO observations (id, device_id, recorded_at, kind, data)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![obs.id, obs.device_id, obs.recorded_at, kind, data],
        )?;
        Ok(())
    }

    pub fn observations_between(
        &self,
        start: OffsetDateTime,
        end: OffsetDateTime,
    ) -> Result<Vec<Observation>> {
        let mut stmt = self.conn.prepare(
            "SELECT data FROM observations
             WHERE recorded_at >= ?1 AND recorded_at < ?2
             ORDER BY recorded_at ASC",
        )?;
        let rows = stmt.query_map(params![start, end], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }

    /// The most-recent `limit` observations, newest first.
    pub fn recent_observations(&self, limit: usize) -> Result<Vec<Observation>> {
        let mut stmt = self.conn.prepare(
            "SELECT data FROM observations
             ORDER BY recorded_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }

    pub fn insert_activity(&self, act: &Activity) -> Result<()> {
        let kind = activity_kind_discriminant(&act.kind);
        let data = serde_json::to_string(act)?;
        self.conn.execute(
            "INSERT INTO activities (id, device_id, starts_at, ends_at, kind, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                act.id,
                act.device_id,
                act.starts_at,
                act.ends_at,
                kind,
                data
            ],
        )?;
        Ok(())
    }

    /// Activities whose time range intersects `[start, end)`.
    pub fn activities_between(
        &self,
        start: OffsetDateTime,
        end: OffsetDateTime,
    ) -> Result<Vec<Activity>> {
        let mut stmt = self.conn.prepare(
            "SELECT data FROM activities
             WHERE starts_at < ?2 AND (ends_at IS NULL OR ends_at > ?1)
             ORDER BY starts_at ASC",
        )?;
        let rows = stmt.query_map(params![start, end], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }

    /// Fetch a single manual activity by id. Used to tell manual rows apart
    /// from derived auto activities (auto ids never match a row).
    pub fn get_activity_by_id(&self, id: Uuid) -> Result<Option<Activity>> {
        let mut stmt = self
            .conn
            .prepare("SELECT data FROM activities WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(r) => Ok(Some(serde_json::from_str(&r?)?)),
            None => Ok(None),
        }
    }

    /// Sync the denormalized `starts_at`/`ends_at` index columns after a manual
    /// time edit, so range queries still find the activity. The authoritative
    /// edited value lives in the time annotations; these columns are a cache
    /// (see the module-level note on sibling columns existing for indexing).
    pub fn update_activity_times(
        &self,
        id: Uuid,
        starts_at: OffsetDateTime,
        ends_at: Option<OffsetDateTime>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE activities SET starts_at = ?1, ends_at = ?2 WHERE id = ?3",
            params![starts_at, ends_at, id],
        )?;
        Ok(())
    }

    /// All annotations targeting any of the given activity ids, in one query.
    /// Feeds [`crate::activity::resolve_activities`].
    pub fn annotations_for_activities(&self, ids: &[Uuid]) -> Result<Vec<Annotation>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT id, target_id, field, value, device_id, edited_at
             FROM annotations
             WHERE target_kind = 'activity' AND target_id IN ({placeholders})"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |row| {
            Ok((
                row.get::<_, Uuid>(0)?,
                row.get::<_, Uuid>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Uuid>(4)?,
                row.get::<_, OffsetDateTime>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, target_id, field, value, device_id, edited_at) = row?;
            out.push(Annotation {
                id,
                target: AnnotationTarget::Activity(target_id),
                field,
                value: serde_json::from_str(&value)?,
                device_id,
                edited_at,
            });
        }
        Ok(out)
    }

    /// All observations in chronological order. Intended for migration / export,
    /// not for general UI queries (use `observations_between` for those).
    pub fn all_observations(&self) -> Result<Vec<Observation>> {
        let mut stmt = self
            .conn
            .prepare("SELECT data FROM observations ORDER BY recorded_at ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }

    pub fn all_activities(&self) -> Result<Vec<Activity>> {
        let mut stmt = self
            .conn
            .prepare("SELECT data FROM activities ORDER BY starts_at ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }

    pub fn all_annotations(&self) -> Result<Vec<Annotation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, target_kind, target_id, field, value, device_id, edited_at
             FROM annotations",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, Uuid>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Uuid>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Uuid>(5)?,
                row.get::<_, OffsetDateTime>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, target_kind, target_id, field, value, device_id, edited_at) = row?;
            let target = match target_kind.as_str() {
                "observation" => AnnotationTarget::Observation(target_id),
                "activity" => AnnotationTarget::Activity(target_id),
                _ => continue,
            };
            out.push(Annotation {
                id,
                target,
                field,
                value: serde_json::from_str(&value)?,
                device_id,
                edited_at,
            });
        }
        Ok(out)
    }

    pub fn insert_annotation(&self, ann: &Annotation) -> Result<()> {
        let (target_kind, target_id) = annotation_target_parts(&ann.target);
        let value = serde_json::to_string(&ann.value)?;
        self.conn.execute(
            "INSERT INTO annotations (id, target_kind, target_id, field, value, device_id, edited_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                ann.id,
                target_kind,
                target_id,
                ann.field,
                value,
                ann.device_id,
                ann.edited_at
            ],
        )?;
        Ok(())
    }

    pub fn list_theme_profiles(&self) -> Result<Vec<ThemeProfileSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, updated_at FROM theme_profiles ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ThemeProfileSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                updated_at: row.get(2)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_theme_profile(&self, id: Uuid) -> Result<Option<ThemeProfile>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, created_at, updated_at, data FROM theme_profiles WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(ThemeProfile {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                data: row.get(4)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn insert_theme_profile(&self, profile: &ThemeProfile) -> Result<()> {
        self.conn.execute(
            "INSERT INTO theme_profiles (id, name, created_at, updated_at, data)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                profile.id,
                profile.name,
                profile.created_at,
                profile.updated_at,
                profile.data,
            ],
        )?;
        Ok(())
    }

    pub fn update_theme_profile(
        &self,
        id: Uuid,
        name: &str,
        data: &str,
        updated_at: OffsetDateTime,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE theme_profiles SET name = ?1, data = ?2, updated_at = ?3 WHERE id = ?4",
            params![name, data, updated_at, id],
        )?;
        Ok(())
    }

    pub fn delete_theme_profile(&self, id: Uuid) -> Result<()> {
        self.conn
            .execute("DELETE FROM theme_profiles WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn annotations_for(&self, target: &AnnotationTarget) -> Result<Vec<Annotation>> {
        let (target_kind, target_id) = annotation_target_parts(target);
        let mut stmt = self.conn.prepare(
            "SELECT id, field, value, device_id, edited_at
             FROM annotations
             WHERE target_kind = ?1 AND target_id = ?2",
        )?;
        let rows = stmt.query_map(params![target_kind, target_id], |row| {
            Ok((
                row.get::<_, Uuid>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Uuid>(3)?,
                row.get::<_, OffsetDateTime>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, field, value, device_id, edited_at) = row?;
            out.push(Annotation {
                id,
                target: target.clone(),
                field,
                value: serde_json::from_str(&value)?,
                device_id,
                edited_at,
            });
        }
        Ok(out)
    }

    // ---- Sync primitives (Phase A) ------------------------------------------

    /// Highest record id held from each origin device, across all event tables.
    /// The peer compares this against its own to learn what it's missing.
    pub fn version_vector(&self) -> Result<VersionVector> {
        let mut vv: VersionVector = HashMap::new();
        for table in ["observations", "activities", "annotations"] {
            let sql = format!("SELECT device_id, MAX(id) FROM {table} GROUP BY device_id");
            let mut stmt = self.conn.prepare(&sql)?;
            let rows =
                stmt.query_map([], |row| Ok((row.get::<_, Uuid>(0)?, row.get::<_, Uuid>(1)?)))?;
            for row in rows {
                let (device, max_id) = row?;
                vv.entry(device)
                    .and_modify(|cur| {
                        if max_id > *cur {
                            *cur = max_id;
                        }
                    })
                    .or_insert(max_id);
            }
        }
        Ok(vv)
    }

    /// Every record this store holds that `peer` lacks: for each origin device, the
    /// records (all tables) with `id` past the peer's watermark for that device, or
    /// all of the device's records if the peer has none.
    pub fn records_since(&self, peer: &VersionVector) -> Result<Vec<SyncRecord>> {
        let mut out = Vec::new();
        for device in self.version_vector()?.keys().copied() {
            let after = peer.get(&device).copied();
            for obs in self.observations_from(device, after)? {
                out.push(SyncRecord::Observation(obs));
            }
            for act in self.activities_from(device, after)? {
                out.push(SyncRecord::Activity(act));
            }
            for ann in self.annotations_from(device, after)? {
                out.push(SyncRecord::Annotation(ann));
            }
        }
        Ok(out)
    }

    /// Idempotently apply foreign records (`INSERT OR IGNORE` by id); returns the
    /// number actually inserted. Reconciles the denormalized activity time columns
    /// for any activity that received a time annotation, so `activities_between`
    /// still finds it.
    pub fn ingest(&self, records: &[SyncRecord]) -> Result<usize> {
        let mut applied = 0;
        let mut time_edited: HashSet<Uuid> = HashSet::new();
        for record in records {
            match record {
                SyncRecord::Observation(o) => applied += self.ingest_observation(o)?,
                SyncRecord::Activity(a) => applied += self.ingest_activity(a)?,
                SyncRecord::Annotation(ann) => {
                    applied += self.ingest_annotation(ann)?;
                    if let AnnotationTarget::Activity(id) = ann.target {
                        if ann.field == activity_fields::STARTS_AT
                            || ann.field == activity_fields::ENDS_AT
                        {
                            time_edited.insert(id);
                        }
                    }
                }
            }
        }
        for id in time_edited {
            self.reconcile_activity_times(id)?;
        }
        Ok(applied)
    }

    fn observations_from(&self, device: Uuid, after: Option<Uuid>) -> Result<Vec<Observation>> {
        let mut stmt = self.conn.prepare(
            "SELECT data FROM observations
             WHERE device_id = ?1 AND (?2 IS NULL OR id > ?2)
             ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![device, after], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }

    fn activities_from(&self, device: Uuid, after: Option<Uuid>) -> Result<Vec<Activity>> {
        let mut stmt = self.conn.prepare(
            "SELECT data FROM activities
             WHERE device_id = ?1 AND (?2 IS NULL OR id > ?2)
             ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![device, after], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }

    fn annotations_from(&self, device: Uuid, after: Option<Uuid>) -> Result<Vec<Annotation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, target_kind, target_id, field, value, device_id, edited_at
             FROM annotations
             WHERE device_id = ?1 AND (?2 IS NULL OR id > ?2)
             ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![device, after], |row| {
            Ok((
                row.get::<_, Uuid>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Uuid>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Uuid>(5)?,
                row.get::<_, OffsetDateTime>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, target_kind, target_id, field, value, device_id, edited_at) = row?;
            let target = match target_kind.as_str() {
                "observation" => AnnotationTarget::Observation(target_id),
                "activity" => AnnotationTarget::Activity(target_id),
                _ => continue,
            };
            out.push(Annotation {
                id,
                target,
                field,
                value: serde_json::from_str(&value)?,
                device_id,
                edited_at,
            });
        }
        Ok(out)
    }

    fn ingest_observation(&self, obs: &Observation) -> Result<usize> {
        let kind = observation_kind_discriminant(&obs.kind);
        let data = serde_json::to_string(obs)?;
        Ok(self.conn.execute(
            "INSERT OR IGNORE INTO observations (id, device_id, recorded_at, kind, data)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![obs.id, obs.device_id, obs.recorded_at, kind, data],
        )?)
    }

    fn ingest_activity(&self, act: &Activity) -> Result<usize> {
        let kind = activity_kind_discriminant(&act.kind);
        let data = serde_json::to_string(act)?;
        Ok(self.conn.execute(
            "INSERT OR IGNORE INTO activities (id, device_id, starts_at, ends_at, kind, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![act.id, act.device_id, act.starts_at, act.ends_at, kind, data],
        )?)
    }

    fn ingest_annotation(&self, ann: &Annotation) -> Result<usize> {
        let (target_kind, target_id) = annotation_target_parts(&ann.target);
        let value = serde_json::to_string(&ann.value)?;
        Ok(self.conn.execute(
            "INSERT OR IGNORE INTO annotations
                 (id, target_kind, target_id, field, value, device_id, edited_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                ann.id,
                target_kind,
                target_id,
                ann.field,
                value,
                ann.device_id,
                ann.edited_at
            ],
        )?)
    }

    /// Re-derive an activity's effective start/end from its annotations (LWW) and
    /// sync the denormalized index columns. No-op if the activity row isn't present.
    fn reconcile_activity_times(&self, activity_id: Uuid) -> Result<()> {
        let Some(base) = self.get_activity_by_id(activity_id)? else {
            return Ok(());
        };
        let resolved = resolve(self.annotations_for_activities(&[activity_id])?);
        let key = |field: &str| AnnotationKey {
            target: AnnotationTarget::Activity(activity_id),
            field: field.to_string(),
        };
        let start = resolved
            .get(&key(activity_fields::STARTS_AT))
            .and_then(|a| a.value.as_str())
            .and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
            .unwrap_or(base.starts_at);
        let end = match resolved.get(&key(activity_fields::ENDS_AT)) {
            Some(a) => a
                .value
                .as_str()
                .and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok()),
            None => base.ends_at,
        };
        self.update_activity_times(activity_id, start, end)
    }
}

/// Issue `PRAGMA key` against an open SQLCipher connection, then sanity-check
/// the key by reading sqlite_master. A wrong key (against an existing DB) causes
/// the read to fail; a correct key (or a brand-new file) succeeds.
fn apply_key(conn: &Connection, key: &[u8]) -> Result<()> {
    let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
    conn.execute_batch(&format!("PRAGMA key = \"x'{hex}'\";"))?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| {
        row.get::<_, i64>(0)
    })?;
    Ok(())
}

/// Copy every record from `source` into `dest`. Used by the opt-in encryption
/// flow to convert plaintext ↔ encrypted stores. Insertion order is
/// observations, activities, annotations, then theme profiles.
pub fn migrate(source: &Store, dest: &Store) -> Result<()> {
    for obs in source.all_observations()? {
        dest.insert_observation(&obs)?;
    }
    for act in source.all_activities()? {
        dest.insert_activity(&act)?;
    }
    for ann in source.all_annotations()? {
        dest.insert_annotation(&ann)?;
    }
    for summary in source.list_theme_profiles()? {
        if let Some(profile) = source.get_theme_profile(summary.id)? {
            dest.insert_theme_profile(&profile)?;
        }
    }
    Ok(())
}

fn observation_kind_discriminant(kind: &ObservationKind) -> &'static str {
    match kind {
        ObservationKind::AppUsage(_) => "app_usage",
        ObservationKind::Location(_) => "location",
        ObservationKind::Idle(_) => "idle",
        ObservationKind::DeviceState(_) => "device_state",
    }
}

fn activity_kind_discriminant(kind: &ActivityKind) -> &'static str {
    match kind {
        ActivityKind::Manual(_) => "manual",
    }
}

fn annotation_target_parts(target: &AnnotationTarget) -> (&'static str, Uuid) {
    match target {
        AnnotationTarget::Observation(id) => ("observation", *id),
        AnnotationTarget::Activity(id) => ("activity", *id),
    }
}

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS observations (
    id BLOB PRIMARY KEY NOT NULL,
    device_id BLOB NOT NULL,
    recorded_at TEXT NOT NULL,
    kind TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_observations_recorded_at
    ON observations(recorded_at);
CREATE INDEX IF NOT EXISTS idx_observations_device_recorded
    ON observations(device_id, recorded_at);

CREATE TABLE IF NOT EXISTS activities (
    id BLOB PRIMARY KEY NOT NULL,
    device_id BLOB NOT NULL,
    starts_at TEXT NOT NULL,
    ends_at TEXT,
    kind TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_activities_starts_at
    ON activities(starts_at);
CREATE INDEX IF NOT EXISTS idx_activities_device_id
    ON activities(device_id);

CREATE TABLE IF NOT EXISTS annotations (
    id BLOB PRIMARY KEY NOT NULL,
    target_kind TEXT NOT NULL,
    target_id BLOB NOT NULL,
    field TEXT NOT NULL,
    value TEXT NOT NULL,
    device_id BLOB NOT NULL,
    edited_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_annotations_target
    ON annotations(target_kind, target_id);
CREATE INDEX IF NOT EXISTS idx_annotations_target_field
    ON annotations(target_kind, target_id, field);
CREATE INDEX IF NOT EXISTS idx_annotations_device_id
    ON annotations(device_id);

CREATE TABLE IF NOT EXISTS theme_profiles (
    id BLOB PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    data TEXT NOT NULL
);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AppUsageSample, IdleSample, ManualActivity};
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    use time::macros::datetime;

    fn nonce() -> Uuid {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Uuid::from_u128(COUNTER.fetch_add(1, Ordering::Relaxed) as u128)
    }

    #[test]
    fn observation_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let obs = Observation {
            id: nonce(),
            device_id: nonce(),
            recorded_at: datetime!(2026-05-25 12:00 UTC),
            kind: ObservationKind::AppUsage(AppUsageSample {
                bundle_id: Some("com.apple.Safari".into()),
                app_name: "Safari".into(),
                window_title: Some("Hacker News".into()),
                is_active: true,
            }),
        };
        store.insert_observation(&obs).unwrap();

        let read = store
            .observations_between(
                datetime!(2026-05-25 00:00 UTC),
                datetime!(2026-05-26 00:00 UTC),
            )
            .unwrap();
        assert_eq!(read, vec![obs]);
    }

    #[test]
    fn observations_filter_by_time_range_and_order() {
        let store = Store::open_in_memory().unwrap();
        let device = nonce();
        let mk = |recorded_at| Observation {
            id: nonce(),
            device_id: device,
            recorded_at,
            kind: ObservationKind::Idle(IdleSample { idle_seconds: 30 }),
        };
        let early = mk(datetime!(2026-05-24 23:59 UTC));
        let inside_a = mk(datetime!(2026-05-25 09:00 UTC));
        let inside_b = mk(datetime!(2026-05-25 18:00 UTC));
        let late = mk(datetime!(2026-05-26 00:01 UTC));

        for obs in [&early, &inside_b, &inside_a, &late] {
            store.insert_observation(obs).unwrap();
        }

        let read = store
            .observations_between(
                datetime!(2026-05-25 00:00 UTC),
                datetime!(2026-05-26 00:00 UTC),
            )
            .unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].recorded_at, inside_a.recorded_at);
        assert_eq!(read[1].recorded_at, inside_b.recorded_at);
    }

    #[test]
    fn activity_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let act = Activity {
            id: nonce(),
            device_id: nonce(),
            starts_at: datetime!(2026-05-25 12:00 UTC),
            ends_at: Some(datetime!(2026-05-25 13:00 UTC)),
            kind: ActivityKind::Manual(ManualActivity {
                title: "Lunch".into(),
                description: None,
            }),
        };
        store.insert_activity(&act).unwrap();

        let read = store
            .activities_between(
                datetime!(2026-05-25 00:00 UTC),
                datetime!(2026-05-26 00:00 UTC),
            )
            .unwrap();
        assert_eq!(read, vec![act]);
    }

    #[test]
    fn activities_intersect_query_window() {
        let store = Store::open_in_memory().unwrap();
        let device = nonce();
        let spanning = Activity {
            id: nonce(),
            device_id: device,
            starts_at: datetime!(2026-05-24 22:00 UTC),
            ends_at: Some(datetime!(2026-05-25 03:00 UTC)),
            kind: ActivityKind::Manual(ManualActivity {
                title: "Late shift".into(),
                description: None,
            }),
        };
        store.insert_activity(&spanning).unwrap();

        let read = store
            .activities_between(
                datetime!(2026-05-25 00:00 UTC),
                datetime!(2026-05-26 00:00 UTC),
            )
            .unwrap();
        assert_eq!(read.len(), 1);
    }

    #[test]
    fn encrypted_round_trip_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("encrypted.db");
        let key = [0xABu8; 32];

        let obs_id;
        {
            let store = Store::open_encrypted(&path, &key).unwrap();
            let obs = Observation {
                id: nonce(),
                device_id: nonce(),
                recorded_at: datetime!(2026-05-26 12:00 UTC),
                kind: ObservationKind::AppUsage(AppUsageSample {
                    bundle_id: None,
                    app_name: "Encrypted Editor".into(),
                    window_title: None,
                    is_active: true,
                }),
            };
            obs_id = obs.id;
            store.insert_observation(&obs).unwrap();
        }

        let store = Store::open_encrypted(&path, &key).unwrap();
        let all = store.all_observations().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, obs_id);
    }

    #[test]
    fn wrong_encryption_key_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("encrypted.db");
        let key = [0xABu8; 32];
        let wrong = [0xCDu8; 32];

        {
            let store = Store::open_encrypted(&path, &key).unwrap();
            store
                .insert_observation(&Observation {
                    id: nonce(),
                    device_id: nonce(),
                    recorded_at: datetime!(2026-05-26 12:00 UTC),
                    kind: ObservationKind::Idle(IdleSample { idle_seconds: 0 }),
                })
                .unwrap();
        }

        assert!(Store::open_encrypted(&path, &wrong).is_err());
    }

    #[test]
    fn plaintext_file_with_wrong_assumption_fails() {
        // A plaintext file cannot be opened as encrypted (no PRAGMA key was used
        // when writing). Confirms the two modes are distinct on disk.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.db");
        {
            let store = Store::open(&path).unwrap();
            store
                .insert_observation(&Observation {
                    id: nonce(),
                    device_id: nonce(),
                    recorded_at: datetime!(2026-05-26 12:00 UTC),
                    kind: ObservationKind::Idle(IdleSample { idle_seconds: 0 }),
                })
                .unwrap();
        }
        assert!(Store::open_encrypted(&path, &[0u8; 32]).is_err());
    }

    #[test]
    fn migrate_plaintext_to_encrypted_preserves_everything() {
        let plaintext = Store::open_in_memory().unwrap();
        let device = nonce();

        let obs = Observation {
            id: nonce(),
            device_id: device,
            recorded_at: datetime!(2026-05-26 09:00 UTC),
            kind: ObservationKind::AppUsage(AppUsageSample {
                bundle_id: Some("com.example".into()),
                app_name: "Example".into(),
                window_title: None,
                is_active: true,
            }),
        };
        let act = Activity {
            id: nonce(),
            device_id: device,
            starts_at: datetime!(2026-05-26 10:00 UTC),
            ends_at: Some(datetime!(2026-05-26 11:00 UTC)),
            kind: ActivityKind::Manual(ManualActivity {
                title: "Standup".into(),
                description: None,
            }),
        };
        let ann = Annotation {
            id: nonce(),
            target: AnnotationTarget::Activity(act.id),
            field: "category".into(),
            value: serde_json::json!("Meetings"),
            device_id: device,
            edited_at: datetime!(2026-05-26 10:30 UTC),
        };
        plaintext.insert_observation(&obs).unwrap();
        plaintext.insert_activity(&act).unwrap();
        plaintext.insert_annotation(&ann).unwrap();

        let encrypted = Store::open_encrypted_in_memory(&[0x42u8; 32]).unwrap();
        migrate(&plaintext, &encrypted).unwrap();

        assert_eq!(encrypted.all_observations().unwrap().len(), 1);
        assert_eq!(encrypted.all_activities().unwrap().len(), 1);
        let migrated_anns = encrypted.all_annotations().unwrap();
        assert_eq!(migrated_anns.len(), 1);
        assert_eq!(migrated_anns[0].field, "category");
    }

    #[test]
    fn annotations_round_trip_and_query_by_target() {
        let store = Store::open_in_memory().unwrap();
        let device = nonce();
        let target = AnnotationTarget::Activity(nonce());

        let category = Annotation {
            id: nonce(),
            target: target.clone(),
            field: "category".into(),
            value: json!("Food"),
            device_id: device,
            edited_at: datetime!(2026-05-25 12:00 UTC),
        };
        let note = Annotation {
            id: nonce(),
            target: target.clone(),
            field: "note".into(),
            value: json!("with Sarah"),
            device_id: device,
            edited_at: datetime!(2026-05-25 12:01 UTC),
        };
        let other_target_ann = Annotation {
            id: nonce(),
            target: AnnotationTarget::Observation(nonce()),
            field: "category".into(),
            value: json!("Other"),
            device_id: device,
            edited_at: datetime!(2026-05-25 12:02 UTC),
        };

        store.insert_annotation(&category).unwrap();
        store.insert_annotation(&note).unwrap();
        store.insert_annotation(&other_target_ann).unwrap();

        let read = store.annotations_for(&target).unwrap();
        assert_eq!(read.len(), 2);
        assert!(read.iter().any(|a| a.field == "category"));
        assert!(read.iter().any(|a| a.field == "note"));
    }

    #[test]
    fn annotations_for_activities_filters_by_ids() {
        let store = Store::open_in_memory().unwrap();
        let device = nonce();
        let a = nonce();
        let b = nonce();
        let mk = |target: Uuid, field: &str| Annotation {
            id: nonce(),
            target: AnnotationTarget::Activity(target),
            field: field.into(),
            value: json!("x"),
            device_id: device,
            edited_at: datetime!(2026-05-25 12:00 UTC),
        };
        store.insert_annotation(&mk(a, "category")).unwrap();
        store.insert_annotation(&mk(a, "note")).unwrap();
        store.insert_annotation(&mk(b, "category")).unwrap();

        assert_eq!(store.annotations_for_activities(&[a]).unwrap().len(), 2);
        assert_eq!(store.annotations_for_activities(&[a, b]).unwrap().len(), 3);
        assert!(store.annotations_for_activities(&[]).unwrap().is_empty());
    }

    #[test]
    fn update_activity_times_moves_into_window() {
        let store = Store::open_in_memory().unwrap();
        let act = Activity {
            id: nonce(),
            device_id: nonce(),
            starts_at: datetime!(2026-05-20 12:00 UTC),
            ends_at: Some(datetime!(2026-05-20 13:00 UTC)),
            kind: ActivityKind::Manual(ManualActivity {
                title: "x".into(),
                description: None,
            }),
        };
        store.insert_activity(&act).unwrap();
        let window = (datetime!(2026-05-25 00:00 UTC), datetime!(2026-05-26 00:00 UTC));
        assert!(
            store
                .activities_between(window.0, window.1)
                .unwrap()
                .is_empty()
        );

        store
            .update_activity_times(
                act.id,
                datetime!(2026-05-25 09:00 UTC),
                Some(datetime!(2026-05-25 10:00 UTC)),
            )
            .unwrap();
        assert_eq!(store.activities_between(window.0, window.1).unwrap().len(), 1);
    }

    #[test]
    fn get_activity_by_id_distinguishes_rows() {
        let store = Store::open_in_memory().unwrap();
        let act = Activity {
            id: nonce(),
            device_id: nonce(),
            starts_at: datetime!(2026-05-25 12:00 UTC),
            ends_at: None,
            kind: ActivityKind::Manual(ManualActivity {
                title: "Open".into(),
                description: None,
            }),
        };
        store.insert_activity(&act).unwrap();
        assert!(store.get_activity_by_id(act.id).unwrap().is_some());
        assert!(store.get_activity_by_id(nonce()).unwrap().is_none());
    }
}
