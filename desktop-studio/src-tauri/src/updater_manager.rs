use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Fixed manifest URLs — one GitHub Release per channel. `stable` is
/// published to the repo's `/releases/latest` alias (release_stable.yml,
/// cut from `main`); `dev` is republished in place under a fixed
/// `dev-latest` prerelease tag (release_dev.yml, cut from `dev` on demand).
/// See issue #133 / docs/archive is not relevant here — this is new.
const STABLE_ENDPOINT: &str =
    "https://github.com/ccoventry/dod-tools/releases/latest/download/latest.json";
const DEV_ENDPOINT: &str =
    "https://github.com/ccoventry/dod-tools/releases/download/dev-latest/latest.json";

fn endpoint_for_channel(channel: &str) -> Result<url::Url, String> {
    let raw = match channel {
        "dev" => DEV_ENDPOINT,
        "stable" => STABLE_ENDPOINT,
        other => return Err(format!("Unknown update channel: {}", other)),
    };
    url::Url::parse(raw).map_err(|e| format!("Invalid updater endpoint URL: {}", e))
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
}

/// Holds the `Update` handle a successful check produced, since it carries a
/// live download/signature-verify closure that can't cross the IPC boundary
/// — `download_and_install_update` reads it back out by channel.
#[derive(Default)]
pub struct UpdaterState {
    pub pending: Arc<Mutex<Option<Update>>>,
}

#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    state: tauri::State<'_, UpdaterState>,
    channel: String,
) -> Result<Option<UpdateInfo>, String> {
    let endpoint = endpoint_for_channel(&channel)?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|e| format!("Failed to set updater endpoint: {}", e))?
        .build()
        .map_err(|e| format!("Failed to build updater: {}", e))?;

    let update = updater
        .check()
        .await
        .map_err(|e| format!("Update check failed: {}", e))?;

    match update {
        Some(update) => {
            let info = UpdateInfo {
                version: update.version.clone(),
                current_version: update.current_version.clone(),
                notes: update.body.clone(),
                pub_date: update.date.map(|d| d.to_string()),
            };
            let pending = Arc::clone(&state.pending);
            let mut guard = pending.lock().unwrap_or_else(|p| p.into_inner());
            *guard = Some(update);
            Ok(Some(info))
        }
        None => {
            let pending = Arc::clone(&state.pending);
            let mut guard = pending.lock().unwrap_or_else(|p| p.into_inner());
            *guard = None;
            Ok(None)
        }
    }
}

#[tauri::command]
pub async fn download_and_install_update(
    app: AppHandle,
    state: tauri::State<'_, UpdaterState>,
) -> Result<(), String> {
    let pending = Arc::clone(&state.pending);
    let update = pending
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take()
        .ok_or_else(|| "No update available to install — call check_for_update first".to_string())?;

    let progress_app = app.clone();
    let mut downloaded: u64 = 0;
    update
        .download_and_install(
            move |chunk_length, content_length| {
                downloaded += chunk_length as u64;
                let _ = progress_app.emit(
                    "update_download_progress",
                    serde_json::json!({
                        "downloaded": downloaded,
                        "total": content_length,
                    }),
                );
            },
            move || {
                let _ = app.emit("update_ready", ());
            },
        )
        .await
        .map_err(|e| format!("Update download/install failed: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart();
}
