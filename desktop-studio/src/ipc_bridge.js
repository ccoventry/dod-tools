import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { showToast } from './toast.js';
import { STRINGS } from './strings.js';

/** Writes one line to today's activity log — for frontend-only events (the
 *  Master Queue's Clear Untracked/Selected/All and row-delete actions) that
 *  have no other IPC call to log from. Best-effort: logs to the console on
 *  failure rather than toasting, since a lost log line isn't worth
 *  interrupting the user over. */
export function logFrontendEvent(message) {
  invoke('log_frontend_event', { message }).catch((err) => {
    console.error("IPC Execution Error (log_frontend_event):", err);
  });
}

/** Opens Explorer with today's activity log pre-selected — AppData/logs
 *  isn't somewhere most users would think to go looking on their own. */
export async function openActivityLog() {
  return invoke('get_activity_log_path')
    .then((path) => revealItemInDir(path))
    .catch((err) => {
      console.error("IPC Execution Error (get_activity_log_path):", err);
      showToast(STRINGS.IPC.logFileOpenFailed(err), 'error');
    });
}

export async function scanDirectory(scanPaths) {
  return invoke("scan_directory", { paths: scanPaths })
    .catch((err) => {
      console.error("IPC Execution Error (scan_directory):", err);
      showToast(STRINGS.IPC.scanError(err), 'error');
      throw err;
    });
}

export async function validatePaths(hlaePath, hlPath) {
  return await invoke('validate_paths', { hlaePath, hlPath })
    .catch((err) => {
      console.error("IPC Execution Error (validate_paths):", err);
      showToast(STRINGS.IPC.validationError(err), 'error');
      throw err;
    });
}

export async function analyzeDemoFull(demoPath) {
  return await invoke('analyze_demo_full', { demoPath })
    .catch((err) => {
      console.error("IPC Execution Error (analyze_demo_full):", err);
      showToast(STRINGS.IPC.analysisError(err), 'error');
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
      showToast(STRINGS.IPC.previewFailed(err), 'error');
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
      showToast(STRINGS.IPC.processCheckFailed(err), 'error');
      throw err;
    });
}

/** Launches HLAE against `hl.exe` directly with no demo loaded, applying the
 *  persisted resolution/HUD/init-command settings. */
export async function launchStandaloneGame() {
  return invoke("launch_standalone_game")
    .catch((err) => {
      console.error("IPC Execution Error (launch_standalone_game):", err);
      showToast(STRINGS.IPC.launchFailed(err), 'error');
      throw err;
    });
}

/** Aggressively kills any running `hl.exe`/`hlae.exe` instances. */
export async function killEngineProcesses() {
  return invoke("kill_engine_processes")
    .catch((err) => {
      console.error("IPC Execution Error (kill_engine_processes):", err);
      showToast(STRINGS.IPC.killEngineFailed(err), 'error');
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
      showToast(STRINGS.IPC.batchPreviewFailed(err), 'error');
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

/** Per-path reason a Capture Output entry isn't usable — "ok" | "not_absolute"
 *  | "malformed" | "not_found" | "not_a_directory". Only worth calling once
 *  the aggregate space check has already found a problem; a per-path OS stat
 *  isn't cheap enough to run on every keystroke. */
export async function diagnoseCaptureOutputPaths(paths) {
  return invoke("diagnose_capture_output_paths", { paths: paths })
    .catch((err) => {
      console.error("IPC Execution Error (diagnose_capture_output_paths):", err);
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
    showToast(STRINGS.IPC.simulationError(err), 'error');
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
  //   { render_directories, codec, fps, ffmpeg_path?, export_directories, max_concurrent_renders }
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

export async function cancelRenderJob(jobId) {
  return invoke("cancel_render_job", { jobId })
    .catch((err) => {
      console.error("IPC Execution Error (cancel_render_job):", err);
      throw err;
    });
}

export async function resetRenderJob(jobId) {
  return invoke("reset_render_job", { jobId })
    .catch((err) => {
      console.error("IPC Execution Error (reset_render_job):", err);
      throw err;
    });
}

export async function getExportPoolFreeGb(directories) {
  return invoke("get_export_pool_free_gb", { directories })
    .catch((err) => {
      console.error("IPC Execution Error (get_export_pool_free_gb):", err);
      return 0;
    });
}

export async function checkRenderAutosave() {
  return invoke("check_render_autosave")
    .catch((err) => {
      console.error("IPC Execution Error (check_render_autosave):", err);
      return null;
    });
}

export async function discardRenderAutosave() {
  return invoke("discard_render_autosave")
    .catch((err) => {
      console.error("IPC Execution Error (discard_render_autosave):", err);
      throw err;
    });
}

export async function recoverRenderBatch() {
  return invoke("recover_render_batch")
    .catch((err) => {
      console.error("IPC Execution Error (recover_render_batch):", err);
      throw err;
    });
}

export async function cancelScan() {
  return invoke("cancel_scan")
    .catch((err) => {
      console.error("IPC Execution Error (cancel_scan):", err);
      showToast(STRINGS.IPC.cancelScanError(err), 'error');
      throw err;
    });
}

export async function getSettings() {
  return invoke("get_settings")
    .catch((err) => {
      console.error("IPC Execution Error (get_settings):", err);
      showToast(STRINGS.IPC.settingsLoadFailed(err), 'error');
      throw err;
    });
}

export async function saveSettings(settings) {
  return invoke("save_settings", { settings })
    .catch((err) => {
      console.error("IPC Execution Error (save_settings):", err);
      showToast(STRINGS.IPC.settingsSaveFailed(err), 'error');
      throw err;
    });
}

export async function runDemoAudit(paths) {
  return invoke("run_demo_audit", { paths })
    .catch((err) => {
      console.error("IPC Execution Error (run_demo_audit):", err);
      showToast(STRINGS.IPC.auditFailed(err), 'error');
      throw err;
    });
}

export async function deleteAuditFiles(paths) {
  return invoke("delete_audit_files", { paths })
    .catch((err) => {
      console.error("IPC Execution Error (delete_audit_files):", err);
      showToast(STRINGS.IPC.deletionFailed(err), 'error');
      throw err;
    });
}

export async function cancelAudit() {
  return invoke("cancel_audit")
    .catch((err) => {
      console.error("IPC Execution Error (cancel_audit):", err);
      showToast(STRINGS.IPC.cancelAuditError(err), 'error');
      throw err;
    });
}

export async function revealInExplorer(path) {
  return invoke("reveal_in_explorer", { path })
    .catch((err) => {
      console.error("IPC Execution Error (reveal_in_explorer):", err);
      showToast(STRINGS.IPC.folderOpenFailed(err), 'error');
      throw err;
    });
}

/** Sweeps `<hl>/dod` for orphaned `*_preview.dem` bookmark previews (files
 *  that still carry their `.dodtools_preview` sidecar) left behind by prior
 *  capture sessions. `gameDir` is the configured hl.exe path. */
export async function scanOrphanedPreviews(gameDir) {
  return invoke("scan_orphaned_previews", { gameDir })
    .catch((err) => {
      console.error("IPC Execution Error (scan_orphaned_previews):", err);
      showToast(STRINGS.IPC.previewScanFailed(err), 'error');
      throw err;
    });
}

/** Deletes the given orphaned preview demos (and their sidecars). Resolves
 *  to the number of preview demos actually removed. */
export async function deleteOrphanedPreviews(filePaths) {
  return invoke("delete_orphaned_previews", { filePaths })
    .catch((err) => {
      console.error("IPC Execution Error (delete_orphaned_previews):", err);
      showToast(STRINGS.IPC.previewDeletionFailed(err), 'error');
      throw err;
    });
}

// NOTE: onRenderStatus and initRenderProgressListener have been intentionally
// removed from this module. render_pane.js registers 'render_jobs_snapshot'/
// 'render_batch_finished' listeners directly to avoid double-registration
// bugs (ipc_bridge.js must not create a second competing listener).

/** Lists the immediate subfolders and `.dem` files of `path` (drive roots
 *  when `path` is null/undefined) for the Demo Analyzer's folder/demo picker
 *  widgets. Errors (e.g. permission denied) are surfaced inline by the
 *  caller rather than as a global toast, since browsing into an
 *  inaccessible folder is an expected, recoverable event. */
export async function browseDirectory(path) {
  return invoke("browse_directory", { path: path ?? null })
    .catch((err) => {
      console.error("IPC Execution Error (browse_directory):", err);
      throw err;
    });
}

export async function defaultBrowseDir() {
  return invoke("default_browse_dir")
    .catch((err) => {
      console.error("IPC Execution Error (default_browse_dir):", err);
      return null;
    });
}

/** Non-recursive `.dem` count for a single folder — used by the Explorer
 *  sidebar's Quick Links rows (Pinned/Recent/Local). */
export async function countDemoFiles(path) {
  return invoke("count_demo_files_in_folder", { path })
    .catch((err) => {
      console.error("IPC Execution Error (count_demo_files_in_folder):", err);
      return 0;
    });
}

/** Bounded background scan (depth-4, 2000-folder cap) for folders containing
 *  at least one `.dem` file, rooted at `root` (or the default browse dir).
 *  Feeds the Explorer sidebar's "Local" Quick Links tier. */
export async function scanDemoFolders(root) {
  return invoke("scan_demo_folders", { root: root ?? null })
    .catch((err) => {
      console.error("IPC Execution Error (scan_demo_folders):", err);
      return [];
    });
}
