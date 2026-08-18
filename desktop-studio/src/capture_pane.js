import { startCaptureBatch, cancelCaptureBatch, validatePaths, calculateExportPoolSpace, scanOrphanedPreviews, deleteOrphanedPreviews, checkEngineProcesses, launchStandaloneGame } from './ipc_bridge.js';
import { listen } from '@tauri-apps/api/event';
import { showToast } from './toast.js';
import { requestProcessGuardedLaunch } from './detail_pane.js';
import { createListEditor } from './list_editor.js';

let unlistenCaptureStatus = null;
// Tracks whether a batch is actively running so refreshLaunchGuard() never
// re-enables Start Capture out from under the capture_status "running" lock.
let capturingInFlight = false;
// getState callback captured from initCaptureUI() so refreshLaunchGuard()
// can be called with no args from other panes (e.g. main.js after a target
// drive is added, or detail_pane.js after a streak selection changes).
let currentGetState = null;
// onSettingsChange callback captured from initCaptureUI() — main.js's
// persistAppSettings, wired here so Timing Options fields and Init/Custom
// Commands actually get written to settings.json on edit instead of only
// being saved incidentally whenever some unrelated action (e.g. browsing
// for hlae.exe) happens to also call it.
let currentOnSettingsChange = null;
// The most recent batch dispatched from this window: its session id and the
// live streak objects, in the exact order they were sent. The backend's take
// manifest indexes into that same order, which is what lets a verified block
// resolve back to the highlights it actually recorded.
let lastDispatch = null;
let unlistenTakesVerified = null;

function notifySettingsChange() {
  if (currentOnSettingsChange) currentOnSettingsChange();
}

/** Generates a `session_YYYYMMDD_HHMMSS` id so each batch routes into its own
 *  output subfolder instead of colliding in the export root (mirrors dev's
 *  `chrono::Local::now().format("%Y%m%d_%H%M%S")` stamp in widgets.rs). */
function generateSessionId() {
  const now = new Date();
  const pad = (n) => String(n).padStart(2, '0');
  return `session_${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}_${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
}

function updateRowBadges(statusText, colorHex) {
  const tableBody = document.querySelector('#master-demo-table-body');
  if (tableBody) {
    const statusSpans = tableBody.querySelectorAll('td span');
    statusSpans.forEach(span => {
      span.textContent = statusText;
      span.style.color = colorHex;
    });
  }
}

// ── Pre-Flight Disk Space Estimator ───────────────────────────────────────────

/**
 * Sums required capture bytes across every selected streak, merging
 * overlapping (or touching) pre/post-roll windows *within each source demo*
 * before billing them for disk space — two highlights that share footage
 * must not be double-counted, since the engine records that overlap once.
 * Base cost is `w * h * 3` bytes/frame at the configured capture FPS;
 * `separate_hud` triples the total (HUD pass recorded as its own stream).
 */
function computeRequiredCaptureBytes(currentScannedDemos, opts) {
  const { preRollSeconds, postRollSeconds, captureFps, resWidth, resHeight, separateHud } = opts;
  let totalSeconds = 0;

  (currentScannedDemos || []).forEach(demo => {
    const intervals = (demo.streaks || [])
      // Opt-in model (detail_pane.js): a streak counts as selected only once
      // explicitly checked. `undefined` covers both demos never opened in the
      // Highlight Details view and every non-recording-player streak (which
      // never renders as a checkable row at all) — neither should ever be
      // billed for capture space.
      .filter(streak => streak.selected === true)
      .map(streak => {
        const fps = streak.demo_fps || 100;
        const startSec = (streak.start_tick / fps) - preRollSeconds;
        const endSec = (streak.end_tick / fps) + postRollSeconds;
        return [startSec, endSec];
      })
      .sort((a, b) => a[0] - b[0]);

    let mergedStart = null;
    let mergedEnd = null;
    intervals.forEach(([start, end]) => {
      if (mergedStart === null) {
        mergedStart = start;
        mergedEnd = end;
      } else if (start <= mergedEnd) {
        mergedEnd = Math.max(mergedEnd, end);
      } else {
        totalSeconds += (mergedEnd - mergedStart);
        mergedStart = start;
        mergedEnd = end;
      }
    });
    if (mergedStart !== null) {
      totalSeconds += (mergedEnd - mergedStart);
    }
  });

  const frames = Math.ceil(Math.max(0, totalSeconds) * captureFps);
  const bytesPerFrame = resWidth * resHeight * 3;
  let requiredBytes = frames * bytesPerFrame;
  if (separateHud) requiredBytes *= 3;
  return requiredBytes;
}

/**
 * Recomputes required-vs-available disk space and hard-locks the Launch
 * button (rather than just toasting at click time) whenever the capture
 * pool can't cover it — including the zero-drive case, which previously
 * bypassed the check entirely because `availableBytes > 0` gated the old
 * warning. Safe to call with no args once `initCaptureUI` has run; other
 * panes (main.js, detail_pane.js) call it after anything that can move
 * required/available bytes: streak selection, target drives, timing/res
 * config fields.
 */
export async function refreshLaunchGuard(state) {
  const startBtn = document.querySelector('#start-capture-btn') || document.querySelector('#start-batch-btn');
  const warningEl = document.querySelector('#disk-space-warning-banner');
  if (!startBtn) return null;

  const resolvedState = state || (currentGetState ? currentGetState() : null) || { targetDrives: [], currentScannedDemos: [] };

  const preRollVal = parseFloat(document.querySelector("#config-pre-roll")?.value) || 2.0;
  const postRollVal = parseFloat(document.querySelector("#config-post-roll")?.value) || 0.6;
  const captureFpsVal = parseInt(document.querySelector("#config-capture-fps")?.value, 10) || 300;
  const resWidthVal = parseInt(document.querySelector("#config-res-width")?.value, 10) || 1280;
  const resHeightVal = parseInt(document.querySelector("#config-res-height")?.value, 10) || 720;
  const separateHudVal = document.querySelector("#config-separate-hud")?.checked || false;

  const requiredBytes = computeRequiredCaptureBytes(resolvedState.currentScannedDemos, {
    preRollSeconds: preRollVal,
    postRollSeconds: postRollVal,
    captureFps: captureFpsVal,
    resWidth: resWidthVal,
    resHeight: resHeightVal,
    separateHud: separateHudVal,
  });

  // Mirrors buildCapturePayload's outputDrivePool — Capture Output is the
  // sole (required) source of output directories now that Primary Media Dir
  // is gone.
  const effectiveDrivePool = (resolvedState.targetDrives || []).filter(Boolean);

  let availableBytes = 0;
  if (effectiveDrivePool.length > 0) {
    try {
      availableBytes = await calculateExportPoolSpace(effectiveDrivePool);
    } catch (err) {
      console.error("Error calculating export pool space for launch guard:", err);
    }
  }

  // Zero configured (or zero-space) drives must lock the button on its own —
  // `requiredBytes > availableBytes` alone would pass with requiredBytes 0.
  const noDrivesConfigured = effectiveDrivePool.length === 0 || availableBytes === 0;
  const insufficientSpace = !noDrivesConfigured && requiredBytes > availableBytes;
  const blocked = noDrivesConfigured || insufficientSpace;

  if (!capturingInFlight) {
    startBtn.disabled = blocked;
  }

  if (warningEl) {
    if (noDrivesConfigured) {
      warningEl.textContent = "No Capture Output directories configured — add at least one with free space before starting a capture.";
      warningEl.style.display = 'block';
    } else if (insufficientSpace) {
      warningEl.textContent = `Insufficient disk space: capture needs ~${(requiredBytes / 1e9).toFixed(2)} GB, only ${(availableBytes / 1e9).toFixed(2)} GB available across the export pool.`;
      warningEl.style.display = 'block';
    } else {
      warningEl.style.display = 'none';
      warningEl.textContent = '';
    }
  }

  const footerRequiredEl = document.querySelector('#footer-required-space');
  if (footerRequiredEl) {
    const requiredGb = (requiredBytes / (1024 * 1024 * 1024)).toFixed(2);
    footerRequiredEl.textContent = `Required: ${requiredGb} GB`;
    footerRequiredEl.style.color = insufficientSpace ? '#f44336' : '#4caf50';
  }

  return { requiredBytes, availableBytes, blocked };
}

// ── Custom Engine Commands (Init / Before-After) ──────────────────────────────
// Local to the Batch Capture Config panel — nothing else in the app reads
// these, unlike scanPaths/targetDrives which main.js owns for cross-pane use.

let initCommands = [];
let customCommands = [];
let initCommandsEditor = null;
let customCommandsEditor = null;

/** Scrapes the current Init/Custom Commands state for settings persistence
 *  (raw, untrimmed — mirrors in-progress edits rather than the filtered
 *  shape `buildCapturePayload` sends to `start_capture_batch`). */
export function getCommandsState() {
  return {
    init_commands: [...initCommands],
    custom_commands: customCommands.map(c => ({
      command: c.command,
      relation: c.relation === 'After' ? 'After' : 'Before',
      offset_seconds: c.offsetSeconds,
    })),
  };
}

/** Hydrates Init/Custom Commands state from persisted settings and
 *  re-renders both lists so they appear in the UI on boot. */
export function hydrateCommandsState(persistedInitCommands, persistedCustomCommands) {
  initCommands = Array.isArray(persistedInitCommands) ? [...persistedInitCommands] : [];
  customCommands = Array.isArray(persistedCustomCommands)
    ? persistedCustomCommands.map(c => ({
        command: c.command || '',
        relation: c.relation === 'After' ? 'After' : 'Before',
        offsetSeconds: typeof c.offset_seconds === 'number' ? c.offset_seconds : 2.0,
      }))
    : [];
  initCommandsEditor?.render();
  customCommandsEditor?.render();
}

// ── Clear Previews audit modal ─────────────────────────────────────────────
//
// Audits `<hl>/dod` for orphaned `*_preview.dem` bookmark previews (see the
// block comment above `patch_bookmark_previews` in capture_manager.rs) left
// behind across capture sessions, and lets the user purge them.

let currentPreviewScanResults = [];

function formatPreviewSize(bytes) {
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

function updateClearPreviewsDeleteButtonState() {
  const deleteBtn = document.querySelector('#clear-previews-delete-btn');
  if (!deleteBtn) return;
  const checked = document.querySelectorAll('.clear-previews-row-cb:checked');
  deleteBtn.disabled = checked.length === 0;
  deleteBtn.textContent = checked.length > 0 ? `Delete ${checked.length} Selected` : 'Delete Selected';
}

function renderClearPreviewsResults() {
  const tbody = document.querySelector('#clear-previews-body');
  const footerEl = document.querySelector('#clear-previews-footer');
  if (!tbody) return;

  if (currentPreviewScanResults.length === 0) {
    tbody.innerHTML = '<tr><td colspan="4" class="table-empty">No orphaned preview demos found.</td></tr>';
    if (footerEl) footerEl.textContent = 'Found: 0 | Reclaimable: 0.00 GB';
    updateClearPreviewsDeleteButtonState();
    return;
  }

  tbody.innerHTML = '';
  let totalBytes = 0;

  currentPreviewScanResults.forEach((entry) => {
    totalBytes += entry.size_bytes;

    const tr = document.createElement('tr');

    const tdCb = document.createElement('td');
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.className = 'clear-previews-row-cb';
    cb.dataset.path = entry.demo_path;
    cb.checked = true;
    cb.addEventListener('change', updateClearPreviewsDeleteButtonState);
    tdCb.appendChild(cb);

    const tdFile = document.createElement('td');
    tdFile.textContent = entry.file_name;

    const tdSize = document.createElement('td');
    tdSize.textContent = formatPreviewSize(entry.size_bytes);

    const tdModified = document.createElement('td');
    tdModified.textContent = entry.modified_unix_secs
      ? new Date(entry.modified_unix_secs * 1000).toLocaleString()
      : '—';

    tr.appendChild(tdCb);
    tr.appendChild(tdFile);
    tr.appendChild(tdSize);
    tr.appendChild(tdModified);
    tbody.appendChild(tr);
  });

  const totalGb = (totalBytes / (1024 * 1024 * 1024)).toFixed(2);
  if (footerEl) footerEl.textContent = `Found: ${currentPreviewScanResults.length} | Reclaimable: ${totalGb} GB`;
  updateClearPreviewsDeleteButtonState();
}

// ── Standalone Game Launch ───────────────────────────────────────────────────
//
// Boots HLAE against hl.exe with no demo loaded. Routes through the same
// running-process guard as the per-demo preview launchers (detail_pane.js) —
// on conflict, the intent is parked behind the shared Preview Detector modal
// via `requestProcessGuardedLaunch` instead of duplicating that modal's
// click listeners here.

function initStandaloneLaunchButton() {
  const btn = document.querySelector('#btn-launch-standalone-game');
  if (!btn) return;

  async function performLaunch() {
    btn.disabled = true;
    const originalLabel = btn.textContent;
    btn.textContent = 'Launching…';
    try {
      await launchStandaloneGame();
      showToast('Launching HLAE...', 'info');
    } catch (err) {
      // Already toasted by ipc_bridge.js.
    } finally {
      btn.textContent = originalLabel;
      btn.disabled = false;
    }
  }

  btn.addEventListener('click', async () => {
    let engineAlreadyRunning = false;
    try {
      engineAlreadyRunning = await checkEngineProcesses();
    } catch (err) {
      // Already toasted by ipc_bridge.js — fail open rather than blocking
      // a legitimate launch just because the detector itself errored.
    }

    if (engineAlreadyRunning) {
      requestProcessGuardedLaunch(performLaunch);
      return;
    }

    await performLaunch();
  });
}

function initClearPreviewsModal() {
  const openBtn = document.querySelector('#open-clear-previews-btn');
  const modal = document.querySelector('#clear-previews-modal');
  if (!openBtn || !modal) return;

  const statusEl = document.querySelector('#clear-previews-status');
  const closeBtn = document.querySelector('#clear-previews-close-btn');
  const deleteBtn = document.querySelector('#clear-previews-delete-btn');
  const selectAllBtn = document.querySelector('#clear-previews-select-all-btn');
  const tbody = document.querySelector('#clear-previews-body');

  openBtn.addEventListener('click', async () => {
    const gameDir = document.querySelector('#hl-path-input')?.value?.trim() || '';
    if (!gameDir) {
      showToast("Configure the Half-Life Executable (hl.exe) path before auditing previews.", 'error');
      return;
    }

    modal.style.display = 'flex';
    currentPreviewScanResults = [];
    if (statusEl) statusEl.textContent = 'Scanning for orphaned preview demos...';
    if (tbody) tbody.innerHTML = '<tr><td colspan="4" class="table-empty">Scanning...</td></tr>';
    updateClearPreviewsDeleteButtonState();

    try {
      currentPreviewScanResults = await scanOrphanedPreviews(gameDir);
      if (statusEl) statusEl.textContent = 'Scan complete.';
      renderClearPreviewsResults();
    } catch (e) {
      if (statusEl) statusEl.textContent = 'Scan failed.';
      if (tbody) tbody.innerHTML = `<tr><td colspan="4" class="table-empty">Scan failed: ${e}</td></tr>`;
    }
  });

  if (closeBtn) {
    closeBtn.addEventListener('click', () => {
      modal.style.display = 'none';
    });
  }

  if (selectAllBtn) {
    selectAllBtn.addEventListener('click', () => {
      const boxes = document.querySelectorAll('.clear-previews-row-cb');
      const allChecked = boxes.length > 0 && Array.from(boxes).every(cb => cb.checked);
      boxes.forEach(cb => { cb.checked = !allChecked; });
      updateClearPreviewsDeleteButtonState();
    });
  }

  if (deleteBtn) {
    deleteBtn.addEventListener('click', async () => {
      const checked = document.querySelectorAll('.clear-previews-row-cb:checked');
      const pathsToDelete = Array.from(checked).map(cb => cb.dataset.path);
      if (pathsToDelete.length === 0) return;
      if (!confirm(`Permanently delete ${pathsToDelete.length} orphaned preview demo(s)?`)) return;

      deleteBtn.disabled = true;
      try {
        const deletedCount = await deleteOrphanedPreviews(pathsToDelete);
        showToast(`Deleted ${deletedCount} orphaned preview demo(s).`, 'success');
        currentPreviewScanResults = currentPreviewScanResults.filter(entry => !pathsToDelete.includes(entry.demo_path));
        renderClearPreviewsResults();
      } catch (e) {
        showToast(`Deletion failed: ${e}`, 'error');
        updateClearPreviewsDeleteButtonState();
      }
    });
  }
}

export function initCaptureUI(getState, onSettingsChange) {
  const startBtn = document.querySelector('#start-capture-btn') || document.querySelector('#start-batch-btn');
  const cancelBtn = document.querySelector('#cancel-batch-btn');
  const statusEl = document.querySelector('#batch-status');
  const progressContainer = document.querySelector('#capture-progress-container');
  const progressBar = document.querySelector('#capture-progress-bar');

  currentGetState = getState;
  currentOnSettingsChange = onSettingsChange || null;

  initCommandsEditor = createListEditor({
    container: document.querySelector('#init-commands-list'),
    getItems: () => initCommands,
    fields: [{ key: 'value', type: 'text', primitive: true, placeholder: 'e.g. mirv_streams add all' }],
    onChange: notifySettingsChange,
  });

  customCommandsEditor = createListEditor({
    container: document.querySelector('#custom-commands-list'),
    getItems: () => customCommands,
    fields: [
      { key: 'command', type: 'text', placeholder: 'Command' },
      { key: 'relation', type: 'select', options: ['Before', 'After'] },
      { key: 'offsetSeconds', type: 'number', step: 0.1, min: 0, width: '70px' },
    ],
    onChange: notifySettingsChange,
  });

  initClearPreviewsModal();
  initStandaloneLaunchButton();

  const addInitCommandBtn = document.querySelector('#add-init-command-btn');
  if (addInitCommandBtn) {
    addInitCommandBtn.addEventListener('click', () => {
      initCommandsEditor.addItem('');
    });
  }

  const addCustomCommandBtn = document.querySelector('#add-custom-command-btn');
  if (addCustomCommandBtn) {
    addCustomCommandBtn.addEventListener('click', () => {
      customCommandsEditor.addItem({ command: '', relation: 'Before', offsetSeconds: 2.0 });
    });
  }

  // Any config field that feeds computeRequiredCaptureBytes recomputes the
  // hard launch guard on change, so the Start button's disabled state stays
  // live instead of only being checked at click time. These (plus the rest
  // of the Timing Options tab, which doesn't feed the guard) also persist to
  // settings.json on edit — previously nothing wired them to a save at all,
  // so values only survived a restart by coincidence, if some unrelated
  // action (e.g. browsing for hlae.exe) happened to save afterward.
  ['#config-res-width', '#config-res-height', '#config-separate-hud',
   '#config-pre-roll', '#config-post-roll', '#config-capture-fps'].forEach(selector => {
    const el = document.querySelector(selector);
    if (el) el.addEventListener('input', () => { refreshLaunchGuard(); notifySettingsChange(); });
  });
  ['#config-record-start-lead', '#config-record-stop-trail', '#config-initial-delay']
    .forEach(selector => {
      const el = document.querySelector(selector);
      if (el) el.addEventListener('input', () => notifySettingsChange());
    });
  // Checkboxes read by persistAppSettings/buildCapturePayload but with no
  // change listener of their own — same missing-wiring bug as the Timing
  // Options fields above, just on Path Routing / Capture Output checkboxes.
  ['#config-add-condebug', '#config-auto-clear-logs', '#config-auto-clear-previews',
   '#config-auto-clear-temp-demos', '#config-save-local-patched'].forEach(selector => {
    const el = document.querySelector(selector);
    if (el) el.addEventListener('change', () => notifySettingsChange());
  });
  // Only read at capture time (buildCapturePayload), same as the checkboxes
  // above, but previously not part of AppSettings at all — reset to default
  // every restart.
  refreshLaunchGuard();

  if (!unlistenCaptureStatus) {
    listen('capture_status', (event) => {
      const payload = event.payload || {};
      if (payload.running) {
        capturingInFlight = true;
        if (progressContainer) progressContainer.style.display = 'block';
        if (progressBar) {
          if (payload.index !== undefined && payload.total && payload.total > 0) {
            const pct = Math.min(100, Math.round((payload.index / payload.total) * 100));
            progressBar.style.width = `${pct}%`;
          } else {
            progressBar.style.width = '50%';
          }
        }
        const statusText = payload.name ? `${payload.status || "Capturing"}: ${payload.name}` : (payload.status || "Capturing...");
        if (statusEl) statusEl.textContent = statusText;
        updateRowBadges(payload.status || "Capturing...", "#ff9800");
        if (startBtn) startBtn.disabled = true;
        if (cancelBtn) cancelBtn.disabled = false;
      } else {
        capturingInFlight = false;
        if (cancelBtn) cancelBtn.disabled = true;
        refreshLaunchGuard();

        if (payload.error) {
          showToast(`Capture error: ${payload.status || "Unknown error"}`, "error");
          updateRowBadges("Error", "#f44336");
          if (statusEl) statusEl.textContent = `Error: ${payload.status || "Capture failed"}`;
        } else if (payload.status === "Cancelled") {
          showToast("Batch capture cancelled.", "info");
          updateRowBadges("Cancelled", "#f44336");
          if (progressBar) progressBar.style.width = '0%';
          if (statusEl) statusEl.textContent = "Cancelled";
        } else {
          showToast("Batch capture completed successfully!", "success");
          updateRowBadges("Completed", "#4caf50");
          if (progressBar) progressBar.style.width = '100%';
          if (statusEl) statusEl.textContent = "Completed";
        }
      }
    }).then(unlistenFn => {
      unlistenCaptureStatus = unlistenFn;
    }).catch(err => {
      console.error("Failed to register capture_status listener:", err);
    });
  }

  // Post-batch take verification. Observe-only for now: it reports what landed
  // on disk but does not yet touch any highlight's status (Phase 2).
  if (!unlistenTakesVerified) {
    listen('capture_takes_verified', (event) => {
      const payload = event.payload || {};
      const blocks = payload.blocks || [];
      const total = payload.total_count ?? blocks.length;
      const captured = payload.captured_count ?? 0;
      const renderable = payload.renderable_count ?? 0;

      const resolved = blocks.map(block => ({
        ...block,
        streaks: (block.source_streak_indices || [])
          .map(i => lastDispatch?.streaks?.[i])
          .filter(Boolean),
      }));
      console.info('[take-verify]', payload.session_id, resolved);

      if (total === 0) return;
      if (captured < total) {
        showToast(`${captured}/${total} takes found on disk — ${total - captured} missing.`, 'error');
      } else if (renderable < total) {
        showToast(`${captured}/${total} takes captured, but ${total - renderable} won't be seen by Render Studio.`, 'info');
      } else {
        showToast(`All ${total} takes verified on disk.`, 'success');
      }
    }).then(unlistenFn => {
      unlistenTakesVerified = unlistenFn;
    }).catch(err => {
      console.error("Failed to register capture_takes_verified listener:", err);
    });
  }

  /**
   * Reads every Batch Capture Config field from the DOM plus the given
   * cross-pane `state` (scanned demos, target drives) and assembles the
   * `start_capture_batch` IPC payload. Returns `null` (after toasting the
   * reason) if a required field is missing — callers must check for that
   * before proceeding.
   */
  function buildCapturePayload(state) {
    const selectedStreaks = [];
    if (state.currentScannedDemos) {
      state.currentScannedDemos.forEach(demo => {
        if (demo.streaks) {
          demo.streaks.forEach(streak => {
            // Opt-in model (detail_pane.js) — see computeRequiredCaptureBytes above.
            if (streak.selected === true) {
              selectedStreaks.push(streak);
            }
          });
        }
      });
    }

    // Remembered so the post-batch take verification can map each block's
    // source_streak_indices back to these exact live streak objects — the
    // backend preserves this array's order, so index N here is index N there.
    const sessionId = generateSessionId();
    lastDispatch = { sessionId, streaks: selectedStreaks };

    const captureFpsVal = parseInt(document.querySelector("#config-capture-fps")?.value, 10) || 300;
    const preRollVal = parseFloat(document.querySelector("#config-pre-roll")?.value) || 2.0;
    const postRollVal = parseFloat(document.querySelector("#config-post-roll")?.value) || 0.6;
    const recordStartLeadVal = parseFloat(document.querySelector("#config-record-start-lead")?.value) || 0.0;
    const recordStopTrailVal = parseFloat(document.querySelector("#config-record-stop-trail")?.value) || 0.0;
    const initialDelayVal = parseFloat(document.querySelector("#config-initial-delay")?.value) || 3.0;
    const fastForwardSpeedVal = parseFloat(document.querySelector("#config-fast-forward-speed")?.value) || 0.05;

    const hlaePathVal = document.querySelector("#hlae-path-input")?.value?.trim() || "";
    const hlPathVal = document.querySelector("#hl-path-input")?.value?.trim() || "";
    const ffmpegOverridePathVal = document.querySelector("#ffmpeg-override-path-input")?.value?.trim() || null;

    const resWidthVal = parseInt(document.querySelector("#config-res-width")?.value, 10) || 1280;
    const resHeightVal = parseInt(document.querySelector("#config-res-height")?.value, 10) || 720;
    const separateHudVal = document.querySelector("#config-separate-hud")?.checked || false;
    const saveLocalPatchedCopyVal = document.querySelector("#config-save-local-patched")?.checked || false;
    const addCondebugVal = document.querySelector("#config-add-condebug")?.checked || false;

    const autoClearLogsVal = document.querySelector("#config-auto-clear-logs")?.checked || false;
    const autoClearPreviewsVal = document.querySelector("#config-auto-clear-previews")?.checked || false;
    const autoClearTempDemosVal = document.querySelector("#config-auto-clear-temp-demos")?.checked || false;

    if (!hlaePathVal || !hlPathVal) {
      const errorMsg = "Please specify valid file paths for both HLAE Executable (hlae.exe) and Half-Life Executable (hl.exe).";
      showToast(errorMsg, 'error');
      return null;
    }

    // capture_directories is the physical BMP/patched-demo output pool —
    // native/src/patch/builder.rs routes capture output there and
    // capture_engine.rs mklinks a junction per entry. It must be actual
    // output directories, NEVER state.scanPaths (the demo *source* files
    // the user added for scanning); mklinking a junction against a .dem
    // file aborts the batch. Sourced from Capture Output — required, no
    // fallback.
    const outputDrivePool = (state.targetDrives || []).filter(Boolean);

    if (outputDrivePool.length === 0) {
      showToast("Configure at least one Capture Output directory before starting a capture.", 'error');
      return null;
    }

    // Blank rows (added via "+ Add" but never filled in) are dropped rather
    // than sent as no-op console commands.
    const initCommandsPayload = initCommands.map(c => c.trim()).filter(c => c.length > 0);
    const customCommandsPayload = customCommands
      .filter(c => c.command && c.command.trim().length > 0)
      .map(c => ({
        command: c.command.trim(),
        relation: c.relation === 'After' ? 'After' : 'Before',
        offset_seconds: c.offsetSeconds,
      }));

    return {
      hlae_path: hlaePathVal,
      game_path: hlPathVal,
      ffmpeg_override_path: ffmpegOverridePathVal,
      resolution_width: resWidthVal,
      resolution_height: resHeightVal,
      separate_hud: separateHudVal,
      save_local_patched_copy: saveLocalPatchedCopyVal,
      add_condebug: addCondebugVal,
      streaks: selectedStreaks,
      pre_roll_seconds: preRollVal,
      post_roll_seconds: postRollVal,
      capture_directories: outputDrivePool,
      capture_fps: captureFpsVal,
      drives: state.targetDrives || [],
      record_start_lead: recordStartLeadVal,
      record_stop_trail: recordStopTrailVal,
      initial_delay: initialDelayVal,
      fast_forward_speed: fastForwardSpeedVal,
      auto_clear_logs: autoClearLogsVal,
      auto_clear_previews: autoClearPreviewsVal,
      auto_clear_temp_demos: autoClearTempDemosVal,
      session_id: sessionId,
      init_commands: initCommandsPayload,
      custom_commands: customCommandsPayload,
    };
  }

  if (startBtn) {
    startBtn.addEventListener('click', async () => {
      const state = getState ? getState() : { scanPaths: [], targetDrives: [], currentScannedDemos: [] };

      // Hard safety gate — recomputed fresh on every click regardless of the
      // button's current disabled state, so a stale/unrefreshed guard can
      // never let a capture start without sufficient disk space (this is
      // also what closes the old zero-drive bypass: `availableBytes === 0`
      // now blocks unconditionally instead of skipping the check).
      const guard = await refreshLaunchGuard(state);
      if (guard && guard.blocked) {
        if (guard.availableBytes === 0) {
          showToast("Configure at least one Capture Output directory with free space before starting a capture.", 'error');
        } else {
          showToast(`Insufficient disk space. Required: ${(guard.requiredBytes / 1e9).toFixed(2)} GB, Available: ${(guard.availableBytes / 1e9).toFixed(2)} GB`, 'error');
        }
        return;
      }

      const activePayload = buildCapturePayload(state);
      if (!activePayload) return; // buildCapturePayload already toasted the reason

      try {
        await validatePaths(activePayload.hlae_path, activePayload.game_path);
      } catch (err) {
        console.error("Executable path validation failed:", err);
        showToast(String(err), 'error');
        return;
      }

      showToast("Initializing capture batch...", "info");
      startBtn.disabled = true;
      if (cancelBtn) cancelBtn.disabled = false;
      updateRowBadges("Queued", "#2196f3");

      startCaptureBatch(activePayload)
        .then(() => {
          showToast("Batch capture queued successfully!", "success");
          if (progressContainer) progressContainer.style.display = 'block';
          if (progressBar) progressBar.style.width = '10%';
        })
        .catch((err) => {
          console.error("IPC Execution Error (start_capture_batch):", err);
          showToast("Error starting batch: " + err, "error");
          updateRowBadges("Failed", "#f44336");
          if (cancelBtn) cancelBtn.disabled = true;
          capturingInFlight = false;
          refreshLaunchGuard(state);
        });
    });
  }

  if (cancelBtn) {
    cancelBtn.addEventListener('click', () => {
      showToast("Cancelling batch...", "info");
      cancelBtn.disabled = true;
      cancelCaptureBatch()
        .then(() => {
          updateRowBadges("Cancelled", "#f44336");
          if (progressBar) progressBar.style.width = '0%';
          capturingInFlight = false;
          refreshLaunchGuard();
        })
        .catch((err) => {
          console.error("IPC Execution Error (cancel_capture_batch):", err);
          capturingInFlight = false;
          refreshLaunchGuard();
        });
    });
  }
}
