use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use lifetime_core::activity::{ResolvedActivity, auto_activity_id, resolve_activities};
use lifetime_core::aggregate::{
    AppDuration, HourActivity, aggregate_by_app, aggregate_by_hour, aggregate_into_segments,
};
use lifetime_core::model::{
    Activity, ActivityKind, Annotation, AnnotationTarget, ManualActivity, Observation,
    ObservationKind, activity_fields,
};
use lifetime_core::storage::{self, Store};
use lifetime_core::theme::{ThemeProfile, ThemeProfileSummary};
use lifetime_crypto::{MasterKey, RecoveryFile, SealedVault, vault as crypto_vault};
use serde::Serialize;
use serde_json::json;
use tauri::{Manager, State};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(5);
const HEARTBEAT_INTERVAL: time::Duration = time::Duration::seconds(60);
const SQLCIPHER_INFO: &[u8] = b"lifetime/sqlcipher/v1";
const SYNC_PSK_INFO: &[u8] = b"lifetime/sync/psk/v1";
/// Bounds how long a stalled peer can hold the app-state lock during a session.
const SYNC_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
struct AppPaths {
    db_path: PathBuf,
    vault_path: PathBuf,
    device_id_path: PathBuf,
}

impl AppPaths {
    fn temp_db(&self) -> PathBuf {
        self.db_path.with_extension("encrypted.new")
    }
    fn backup_db(&self) -> PathBuf {
        self.db_path.with_extension("plaintext-backup")
    }
    fn temp_vault(&self) -> PathBuf {
        self.vault_path.with_extension("json.new")
    }
}

/// Whether the app currently has a usable store, and if so under what protection.
enum AppState {
    /// No vault file present; data is plaintext on disk.
    Plaintext(Store),
    /// Vault file present; passphrase has not yet been entered this session.
    /// Background sampling is paused; data commands return an error.
    Locked,
    /// Vault unlocked; encrypted store is open.
    Unlocked { store: Store, master_key: MasterKey },
}

impl AppState {
    fn store(&self) -> Option<&Store> {
        match self {
            Self::Plaintext(s) | Self::Unlocked { store: s, .. } => Some(s),
            Self::Locked => None,
        }
    }
}

struct AppContext {
    paths: AppPaths,
    device_id: Uuid,
    state: Mutex<AppState>,
    sync: Mutex<SyncStatusInfo>,
}

/// Live, observable state of the P2P sync service (for `sync_status` / the UI).
#[derive(Debug, Clone, Default, Serialize)]
struct SyncStatusInfo {
    /// Port this device's sync listener is bound to (for a peer to dial).
    listening_port: Option<u16>,
    /// Other Lifetime devices discovered on the LAN via mDNS.
    peers: Vec<PeerInfo>,
    last_peer: Option<String>,
    last_synced_at: Option<String>,
    last_sent: usize,
    last_received: usize,
    last_error: Option<String>,
}

/// A peer discovered on the local network.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct PeerInfo {
    device_id: String,
    host: String,
    port: u16,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum AppStateInfo {
    Plaintext,
    Locked { fingerprint: String },
    Unlocked { fingerprint: String },
}

#[derive(Serialize)]
struct EnableEncryptionResult {
    recovery_file: String,
    fingerprint: String,
}

#[tauri::command]
fn app_state(ctx: State<Arc<AppContext>>) -> Result<AppStateInfo, String> {
    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    match &*guard {
        AppState::Plaintext(_) => Ok(AppStateInfo::Plaintext),
        AppState::Locked => {
            drop(guard);
            let sealed = read_vault(&ctx.paths.vault_path).map_err(|e| e.to_string())?;
            Ok(AppStateInfo::Locked {
                fingerprint: sealed.fingerprint,
            })
        }
        AppState::Unlocked { master_key, .. } => Ok(AppStateInfo::Unlocked {
            fingerprint: master_key.fingerprint(),
        }),
    }
}

#[tauri::command]
fn unlock(ctx: State<Arc<AppContext>>, passphrase: String) -> Result<(), String> {
    let mut guard = ctx.state.lock().map_err(|e| e.to_string())?;
    if !matches!(&*guard, AppState::Locked) {
        return Err("app is not locked".to_string());
    }

    let sealed = read_vault(&ctx.paths.vault_path).map_err(|e| e.to_string())?;
    let master_key = crypto_vault::unlock_with_passphrase(&sealed, &passphrase)
        .map_err(|e| e.to_string())?;
    let sqlcipher_key = master_key.derive_subkey(SQLCIPHER_INFO, 32);
    let store = Store::open_encrypted(&ctx.paths.db_path, &sqlcipher_key)
        .map_err(|e| e.to_string())?;

    *guard = AppState::Unlocked { store, master_key };
    Ok(())
}

#[tauri::command]
fn enable_encryption(
    ctx: State<Arc<AppContext>>,
    passphrase: String,
) -> Result<EnableEncryptionResult, String> {
    if passphrase.is_empty() {
        return Err("passphrase cannot be empty".to_string());
    }
    let master_key = MasterKey::generate();
    let recovery_file = RecoveryFile::new(master_key.clone()).to_text();
    let fingerprint = establish_encryption(ctx.inner(), &passphrase, master_key)?;
    Ok(EnableEncryptionResult {
        recovery_file,
        fingerprint,
    })
}

/// Join an existing vault by importing its recovery file: install the shared
/// master key, seal a local vault with `passphrase`, migrate any local plaintext
/// data into the encrypted store, and unlock. The sync service then pulls the
/// rest of the dataset from the paired device. Requires plaintext state.
#[tauri::command]
fn import_recovery_and_pair(
    ctx: State<Arc<AppContext>>,
    recovery_text: String,
    passphrase: String,
) -> Result<String, String> {
    if passphrase.is_empty() {
        return Err("passphrase cannot be empty".to_string());
    }
    let master_key = RecoveryFile::from_text(&recovery_text)
        .map_err(|e| e.to_string())?
        .into_master_key();
    establish_encryption(ctx.inner(), &passphrase, master_key)
}

/// Shared encryption bootstrap for both `enable_encryption` (fresh key) and
/// `import_recovery_and_pair` (imported key): seal the vault with `passphrase`
/// wrapping `master_key`, migrate the current plaintext DB into a fresh encrypted
/// one, and transition to Unlocked. Requires the current state to be Plaintext;
/// returns the key fingerprint.
fn establish_encryption(
    ctx: &Arc<AppContext>,
    passphrase: &str,
    master_key: MasterKey,
) -> Result<String, String> {
    let paths = ctx.paths.clone();
    let mut guard = ctx.state.lock().map_err(|e| e.to_string())?;

    if !matches!(&*guard, AppState::Plaintext(_)) {
        return Err("encryption already enabled".to_string());
    }

    let _ = std::fs::remove_file(paths.temp_db());
    let _ = std::fs::remove_file(paths.temp_vault());

    let sqlcipher_key = master_key.derive_subkey(SQLCIPHER_INFO, 32);
    let encrypted_store =
        Store::open_encrypted(&paths.temp_db(), &sqlcipher_key).map_err(|e| e.to_string())?;

    if let AppState::Plaintext(plaintext) = &*guard {
        if let Err(e) = storage::migrate(plaintext, &encrypted_store) {
            drop(encrypted_store);
            let _ = std::fs::remove_file(paths.temp_db());
            return Err(format!("migration failed: {e}"));
        }
    }
    drop(encrypted_store);

    let sealed = crypto_vault::seal(&master_key, passphrase).map_err(|e| e.to_string())?;
    let vault_json = serde_json::to_string_pretty(&sealed).map_err(|e| e.to_string())?;
    std::fs::write(paths.temp_vault(), &vault_json).map_err(|e| e.to_string())?;

    // Close the plaintext store by replacing state so its file handle releases.
    *guard = AppState::Locked;

    // Commit sequence. The vault rename is the point of no return.
    if let Err(e) = std::fs::rename(paths.temp_vault(), &paths.vault_path) {
        let _ = std::fs::remove_file(paths.temp_vault());
        let _ = std::fs::remove_file(paths.temp_db());
        if let Ok(store) = Store::open(&paths.db_path) {
            *guard = AppState::Plaintext(store);
        }
        return Err(format!("failed to commit vault: {e}"));
    }
    if let Err(e) = std::fs::rename(&paths.db_path, paths.backup_db()) {
        return Err(format!("failed to backup plaintext db: {e}"));
    }
    if let Err(e) = std::fs::rename(paths.temp_db(), &paths.db_path) {
        let _ = std::fs::rename(paths.backup_db(), &paths.db_path);
        let _ = std::fs::remove_file(&paths.vault_path);
        return Err(format!("failed to install encrypted db: {e}"));
    }
    let _ = std::fs::remove_file(paths.backup_db());

    let new_store =
        Store::open_encrypted(&paths.db_path, &sqlcipher_key).map_err(|e| e.to_string())?;
    let fingerprint = master_key.fingerprint();
    *guard = AppState::Unlocked {
        store: new_store,
        master_key,
    };
    Ok(fingerprint)
}

#[tauri::command]
fn accessibility_granted() -> bool {
    #[cfg(target_os = "macos")]
    {
        lifetime_tracker::macos::is_accessibility_granted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[tauri::command]
fn request_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    {
        lifetime_tracker::macos::request_accessibility_permission()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[tauri::command]
fn get_recent_observations(
    ctx: State<Arc<AppContext>>,
    limit: usize,
) -> Result<Vec<Observation>, String> {
    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    let store = guard
        .store()
        .ok_or_else(|| "app is locked".to_string())?;
    store.recent_observations(limit).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_app_totals(
    ctx: State<Arc<AppContext>>,
    start_iso: String,
    end_iso: String,
    device_id: Option<String>,
) -> Result<Vec<AppDuration>, String> {
    let start = OffsetDateTime::parse(&start_iso, &Rfc3339).map_err(|e| e.to_string())?;
    let end = OffsetDateTime::parse(&end_iso, &Rfc3339).map_err(|e| e.to_string())?;
    let scope = resolve_scope(ctx.device_id, device_id)?;

    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    let store = guard
        .store()
        .ok_or_else(|| "app is locked".to_string())?;
    let observations = scope_observations(
        store
            .observations_between(start, end)
            .map_err(|e| e.to_string())?,
        scope,
    );

    Ok(aggregate_by_app(
        &observations,
        end,
        time::Duration::minutes(2),
    ))
}

#[tauri::command]
fn create_manual_activity(
    ctx: State<Arc<AppContext>>,
    title: String,
    description: Option<String>,
    starts_at_iso: String,
    ends_at_iso: Option<String>,
) -> Result<Activity, String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("title cannot be empty".to_string());
    }
    let starts_at =
        OffsetDateTime::parse(&starts_at_iso, &Rfc3339).map_err(|e| e.to_string())?;
    let ends_at = match ends_at_iso {
        Some(s) => Some(OffsetDateTime::parse(&s, &Rfc3339).map_err(|e| e.to_string())?),
        None => None,
    };
    if let Some(end) = ends_at {
        if end <= starts_at {
            return Err("end time must be after start time".to_string());
        }
    }

    let activity = Activity {
        id: Uuid::now_v7(),
        device_id: ctx.device_id,
        starts_at,
        ends_at,
        kind: ActivityKind::Manual(ManualActivity {
            title,
            description: description.and_then(|s| {
                let trimmed = s.trim().to_string();
                if trimmed.is_empty() { None } else { Some(trimmed) }
            }),
        }),
    };

    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    let store = guard
        .store()
        .ok_or_else(|| "app is locked".to_string())?;
    store.insert_activity(&activity).map_err(|e| e.to_string())?;
    Ok(activity)
}

/// Unified activity read: manual rows + auto runs derived from observations,
/// each with its last-write-wins annotations overlaid, tombstoned ones dropped.
#[tauri::command]
fn get_activities_between(
    ctx: State<Arc<AppContext>>,
    start_iso: String,
    end_iso: String,
    device_id: Option<String>,
) -> Result<Vec<ResolvedActivity>, String> {
    let start = OffsetDateTime::parse(&start_iso, &Rfc3339).map_err(|e| e.to_string())?;
    let end = OffsetDateTime::parse(&end_iso, &Rfc3339).map_err(|e| e.to_string())?;
    let scope = resolve_scope(ctx.device_id, device_id)?;

    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    let store = guard.store().ok_or_else(|| "app is locked".to_string())?;

    // Scope auto activities (derived from observations) and manual rows to the
    // requested device. Annotations are fetched by activity id regardless of which
    // device authored the edit, so cross-device edits still apply (LWW).
    let observations = scope_observations(
        store
            .observations_between(start, end)
            .map_err(|e| e.to_string())?,
        scope,
    );
    let segments =
        aggregate_into_segments(&observations, start, end, time::Duration::minutes(2));
    let manual: Vec<Activity> = store
        .activities_between(start, end)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|a| scope.is_none_or(|d| a.device_id == d))
        .collect();

    let mut ids: Vec<Uuid> = manual.iter().map(|a| a.id).collect();
    ids.extend(segments.iter().map(|s| auto_activity_id(s.origin_observation_id)));
    let annotations = store
        .annotations_for_activities(&ids)
        .map_err(|e| e.to_string())?;

    Ok(resolve_activities(manual, segments, annotations))
}

/// Edit an activity by appending annotation events (LWW). Each provided field
/// is set; for the nullable text fields an empty string clears the value.
/// Auto-tracked activities only accept `category`/`description`; manual ones
/// accept all fields. Manual time edits also sync the index columns.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn update_activity(
    ctx: State<Arc<AppContext>>,
    id: String,
    title: Option<String>,
    description: Option<String>,
    category: Option<String>,
    starts_at_iso: Option<String>,
    ends_at_iso: Option<String>,
) -> Result<(), String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    let store = guard.store().ok_or_else(|| "app is locked".to_string())?;

    let is_manual = store
        .get_activity_by_id(uuid)
        .map_err(|e| e.to_string())?
        .is_some();
    if !is_manual && (title.is_some() || starts_at_iso.is_some() || ends_at_iso.is_some()) {
        return Err("auto-tracked activities only support category and note edits".to_string());
    }

    let now = OffsetDateTime::now_utc();
    let device = ctx.device_id;

    if let Some(title) = &title {
        let title = title.trim();
        if title.is_empty() {
            return Err("title cannot be empty".to_string());
        }
        write_annotation(store, device, uuid, activity_fields::TITLE, json!(title), now)?;
    }
    if let Some(desc) = &description {
        write_annotation(
            store,
            device,
            uuid,
            activity_fields::DESCRIPTION,
            nullable_text(desc),
            now,
        )?;
    }
    if let Some(cat) = &category {
        write_annotation(
            store,
            device,
            uuid,
            activity_fields::CATEGORY,
            nullable_text(cat),
            now,
        )?;
    }

    // Time edits (manual only — guarded above).
    let new_start = match &starts_at_iso {
        Some(s) => Some(OffsetDateTime::parse(s, &Rfc3339).map_err(|e| e.to_string())?),
        None => None,
    };
    let new_end: Option<Option<OffsetDateTime>> = match &ends_at_iso {
        Some(s) if s.trim().is_empty() => Some(None),
        Some(s) => Some(Some(
            OffsetDateTime::parse(s, &Rfc3339).map_err(|e| e.to_string())?,
        )),
        None => None,
    };
    if let Some(s) = &starts_at_iso {
        write_annotation(store, device, uuid, activity_fields::STARTS_AT, json!(s), now)?;
    }
    if let Some(s) = &ends_at_iso {
        let value = if s.trim().is_empty() {
            serde_json::Value::Null
        } else {
            json!(s)
        };
        write_annotation(store, device, uuid, activity_fields::ENDS_AT, value, now)?;
    }
    if is_manual && (new_start.is_some() || new_end.is_some()) {
        let base = store
            .get_activity_by_id(uuid)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "activity not found".to_string())?;
        let resolved_start = new_start.unwrap_or(base.starts_at);
        let resolved_end = new_end.unwrap_or(base.ends_at);
        if let Some(end) = resolved_end {
            if end <= resolved_start {
                return Err("end time must be after start time".to_string());
            }
        }
        store
            .update_activity_times(uuid, resolved_start, resolved_end)
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Delete an activity by writing a `deleted` tombstone annotation. Works for
/// manual rows and derived auto activities alike; raw observations are kept.
#[tauri::command]
fn delete_activity(ctx: State<Arc<AppContext>>, id: String) -> Result<(), String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    let store = guard.store().ok_or_else(|| "app is locked".to_string())?;
    write_annotation(
        store,
        ctx.device_id,
        uuid,
        activity_fields::DELETED,
        json!(true),
        OffsetDateTime::now_utc(),
    )
}

#[tauri::command]
fn get_hourly_activity(
    ctx: State<Arc<AppContext>>,
    start_iso: String,
    end_iso: String,
    device_id: Option<String>,
) -> Result<Vec<HourActivity>, String> {
    let start = OffsetDateTime::parse(&start_iso, &Rfc3339).map_err(|e| e.to_string())?;
    let end = OffsetDateTime::parse(&end_iso, &Rfc3339).map_err(|e| e.to_string())?;
    let scope = resolve_scope(ctx.device_id, device_id)?;

    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    let store = guard
        .store()
        .ok_or_else(|| "app is locked".to_string())?;
    let observations = scope_observations(
        store
            .observations_between(start, end)
            .map_err(|e| e.to_string())?,
        scope,
    );

    Ok(aggregate_by_hour(
        &observations,
        start,
        end,
        time::Duration::minutes(2),
    ))
}

#[tauri::command]
fn list_theme_profiles(
    ctx: State<Arc<AppContext>>,
) -> Result<Vec<ThemeProfileSummary>, String> {
    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    let store = guard.store().ok_or_else(|| "app is locked".to_string())?;
    store.list_theme_profiles().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_theme_profile(
    ctx: State<Arc<AppContext>>,
    id: String,
) -> Result<Option<ThemeProfile>, String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    let store = guard.store().ok_or_else(|| "app is locked".to_string())?;
    store.get_theme_profile(uuid).map_err(|e| e.to_string())
}

#[tauri::command]
fn create_theme_profile(
    ctx: State<Arc<AppContext>>,
    name: String,
    data: String,
) -> Result<ThemeProfile, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("name cannot be empty".to_string());
    }
    let now = OffsetDateTime::now_utc();
    let profile = ThemeProfile {
        id: Uuid::now_v7(),
        name,
        created_at: now,
        updated_at: now,
        data,
    };
    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    let store = guard.store().ok_or_else(|| "app is locked".to_string())?;
    store
        .insert_theme_profile(&profile)
        .map_err(|e| e.to_string())?;
    Ok(profile)
}

#[tauri::command]
fn update_theme_profile(
    ctx: State<Arc<AppContext>>,
    id: String,
    name: String,
    data: String,
) -> Result<ThemeProfile, String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("name cannot be empty".to_string());
    }
    let now = OffsetDateTime::now_utc();
    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    let store = guard.store().ok_or_else(|| "app is locked".to_string())?;
    store
        .update_theme_profile(uuid, &name, &data, now)
        .map_err(|e| e.to_string())?;
    store
        .get_theme_profile(uuid)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "profile vanished after update".to_string())
}

#[tauri::command]
fn delete_theme_profile(ctx: State<Arc<AppContext>>, id: String) -> Result<(), String> {
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    let store = guard.store().ok_or_else(|| "app is locked".to_string())?;
    store.delete_theme_profile(uuid).map_err(|e| e.to_string())
}

#[tauri::command]
fn sync_status(ctx: State<Arc<AppContext>>) -> Result<SyncStatusInfo, String> {
    let st = ctx.sync.lock().map_err(|e| e.to_string())?;
    Ok(st.clone())
}

/// Manually sync with a peer at `host:port` (this device initiates). Phase B
/// uses explicit addresses; mDNS auto-discovery is the next increment.
#[tauri::command]
fn sync_with(
    ctx: State<Arc<AppContext>>,
    host: String,
    port: u16,
) -> Result<SyncStatusInfo, String> {
    let addr = format!("{host}:{port}");
    let stream = TcpStream::connect(&addr).map_err(|e| e.to_string())?;
    run_sync(ctx.inner(), stream, addr, true)?;
    let st = ctx.sync.lock().map_err(|e| e.to_string())?;
    Ok(st.clone())
}

/// Run one sync round over `stream`, holding the app-state lock for its (small,
/// fast) duration — bounded by socket timeouts so a stalled peer can't freeze the
/// app. Records the outcome in the shared sync status.
fn run_sync(
    ctx: &Arc<AppContext>,
    stream: TcpStream,
    peer: String,
    initiator: bool,
) -> Result<(), String> {
    let _ = stream.set_read_timeout(Some(SYNC_TIMEOUT));
    let _ = stream.set_write_timeout(Some(SYNC_TIMEOUT));

    let outcome = {
        let guard = ctx.state.lock().map_err(|e| e.to_string())?;
        let AppState::Unlocked { store, master_key } = &*guard else {
            return Err("enable encryption to sync".to_string());
        };
        let psk = master_key.derive_subkey(SYNC_PSK_INFO, 32);
        lifetime_net::run_session(store, stream, &psk, initiator).map_err(|e| e.to_string())
    };

    let mut st = ctx.sync.lock().map_err(|e| e.to_string())?;
    st.last_peer = Some(peer);
    match &outcome {
        Ok(o) => {
            st.last_synced_at = OffsetDateTime::now_utc().format(&Rfc3339).ok();
            st.last_sent = o.sent;
            st.last_received = o.received;
            st.last_error = None;
        }
        Err(e) => st.last_error = Some(e.clone()),
    }
    outcome.map(|_| ())
}

/// Accept inbound sync connections on an ephemeral port, recording it in the sync
/// status so a peer can dial it. Each connection runs a responder session. Returns
/// the bound port (for mDNS advertising).
fn start_sync_listener(ctx: Arc<AppContext>) -> Option<u16> {
    let listener = match TcpListener::bind("0.0.0.0:0") {
        Ok(l) => l,
        Err(e) => {
            eprintln!("sync listener bind failed: {e}");
            return None;
        }
    };
    let port = listener.local_addr().ok()?.port();
    if let Ok(mut st) = ctx.sync.lock() {
        st.listening_port = Some(port);
    }
    thread::spawn(move || {
        for incoming in listener.incoming() {
            let Ok(stream) = incoming else { continue };
            let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
            let ctx = ctx.clone();
            thread::spawn(move || {
                if let Err(e) = run_sync(&ctx, stream, peer, false) {
                    eprintln!("inbound sync failed: {e}");
                }
            });
        }
    });
    Some(port)
}

const SYNC_SERVICE: &str = "_lifetime-sync._tcp.local.";

/// Advertise this device and browse for other Lifetime devices on the LAN via
/// mDNS. Discovered peers land in the sync status so the UI can offer one-click
/// sync (no IP typing). Same-vault trust is still enforced at the PSK handshake,
/// so a discovered peer from a different vault simply fails to sync.
fn start_mdns(ctx: Arc<AppContext>, port: u16) {
    use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

    let mdns = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("mdns init failed: {e}");
            return;
        }
    };

    let instance = ctx.device_id.to_string();
    let host = format!("{instance}.local.");
    let mut props = std::collections::HashMap::new();
    props.insert("device".to_string(), instance.clone());
    match ServiceInfo::new(SYNC_SERVICE, &instance, &host, "", port, props) {
        Ok(info) => {
            if let Err(e) = mdns.register(info.enable_addr_auto()) {
                eprintln!("mdns register failed: {e}");
            }
        }
        Err(e) => eprintln!("mdns service info failed: {e}"),
    }

    let receiver = match mdns.browse(SYNC_SERVICE) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("mdns browse failed: {e}");
            return;
        }
    };

    thread::spawn(move || {
        let _daemon = mdns; // keep the daemon alive for the app's lifetime
        let me = ctx.device_id.to_string();
        while let Ok(event) = receiver.recv() {
            match event {
                ServiceEvent::ServiceResolved(info) => {
                    let device = info.get_property_val_str("device").unwrap_or("").to_string();
                    if device.is_empty() || device == me {
                        continue;
                    }
                    let addr = info
                        .get_addresses()
                        .iter()
                        .find(|a| a.is_ipv4())
                        .or_else(|| info.get_addresses().iter().next())
                        .copied();
                    let Some(addr) = addr else { continue };
                    let peer = PeerInfo {
                        device_id: device,
                        host: addr.to_string(),
                        port: info.get_port(),
                    };
                    if let Ok(mut st) = ctx.sync.lock() {
                        st.peers.retain(|p| p.device_id != peer.device_id);
                        st.peers.push(peer);
                    }
                }
                ServiceEvent::ServiceRemoved(_ty, fullname) => {
                    let dev = fullname.split('.').next().unwrap_or("");
                    if let Ok(mut st) = ctx.sync.lock() {
                        st.peers.retain(|p| p.device_id != dev);
                    }
                }
                _ => {}
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // LIFETIME_DATA_DIR lets a second instance run with an isolated
            // dataset on the same machine (for two-device sync testing).
            let app_data_dir = match std::env::var_os("LIFETIME_DATA_DIR") {
                Some(dir) => PathBuf::from(dir),
                None => app.path().app_data_dir()?,
            };
            std::fs::create_dir_all(&app_data_dir)?;

            let paths = AppPaths {
                db_path: app_data_dir.join("lifetime.sqlite"),
                vault_path: app_data_dir.join("vault.json"),
                device_id_path: app_data_dir.join("device_id"),
            };

            let device_id = load_or_create_device_id(&paths.device_id_path)?;

            let state = if paths.vault_path.exists() {
                AppState::Locked
            } else {
                AppState::Plaintext(Store::open(&paths.db_path)?)
            };

            let ctx = Arc::new(AppContext {
                paths,
                device_id,
                state: Mutex::new(state),
                sync: Mutex::new(SyncStatusInfo::default()),
            });

            app.manage(ctx.clone());
            start_sampling_loop(ctx.clone());
            if let Some(port) = start_sync_listener(ctx.clone()) {
                start_mdns(ctx, port);
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_state,
            unlock,
            enable_encryption,
            import_recovery_and_pair,
            sync_with,
            sync_status,
            get_recent_observations,
            get_app_totals,
            get_hourly_activity,
            create_manual_activity,
            get_activities_between,
            update_activity,
            delete_activity,
            accessibility_granted,
            request_accessibility,
            list_theme_profiles,
            get_theme_profile,
            create_theme_profile,
            update_theme_profile,
            delete_theme_profile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Resolve a read-scope request to an optional origin-device filter. A `None`
/// request ⇒ the local device (the default, so synced-in data never force-merges
/// into one timeline); `"all"` ⇒ no filter (amalgamated); a uuid ⇒ that device.
fn resolve_scope(local: Uuid, requested: Option<String>) -> Result<Option<Uuid>, String> {
    match requested.as_deref() {
        None => Ok(Some(local)),
        Some("all") => Ok(None),
        Some(s) => Uuid::parse_str(s).map(Some).map_err(|e| e.to_string()),
    }
}

/// Keep only observations from the scoped origin device (all, if `None`).
fn scope_observations(obs: Vec<Observation>, scope: Option<Uuid>) -> Vec<Observation> {
    match scope {
        Some(device) => obs.into_iter().filter(|o| o.device_id == device).collect(),
        None => obs,
    }
}

/// Append one activity-edit annotation event.
fn write_annotation(
    store: &Store,
    device_id: Uuid,
    target: Uuid,
    field: &str,
    value: serde_json::Value,
    edited_at: OffsetDateTime,
) -> Result<(), String> {
    store
        .insert_annotation(&Annotation {
            id: Uuid::now_v7(),
            target: AnnotationTarget::Activity(target),
            field: field.to_string(),
            value,
            device_id,
            edited_at,
        })
        .map_err(|e| e.to_string())
}

/// Trim a free-text edit; an empty result becomes JSON `null` (an explicit
/// clear), which resolution reads as "unset".
fn nullable_text(s: &str) -> serde_json::Value {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(trimmed.to_string())
    }
}

fn read_vault(path: &Path) -> Result<SealedVault, String> {
    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&contents).map_err(|e| e.to_string())
}

fn load_or_create_device_id(path: &Path) -> std::io::Result<Uuid> {
    if path.exists() {
        let contents = std::fs::read_to_string(path)?;
        Uuid::parse_str(contents.trim())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    } else {
        let id = Uuid::now_v7();
        std::fs::write(path, id.to_string())?;
        Ok(id)
    }
}

#[cfg(target_os = "macos")]
fn start_sampling_loop(ctx: Arc<AppContext>) {
    use lifetime_tracker::Sampler;
    use lifetime_tracker::macos::MacOsSampler;

    let sampler = MacOsSampler::new();
    thread::spawn(move || {
        let mut last_written: Option<(ObservationKind, OffsetDateTime)> = None;
        loop {
            thread::sleep(SAMPLE_INTERVAL);
            let now = OffsetDateTime::now_utc();
            let kinds = sampler.sample();
            let Ok(guard) = ctx.state.lock() else { continue };
            let Some(store) = guard.store() else { continue };

            for kind in kinds {
                let should_write = match &last_written {
                    None => true,
                    Some((prev, t)) => prev != &kind || (now - *t) >= HEARTBEAT_INTERVAL,
                };
                if !should_write {
                    continue;
                }
                let obs = Observation {
                    id: Uuid::now_v7(),
                    device_id: ctx.device_id,
                    recorded_at: now,
                    kind: kind.clone(),
                };
                if let Err(e) = store.insert_observation(&obs) {
                    eprintln!("failed to insert observation: {e}");
                    continue;
                }
                last_written = Some((kind, now));
            }
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn start_sampling_loop(_ctx: Arc<AppContext>) {}
