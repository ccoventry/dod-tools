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
            start_capture_batch,
            cancel_capture_batch,
            capture_status,
            scan_directory,
            scan_demos,
            scan_render_directories,
            execute_render_batch,
            render_status,
            cancel_render_batch,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
