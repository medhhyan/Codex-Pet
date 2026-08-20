use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager, State, WebviewWindow, WindowEvent};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_positioner::{Position, WindowExt};
use tauri_plugin_store::StoreExt;

use crate::codex_adapter::{
    CodexDataAdapter, CodexPaths, PetSnapshot, SyncState, TaskSnapshot, TaskStatus, UsageSnapshot,
};
use crate::model::{PetState, Status};

const ISLAND_LABEL: &str = "island";
const SNAPSHOT_EVENT: &str = "pet://snapshot";
const SETTINGS_EVENT: &str = "pet://settings";
const ERROR_EVENT: &str = "pet://error";
const SETTINGS_STORE: &str = "settings.json";
const SETTINGS_KEY: &str = "settings";
const DISMISSED_COMPLETED_TASKS_KEY: &str = "dismissedCompletedTaskIds";
const RESTORE_ID: &str = "restore";
const MOTION_ID: &str = "motion";
const AUTOSTART_ID: &str = "autostart";
const QUIT_ID: &str = "quit";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub motion_enabled: bool,
    pub autostart_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            motion_enabled: false,
            autostart_enabled: false,
        }
    }
}

pub struct AppState {
    settings: Mutex<Settings>,
    snapshot: Mutex<PetSnapshot>,
    acknowledged_completed_until_ms: Mutex<Option<u64>>,
    dismissed_completed_task_ids: Mutex<BTreeSet<String>>,
    motion_item: CheckMenuItem<tauri::Wry>,
    autostart_item: CheckMenuItem<tauri::Wry>,
}

impl Serialize for Status {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Self::Working => "working",
            Self::Completed => "completed",
            Self::Resting => "resting",
        })
    }
}

impl Serialize for PetState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("PetState", 2)?;
        state.serialize_field("status", &self.status)?;
        state.serialize_field("completedUntilMs", &self.completed_until_ms)?;
        state.end()
    }
}

impl Serialize for SyncState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Self::Ready => "ready",
            Self::Stale => "stale",
            Self::Unavailable => "unavailable",
        })
    }
}

impl Serialize for TaskStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Self::Working => "working",
            Self::Completed => "completed",
            Self::Waiting => "waiting",
        })
    }
}

impl Serialize for TaskSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut task = serializer.serialize_struct("TaskSnapshot", 4)?;
        task.serialize_field("turnId", &self.turn_id)?;
        task.serialize_field("title", &self.title)?;
        task.serialize_field("status", &self.status)?;
        task.serialize_field("observedAtMs", &self.observed_at_ms)?;
        task.end()
    }
}

impl Serialize for UsageSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut usage = serializer.serialize_struct("UsageSnapshot", 4)?;
        usage.serialize_field("todayTokens", &self.today_tokens)?;
        usage.serialize_field("weeklyUsagePercent", &self.weekly_usage_percent)?;
        usage.serialize_field("weeklyResetAtMs", &self.weekly_reset_at_ms)?;
        usage.serialize_field("syncedAtMs", &self.synced_at_ms)?;
        usage.end()
    }
}

impl Serialize for PetSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut snapshot = serializer.serialize_struct("PetSnapshot", 4)?;
        snapshot.serialize_field("state", &self.state)?;
        snapshot.serialize_field("usage", &self.usage)?;
        snapshot.serialize_field("syncState", &self.sync_state)?;
        snapshot.serialize_field("tasks", &self.tasks)?;
        snapshot.end()
    }
}

#[tauri::command]
pub fn get_snapshot(state: State<'_, AppState>) -> Result<PetSnapshot, String> {
    Ok(lock(&state.snapshot, "snapshot")?.clone())
}

#[tauri::command]
pub fn set_motion_enabled(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    update_settings(&app, &state, |settings| settings.motion_enabled = enabled)?;
    state.motion_item.set_checked(enabled).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    set_autostart_value(&app, &state, enabled)?;
    state.autostart_item.set_checked(enabled).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn hide_to_tray(window: WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(*lock(&state.settings, "settings")?)
}

#[tauri::command]
pub fn acknowledge_completion(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let snapshot = {
        let mut current = lock(&state.snapshot, "snapshot")?;
        *lock(&state.acknowledged_completed_until_ms, "acknowledgement")? = current.state.completed_until_ms;
        current.state = PetState {
            status: Status::Resting,
            completed_until_ms: None,
        };
        current.clone()
    };
    app.emit_to(ISLAND_LABEL, SNAPSHOT_EVENT, snapshot)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn dismiss_completed_task(app: AppHandle, state: State<'_, AppState>, turn_id: String) -> Result<(), String> {
    let snapshot = {
        let mut current = lock(&state.snapshot, "snapshot")?;
        let Some(task) = current.tasks.iter().find(|task| task.turn_id == turn_id) else { return Ok(()); };
        if task.status != TaskStatus::Completed { return Ok(()); }
        let dismissed = {
            let mut dismissed = lock(&state.dismissed_completed_task_ids, "dismissed completed tasks")?;
            dismissed.insert(turn_id.clone());
            dismissed.iter().cloned().collect::<Vec<_>>()
        };
        persist_dismissed_completed_task_ids(&app, dismissed)?;
        current.tasks.retain(|task| task.turn_id != turn_id);
        current.clone()
    };
    app.emit_to(ISLAND_LABEL, SNAPSHOT_EVENT, snapshot).map_err(|error| error.to_string())
}

pub fn run(codex_home: Option<PathBuf>) -> tauri::Result<()> {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Err(error) = restore_island(app) {
                report_error(app, error);
            }
        }))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_positioner::init())
        .setup(move |app| setup_app(app, codex_home.clone()))
        .invoke_handler(tauri::generate_handler![get_snapshot, get_settings, set_motion_enabled, set_autostart, hide_to_tray, acknowledge_completion, dismiss_completed_task])
        .on_window_event(|window, event| {
            if window.label() == ISLAND_LABEL {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Err(error) = window.hide() {
                        report_error(window.app_handle(), error.to_string());
                    }
                }
            }
        })
        .run(tauri::generate_context!())
}

fn setup_app(app: &mut App, supplied_codex_home: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let store = app.store(SETTINGS_STORE)?;
    let mut settings = match store.get(SETTINGS_KEY) {
        Some(value) => serde_json::from_value(value)?,
        None => Settings::default(),
    };
    let dismissed_completed_task_ids = store
        .get(DISMISSED_COMPLETED_TASKS_KEY)
        .and_then(|value| serde_json::from_value::<Vec<String>>(value).ok())
        .unwrap_or_default()
        .into_iter()
        .collect();
    settings.autostart_enabled = app.autolaunch().is_enabled()?;

    let restore = MenuItem::with_id(app, RESTORE_ID, "Restore", true, None::<&str>)?;
    let motion = CheckMenuItem::with_id(app, MOTION_ID, "Motion", true, settings.motion_enabled, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(app, AUTOSTART_ID, "Autostart", true, settings.autostart_enabled, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&restore, &motion, &autostart, &quit])?;

    let codex_home = resolve_codex_home(supplied_codex_home)?;
    let adapter = CodexDataAdapter::new(CodexPaths { home: codex_home });
    let snapshot = PetSnapshot {
        state: PetState { status: Status::Resting, completed_until_ms: None },
        usage: UsageSnapshot { today_tokens: None, weekly_usage_percent: None, weekly_reset_at_ms: None, synced_at_ms: None },
        sync_state: SyncState::Unavailable,
        tasks: Vec::new(),
    };
    if !app.manage(AppState {
        settings: Mutex::new(settings),
        snapshot: Mutex::new(snapshot),
        acknowledged_completed_until_ms: Mutex::new(None),
        dismissed_completed_task_ids: Mutex::new(dismissed_completed_task_ids),
        motion_item: motion,
        autostart_item: autostart,
    }) {
        return Err(std::io::Error::other("application state was already managed").into());
    }

    let icon = app.default_window_icon().cloned().ok_or_else(|| std::io::Error::other("the tray icon is not configured"))?;
    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if let Err(error) = handle_tray_menu(app, &event) {
                report_error(app, error);
            }
        })
        .on_tray_icon_event(|tray, event| {
            tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
            if matches!(event, TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }) {
                if let Err(error) = restore_island(tray.app_handle()) {
                    report_error(tray.app_handle(), error);
                }
            }
        })
        .build(app)?;

    let window = app.get_webview_window(ISLAND_LABEL).ok_or_else(|| std::io::Error::other("island window is not configured"))?;
    window.move_window(Position::BottomRight)?;
    start_poller(app.handle().clone(), adapter)?;
    Ok(())
}

fn handle_tray_menu(app: &AppHandle, event: &MenuEvent) -> Result<(), String> {
    let state = app.try_state::<AppState>().ok_or_else(|| "application state is unavailable".to_owned())?;
    match event.id().as_ref() {
        RESTORE_ID => restore_island(app),
        MOTION_ID => {
            let enabled = !lock(&state.settings, "settings")?.motion_enabled;
            update_settings(app, &state, |settings| settings.motion_enabled = enabled)?;
            state.motion_item.set_checked(enabled).map_err(|error| error.to_string())
        }
        AUTOSTART_ID => {
            let enabled = !lock(&state.settings, "settings")?.autostart_enabled;
            set_autostart_value(app, &state, enabled)?;
            state.autostart_item.set_checked(enabled).map_err(|error| error.to_string())
        }
        QUIT_ID => {
            app.exit(0);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn set_autostart_value(app: &AppHandle, state: &AppState, enabled: bool) -> Result<(), String> {
    let previous = lock(&state.settings, "settings")?.autostart_enabled;
    if enabled { app.autolaunch().enable() } else { app.autolaunch().disable() }.map_err(|error| error.to_string())?;
    if let Err(error) = update_settings(app, state, |settings| settings.autostart_enabled = enabled) {
        let rollback = if previous { app.autolaunch().enable() } else { app.autolaunch().disable() };
        return match rollback {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(format!("{error}; failed to restore autostart state: {rollback_error}")),
        };
    }
    Ok(())
}

fn update_settings(app: &AppHandle, state: &AppState, update: impl FnOnce(&mut Settings)) -> Result<(), String> {
    let mut current = lock(&state.settings, "settings")?;
    let mut next = *current;
    update(&mut next);
    persist_settings(app, next)?;
    *current = next;
    app.emit_to(ISLAND_LABEL, SETTINGS_EVENT, next).map_err(|error| error.to_string())?;
    Ok(())
}

fn persist_settings(app: &AppHandle, settings: Settings) -> Result<(), String> {
    let store = app.store(SETTINGS_STORE).map_err(|error| error.to_string())?;
    let value = serde_json::to_value(settings).map_err(|error| error.to_string())?;
    store.set(SETTINGS_KEY, value);
    store.save().map_err(|error| error.to_string())
}

fn persist_dismissed_completed_task_ids(app: &AppHandle, task_ids: Vec<String>) -> Result<(), String> {
    let store = app.store(SETTINGS_STORE).map_err(|error| error.to_string())?;
    let value = serde_json::to_value(task_ids).map_err(|error| error.to_string())?;
    store.set(DISMISSED_COMPLETED_TASKS_KEY, value);
    store.save().map_err(|error| error.to_string())
}

fn restore_island(app: &AppHandle) -> Result<(), String> {
    let window = app.get_webview_window(ISLAND_LABEL).ok_or_else(|| "island window is unavailable".to_owned())?;
    window.show().map_err(|error| error.to_string())?;
    window.unminimize().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

fn start_poller(app: AppHandle, mut adapter: CodexDataAdapter) -> Result<(), std::io::Error> {
    thread::Builder::new().name("codex-adapter-poller".to_owned()).spawn(move || loop {
        let now = match now_ms() {
            Ok(now) => now,
            Err(error) => {
                report_error(&app, error.to_string());
                continue;
            }
        };
        let mut snapshot = adapter.poll(now);
        let Some(state) = app.try_state::<AppState>() else { break };
        let acknowledged_until = match lock(&state.acknowledged_completed_until_ms, "acknowledgement") {
            Ok(value) => *value,
            Err(error) => {
                report_error(&app, error);
                continue;
            }
        };
        if snapshot.state.status == Status::Completed
            && snapshot.state.completed_until_ms.is_some_and(|until| acknowledged_until.is_some_and(|acknowledged| until <= acknowledged))
        {
            snapshot.state = PetState { status: Status::Resting, completed_until_ms: None };
        }
        let dismissed = match lock(&state.dismissed_completed_task_ids, "dismissed completed tasks") {
            Ok(tasks) => tasks.clone(),
            Err(error) => {
                report_error(&app, error);
                continue;
            }
        };
        snapshot.tasks.retain(|task| !dismissed.contains(&task.turn_id));
        match lock(&state.snapshot, "snapshot") {
            Ok(mut current) => *current = snapshot.clone(),
            Err(error) => {
                report_error(&app, error);
                continue;
            }
        }
        if let Err(error) = app.emit_to(ISLAND_LABEL, SNAPSHOT_EVENT, snapshot) {
            report_error(&app, error.to_string());
        }
        thread::sleep(Duration::from_secs(2));
    })?;
    Ok(())
}

fn resolve_codex_home(supplied: Option<PathBuf>) -> Result<PathBuf, std::io::Error> {
    if let Some(path) = supplied.or_else(|| std::env::var_os("CODEX_HOME").map(PathBuf::from)) {
        return Ok(path);
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).ok_or_else(|| std::io::Error::other("home directory is unavailable"))?;
    Ok(PathBuf::from(home).join(".codex"))
}

fn now_ms() -> Result<u64, std::time::SystemTimeError> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64)
}

fn lock<'a, T>(mutex: &'a Mutex<T>, name: &str) -> Result<MutexGuard<'a, T>, String> {
    mutex.lock().map_err(|_| format!("{name} state lock is poisoned"))
}

fn report_error(app: &AppHandle, error: String) {
    eprintln!("Codex Pet Island: {error}");
    let _ = app.emit_to(ISLAND_LABEL, ERROR_EVENT, error);
}
