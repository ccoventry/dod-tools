import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export async function scanDirectory(scanPaths) {
  return invoke("scan_directory", { paths: scanPaths })
    .catch((err) => {
      console.error("IPC Execution Error (scan_directory):", err);
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
  return invoke("execute_render_batch", { payload: payload })
    .catch((err) => {
      console.error("IPC Execution Error (execute_render_batch):", err);
      throw err;
    });
}

export async function getRenderStatus() {
  return invoke("render_status")
    .catch((err) => {
      console.error("IPC Execution Error (render_status):", err);
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

export async function initRenderProgressListener(onStatusUpdate) {
  return listen('render_status', (event) => {
    if (onStatusUpdate) onStatusUpdate(event.payload);
  }).catch((err) => {
    console.error("IPC Execution Error (listen render_status):", err);
  });
}
