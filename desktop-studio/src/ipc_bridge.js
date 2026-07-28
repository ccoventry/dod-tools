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

// NOTE: onRenderStatus and initRenderProgressListener have been intentionally
// removed from this module. render_pane.js registers a single
// listen('render_status', ...) listener directly to avoid double-registration
// bugs (ipc_bridge.js must not create a second competing listener).
