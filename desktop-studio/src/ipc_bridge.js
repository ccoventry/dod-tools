import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { showToast } from './toast.js';

export async function scanDirectory(scanPaths) {
  return invoke("scan_directory", { paths: scanPaths })
    .catch((err) => {
      console.error("IPC Execution Error (scan_directory):", err);
      showToast(`Scan error: ${err}`, 'error');
      throw err;
    });
}

export async function validatePaths(hlaePath, hlPath) {
  return await invoke('validate_paths', { hlaePath, hlPath })
    .catch((err) => {
      console.error("IPC Execution Error (validate_paths):", err);
      showToast(`Validation error: ${err}`, 'error');
      throw err;
    });
}

export async function analyzeDemo(demoPath) {
  return await invoke('analyze_demo', { demoPath })
    .catch((err) => {
      console.error("IPC Execution Error (analyze_demo):", err);
      showToast(`Analysis error: ${err}`, 'error');
      throw err;
    });
}

export async function analyzeDemoFull(demoPath) {
  return await invoke('analyze_demo_full', { demoPath })
    .catch((err) => {
      console.error("IPC Execution Error (analyze_demo_full):", err);
      showToast(`Analysis error: ${err}`, 'error');
      throw err;
    });
}

export async function startCaptureBatch(payload) {
  return invoke("start_capture_batch", { payload: payload })
    .catch((err) => {
      console.error("IPC Execution Error (start_capture_batch):", err);
      throw err;
    });
}

/** Patches the given demo's selected highlights into a single `<stem>_preview.dem`
 *  (BOOKMARK/director events at each highlight) and immediately launches it in
 *  HLAE via `+viewdemo`. */
export async function launchDemoPreview(hlaePath, gamePath, streaks) {
  return invoke("launch_demo_preview", { hlaePath, gamePath, streaks })
    .catch((err) => {
      console.error("IPC Execution Error (launch_demo_preview):", err);
      showToast(`Preview failed: ${err}`, 'error');
      throw err;
    });
}

/** True if an `hl.exe`/`hlae.exe` instance is already running — used as a
 *  pre-flight guard before `launchDemoPreview` so a stale HLAE session
 *  doesn't corrupt the freshly-patched preview demo. */
export async function checkEngineProcesses() {
  return invoke("check_engine_processes")
    .catch((err) => {
      console.error("IPC Execution Error (check_engine_processes):", err);
      showToast(`Process check failed: ${err}`, 'error');
      throw err;
    });
}

/** Aggressively kills any running `hl.exe`/`hlae.exe` instances. */
export async function killEngineProcesses() {
  return invoke("kill_engine_processes")
    .catch((err) => {
      console.error("IPC Execution Error (kill_engine_processes):", err);
      showToast(`Failed to close running engine processes: ${err}`, 'error');
      throw err;
    });
}

/** Patches every demo with selected highlights into its own `<stem>_preview.dem`
 *  (grouped server-side by source demo) without launching HLAE. Resolves to the
 *  number of preview demos generated. */
export async function generateAllPreviews(hlaePath, gamePath, streaks) {
  return invoke("generate_all_previews", { hlaePath, gamePath, streaks })
    .catch((err) => {
      console.error("IPC Execution Error (generate_all_previews):", err);
      showToast(`Batch preview generation failed: ${err}`, 'error');
      throw err;
    });
}

export async function cancelCaptureBatch() {
  return invoke("cancel_capture_batch")
    .catch((err) => {
      console.error("IPC Execution Error (cancel_capture_batch):", err);
      throw err;
    });
}

export async function getCaptureStatus() {
  return invoke("capture_status")
    .catch((err) => {
      console.error("IPC Execution Error (capture_status):", err);
      throw err;
    });
}

export async function calculateExportPoolSpace(paths) {
  return invoke("calculate_export_pool_space", { paths: paths })
    .catch((err) => {
      console.error("IPC Execution Error (calculate_export_pool_space):", err);
      throw err;
    });
}

export async function simulateAotCapacity(streaks, fps, bytesPerFrame, availableBytes) {
  return invoke("simulate_aot_capacity", { 
    streaks, 
    fps, 
    bytesPerFrame, 
    availableBytes 
  }).catch((err) => {
    console.error("IPC Execution Error (simulate_aot_capacity):", err);
    showToast(`Simulation error: ${err}`, 'error');
    throw err;
  });
}

export async function scanRenderDirectories(renderFolders) {
  return invoke("scan_render_directories", { paths: renderFolders })
    .catch((err) => {
      console.error("IPC Execution Error (scan_render_directories):", err);
      throw err;
    });
}

export async function executeRenderBatch(payload) {
  // payload must match RenderBatchPayload:
  //   { render_directories, codec, fps, ffmpeg_path?, export_directory? }
  return invoke("execute_render_batch", { payload: payload })
    .catch((err) => {
      console.error("IPC Execution Error (execute_render_batch):", err);
      throw err;
    });
}

export async function cancelRenderBatch() {
  return invoke("cancel_render_batch")
    .catch((err) => {
      console.error("IPC Execution Error (cancel_render_batch):", err);
      throw err;
    });
}

export async function cancelScan() {
  return invoke("cancel_scan")
    .catch((err) => {
      console.error("IPC Execution Error (cancel_scan):", err);
      showToast(`Cancel scan error: ${err}`, 'error');
      throw err;
    });
}

export async function getSettings() {
  return invoke("get_settings")
    .catch((err) => {
      console.error("IPC Execution Error (get_settings):", err);
      showToast(`Failed to load settings: ${err}`, 'error');
      throw err;
    });
}

export async function saveSettings(settings) {
  return invoke("save_settings", { settings })
    .catch((err) => {
      console.error("IPC Execution Error (save_settings):", err);
      showToast(`Failed to save settings: ${err}`, 'error');
      throw err;
    });
}

export async function runDemoAudit(paths) {
  return invoke("run_demo_audit", { paths })
    .catch((err) => {
      console.error("IPC Execution Error (run_demo_audit):", err);
      showToast(`Audit failed: ${err}`, 'error');
      throw err;
    });
}

export async function deleteAuditFiles(paths) {
  return invoke("delete_audit_files", { paths })
    .catch((err) => {
      console.error("IPC Execution Error (delete_audit_files):", err);
      showToast(`Deletion failed: ${err}`, 'error');
      throw err;
    });
}

// NOTE: onRenderStatus and initRenderProgressListener have been intentionally
// removed from this module. render_pane.js registers a single
// listen('render_status', ...) listener directly to avoid double-registration
// bugs (ipc_bridge.js must not create a second competing listener).
