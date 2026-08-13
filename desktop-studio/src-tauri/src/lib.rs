mod capture_manager;
mod render_manager;
mod settings_manager;
mod audit_manager;

use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use capture_manager::{CaptureManager, CapturePayload};
use render_manager::{RenderManager, scan_render_directories, execute_render_batch, render_status, cancel_render_batch};
use settings_manager::{AppSettings, SettingsManager};
use audit_manager::{AuditManager, SerializedDuplicateGroup};

// ── ScanManager ────────────────────────────────────────────────────────────────

pub struct ScanManager {
    pub is_scanning: Arc<AtomicBool>,
    pub cancel_token: Arc<AtomicBool>,
}

impl Default for ScanManager {
    fn default() -> Self {
        Self {
            is_scanning: Arc::new(AtomicBool::new(false)),
            cancel_token: Arc::new(AtomicBool::new(false)),
        }
    }
}

// ── Settings IPC Commands ──────────────────────────────────────────────────────

#[tauri::command]
async fn get_settings(state: tauri::State<'_, SettingsManager>) -> Result<AppSettings, String> {
    let inner_arc = Arc::clone(&state.inner);
    tokio::task::spawn_blocking(move || {
        let guard = inner_arc.lock().unwrap_or_else(|p| p.into_inner());
        Ok(guard.clone())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
async fn save_settings(
    state: tauri::State<'_, SettingsManager>,
    settings: AppSettings,
) -> Result<(), String> {
    let inner_arc = Arc::clone(&state.inner);
    tokio::task::spawn_blocking(move || {
        settings.save()?;
        let mut guard = inner_arc.lock().unwrap_or_else(|p| p.into_inner());
        *guard = settings;
        Ok(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

// ── Auditor IPC Commands ───────────────────────────────────────────────────────

#[tauri::command]
async fn run_demo_audit(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AuditManager>,
    paths: Vec<String>,
) -> Result<Vec<SerializedDuplicateGroup>, String> {
    audit_manager::run_demo_audit_impl(
        app_handle,
        Arc::clone(&state.is_running),
        Arc::clone(&state.cancel_token),
        paths,
    ).await
}

#[tauri::command]
fn delete_audit_files(paths: Vec<String>) -> Result<(), String> {
    audit_manager::delete_audit_files_impl(paths)
}

#[tauri::command]
fn cancel_audit(state: tauri::State<'_, AuditManager>) -> Result<(), String> {
    state.cancel_token.store(true, Ordering::SeqCst);
    Ok(())
}

// ── Tauri commands ─────────────────────────────────────────────────────────────

/// Triggers a batch capture run from the Vite frontend.
///
/// Async so the Tauri dispatcher never blocks the main thread.
/// The heavy lifting is delegated inside `start_capture_batch_impl`
/// via `tokio::task::spawn_blocking`.
#[tauri::command]
async fn start_capture_batch(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, CaptureManager>,
    payload: CapturePayload,
) -> Result<(), String> {
    capture_manager::start_capture_batch_impl(app_handle, &state, payload).await
}

#[tauri::command]
fn simulate_aot_capacity(
    streaks: Vec<f32>,
    fps: u32,
    bytes_per_frame: u64,
    available_bytes: u64,
) -> Result<(u64, bool), String> {
    Ok(capture_manager::simulate_aot_capacity(
        streaks,
        fps,
        bytes_per_frame,
        available_bytes,
    ))
}

/// Cancel a running capture batch.
#[tauri::command]
async fn cancel_capture_batch(state: tauri::State<'_, CaptureManager>) -> Result<(), String> {
    state
        .cancel_token
        .store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// Returns whether a capture batch is currently running.
#[tauri::command]
fn capture_status(state: tauri::State<'_, CaptureManager>) -> bool {
    state.is_running()
}

/// Scaffolding command kept for bridge smoke-testing.
#[tauri::command]
fn test_bridge(path: String) -> String {
    format!("Tauri Backend received target: {}. Engine ready.", path)
}

#[tauri::command]
async fn scan_directory(
    app_handle: tauri::AppHandle,
    scan_state: tauri::State<'_, ScanManager>,
    paths: Vec<String>,
) -> Result<Vec<capture_manager::SerializedDemo>, String> {
    capture_manager::scan_directory_impl(
        app_handle,
        Arc::clone(&scan_state.is_scanning),
        Arc::clone(&scan_state.cancel_token),
        paths,
    ).await
}

#[tauri::command]
async fn scan_demos(
    app_handle: tauri::AppHandle,
    scan_state: tauri::State<'_, ScanManager>,
    paths: Vec<String>,
) -> Result<Vec<capture_manager::SerializedDemo>, String> {
    capture_manager::scan_directory_impl(
        app_handle,
        Arc::clone(&scan_state.is_scanning),
        Arc::clone(&scan_state.cancel_token),
        paths,
    ).await
}

#[tauri::command]
fn cancel_scan(scan_state: tauri::State<'_, ScanManager>) -> Result<(), String> {
    scan_state.cancel_token.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
fn calculate_export_pool_space(paths: Vec<String>) -> Result<u64, String> {
    let mut total: u64 = 0;
    for path_str in paths {
        let p = std::path::PathBuf::from(path_str);
        let space = native::sys::disk::get_available_bytes(&p);
        if space != u64::MAX {
            total += space;
        }
    }
    Ok(total)
}

#[tauri::command]
async fn validate_paths(hlae_path: String, hl_path: String) -> Result<bool, String> {
    let hlae_p = std::path::Path::new(&hlae_path);
    let hl_p = std::path::Path::new(&hl_path);

    if !hlae_p.exists() || !hlae_p.is_file() {
        return Err("HLAE executable not found at specified path.".into());
    }
    if !hl_p.exists() || !hl_p.is_file() {
        return Err("Half-Life executable not found at specified path.".into());
    }
    Ok(true)
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SerializedAnalysis {
    pub scoreboard: serde_json::Value,
    pub chat_logs: serde_json::Value,
    pub mortality_metrics: serde_json::Value,
    pub round_chronologies: serde_json::Value,
    pub file_info: serde_json::Value,
}

#[tauri::command]
async fn analyze_demo(demo_path: String) -> Result<SerializedAnalysis, String> {
    tokio::task::spawn_blocking(move || {
        let path = std::path::PathBuf::from(&demo_path);

        if !path.exists() || !path.is_file() {
            return Err(format!("Demo file not found: {}", demo_path));
        }

        match native::run_analyzer_with_progress(&path, |_, _| {}) {
            Ok((file_info, analysis)) => {
                let analysis_json = serde_json::to_value(&analysis)
                    .map_err(|e| format!("Serialization error (analysis): {}", e))?;
                let file_info_json = serde_json::to_value(&file_info)
                    .map_err(|e| format!("Serialization error (file_info): {}", e))?;

                let scoreboard = analysis_json
                    .get("scoreboard")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let chat_logs = analysis_json
                    .get("chat_logs")
                    .or_else(|| analysis_json.get("chat"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let mortality_metrics = analysis_json
                    .get("mortality_metrics")
                    .or_else(|| analysis_json.get("deaths"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let round_chronologies = analysis_json
                    .get("round_chronologies")
                    .or_else(|| analysis_json.get("rounds"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);

                Ok(SerializedAnalysis {
                    scoreboard,
                    chat_logs,
                    mortality_metrics,
                    round_chronologies,
                    file_info: file_info_json,
                })
            }
            Err(e) => Err(format!("Analyzer error: {}", e)),
        }
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

// ── Full-fidelity analysis payload for the standalone Demo Analyzer tab ─────────
// Unlike `SerializedAnalysis` (which flattens a handful of sections into loose
// JSON for the compact inline telemetry summary), this passes the typed
// `analysis::DemoInfo`/`AnalyzerState` straight through so the frontend can
// reconstruct every report sub-view (Summary/Scoreboard/Player Details/Team
// Details/Timeline/Rounds/Chat) with full fidelity, matching the data the
// egui report views on `dev` were built from.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct AnalyzerReportPayload {
    pub file_name: String,
    pub file_path: String,
    pub file_dir: String,
    pub file_size_mb: f64,
    pub file_created_unix_secs: f64,
    pub demo_info: analysis::DemoInfo,
    pub state: analysis::AnalyzerState,
}

#[tauri::command]
async fn analyze_demo_full(demo_path: String) -> Result<AnalyzerReportPayload, String> {
    tokio::task::spawn_blocking(move || {
        let path = std::path::PathBuf::from(&demo_path);

        if !path.exists() || !path.is_file() {
            return Err(format!("Demo file not found: {}", demo_path));
        }

        match native::run_analyzer_with_progress(&path, |_, _| {}) {
            Ok((file_info, analysis)) => {
                // Independent metadata lookup (not `file_info.created_at`) to avoid
                // depending on `web_time::SystemTime`'s exact type identity here.
                let created_unix_secs = std::fs::metadata(&path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0);

                let file_dir = std::path::Path::new(&file_info.path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                Ok(AnalyzerReportPayload {
                    file_name: file_info.name,
                    file_path: file_info.path,
                    file_dir,
                    file_size_mb: file_info.size_bytes as f64 / 1_048_576.0,
                    file_created_unix_secs: created_unix_secs,
                    demo_info: analysis.demo_info,
                    state: analysis.state,
                })
            }
            Err(e) => Err(format!("Analyzer error: {}", e)),
        }
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

// ── App entry point ────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(CaptureManager::new())
        .manage(RenderManager::new())
        .manage(ScanManager::default())
        .manage(SettingsManager::new())
        .manage(AuditManager::default())
        .invoke_handler(tauri::generate_handler![
            test_bridge,
            validate_paths,
            analyze_demo,
            analyze_demo_full,
            start_capture_batch,
            cancel_capture_batch,
            capture_status,
            scan_directory,
            scan_demos,
            cancel_scan,
            calculate_export_pool_space,
            simulate_aot_capacity,
            scan_render_directories,
            execute_render_batch,
            render_status,
            cancel_render_batch,
            get_settings,
            save_settings,
            run_demo_audit,
            delete_audit_files,
            cancel_audit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
