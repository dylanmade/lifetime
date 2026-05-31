use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use lifetime_core::aggregate::{
    AppDuration, AppSegment, HourActivity, aggregate_by_app, aggregate_by_hour,
    aggregate_into_segments,
};
use lifetime_core::model::{
    Activity, ActivityKind, ManualActivity, Observation, ObservationKind,
};
use lifetime_core::storage::{self, Store};
use lifetime_core::theme::{ThemeProfile, ThemeProfileSummary};
use lifetime_crypto::{MasterKey, RecoveryFile, SealedVault, vault as crypto_vault};
use serde::Serialize;
use tauri::{Manager, State};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(5);
const HEARTBEAT_INTERVAL: time::Duration = time::Duration::seconds(60);
const SQLCIPHER_INFO: &[u8] = b"lifetime/sqlcipher/v1";

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

    let paths = ctx.paths.clone();
    let mut guard = ctx.state.lock().map_err(|e| e.to_string())?;

    if !matches!(&*guard, AppState::Plaintext(_)) {
        return Err("encryption already enabled".to_string());
    }

    let _ = std::fs::remove_file(paths.temp_db());
    let _ = std::fs::remove_file(paths.temp_vault());

    let master_key = MasterKey::generate();
    let sqlcipher_key = master_key.derive_subkey(SQLCIPHER_INFO, 32);

    let encrypted_store = Store::open_encrypted(&paths.temp_db(), &sqlcipher_key)
        .map_err(|e| e.to_string())?;

    if let AppState::Plaintext(plaintext) = &*guard {
        if let Err(e) = storage::migrate(plaintext, &encrypted_store) {
            drop(encrypted_store);
            let _ = std::fs::remove_file(paths.temp_db());
            return Err(format!("migration failed: {e}"));
        }
    }
    drop(encrypted_store);

    let sealed = crypto_vault::seal(&master_key, &passphrase).map_err(|e| e.to_string())?;
    let vault_json = serde_json::to_string_pretty(&sealed).map_err(|e| e.to_string())?;
    std::fs::write(paths.temp_vault(), &vault_json).map_err(|e| e.to_string())?;

    // Close plaintext store by replacing state. The previous Store drops here,
    // releasing the file handle so the rename below works on Windows too.
    *guard = AppState::Locked;

    // Commit sequence. The vault rename is the point of no return — after it
    // succeeds, future startups will require the passphrase.
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

    let new_store = Store::open_encrypted(&paths.db_path, &sqlcipher_key)
        .map_err(|e| e.to_string())?;

    let fingerprint = master_key.fingerprint();
    let recovery_text = RecoveryFile::new(master_key.clone()).to_text();

    *guard = AppState::Unlocked {
        store: new_store,
        master_key,
    };

    Ok(EnableEncryptionResult {
        recovery_file: recovery_text,
        fingerprint,
    })
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
) -> Result<Vec<AppDuration>, String> {
    let start = OffsetDateTime::parse(&start_iso, &Rfc3339).map_err(|e| e.to_string())?;
    let end = OffsetDateTime::parse(&end_iso, &Rfc3339).map_err(|e| e.to_string())?;

    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    let store = guard
        .store()
        .ok_or_else(|| "app is locked".to_string())?;
    let observations = store
        .observations_between(start, end)
        .map_err(|e| e.to_string())?;

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

#[tauri::command]
fn get_activities_between(
    ctx: State<Arc<AppContext>>,
    start_iso: String,
    end_iso: String,
) -> Result<Vec<Activity>, String> {
    let start = OffsetDateTime::parse(&start_iso, &Rfc3339).map_err(|e| e.to_string())?;
    let end = OffsetDateTime::parse(&end_iso, &Rfc3339).map_err(|e| e.to_string())?;

    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    let store = guard
        .store()
        .ok_or_else(|| "app is locked".to_string())?;
    store
        .activities_between(start, end)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_hourly_activity(
    ctx: State<Arc<AppContext>>,
    start_iso: String,
    end_iso: String,
) -> Result<Vec<HourActivity>, String> {
    let start = OffsetDateTime::parse(&start_iso, &Rfc3339).map_err(|e| e.to_string())?;
    let end = OffsetDateTime::parse(&end_iso, &Rfc3339).map_err(|e| e.to_string())?;

    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    let store = guard
        .store()
        .ok_or_else(|| "app is locked".to_string())?;
    let observations = store
        .observations_between(start, end)
        .map_err(|e| e.to_string())?;

    Ok(aggregate_by_hour(
        &observations,
        start,
        end,
        time::Duration::minutes(2),
    ))
}

#[tauri::command]
fn get_timeline_segments(
    ctx: State<Arc<AppContext>>,
    start_iso: String,
    end_iso: String,
) -> Result<Vec<AppSegment>, String> {
    let start = OffsetDateTime::parse(&start_iso, &Rfc3339).map_err(|e| e.to_string())?;
    let end = OffsetDateTime::parse(&end_iso, &Rfc3339).map_err(|e| e.to_string())?;

    let guard = ctx.state.lock().map_err(|e| e.to_string())?;
    let store = guard
        .store()
        .ok_or_else(|| "app is locked".to_string())?;
    let observations = store
        .observations_between(start, end)
        .map_err(|e| e.to_string())?;

    Ok(aggregate_into_segments(
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
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
            });

            app.manage(ctx.clone());
            start_sampling_loop(ctx);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_state,
            unlock,
            enable_encryption,
            get_recent_observations,
            get_app_totals,
            get_hourly_activity,
            get_timeline_segments,
            create_manual_activity,
            get_activities_between,
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
