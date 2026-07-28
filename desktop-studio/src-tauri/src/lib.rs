mod capture_manager;
mod render_manager;

use capture_manager::{CaptureManager, CapturePayload};
use render_manager::{RenderManager, scan_render_directories, execute_render_batch, render_status, cancel_render_batch};

// ── Tauri commands ─────────────────────────────────────────────────────────────

/// Triggers a batch capture run from the Vite frontend.
///
/// Async so the Tauri dispatcher never blocks the main thread.
/// The heavy lifting is delegated inside `start_capture_batch_impl`
/// via `tokio::task::spawn_blocking`.
#[tauri::command]
async fn start_capture_batch(
    state: tauri::State<'_, CaptureManager>,
    payload: CapturePayload,
) -> Result<(), String> {
    capture_manager::start_capture_batch_impl(&state, payload).await
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
async fn scan_directory(paths: Vec<String>) -> Result<Vec<capture_manager::SerializedDemo>, String> {
    capture_manager::scan_directory_impl(paths).await
}

#[tauri::command]
async fn scan_demos(paths: Vec<String>) -> Result<Vec<capture_manager::SerializedDemo>, String> {
    capture_manager::scan_directory_impl(paths).await
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

// ── App entry point ────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(CaptureManager::new())
        .manage(RenderManager::new())
        .invoke_handler(tauri::generate_handler![
            test_bridge,
            validate_paths,
            analyze_demo,
            start_capture_batch,
            cancel_capture_batch,
            capture_status,
            scan_directory,
            scan_demos,
            calculate_export_pool_space,
            scan_render_directories,
            execute_render_batch,
            render_status,
            cancel_render_batch,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
