import { startCaptureBatch, cancelCaptureBatch, validatePaths, simulateAotCapacity, calculateExportPoolSpace } from './ipc_bridge.js';
import { listen } from '@tauri-apps/api/event';
import { showToast } from './toast.js';

let unlistenCaptureStatus = null;

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

// ── Custom Engine Commands (Init / Before-After) ──────────────────────────────
// Local to the Batch Capture Config panel — nothing else in the app reads
// these, unlike scanPaths/targetDrives which main.js owns for cross-pane use.

let initCommands = [];
let customCommands = [];

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
  renderInitCommandsList();
  renderCustomCommandsList();
}

function escAttr(s) {
  return String(s ?? '').replace(/"/g, '&quot;');
}

function renderInitCommandsList() {
  const container = document.querySelector('#init-commands-list');
  if (!container) return;
  container.innerHTML = '';
  initCommands.forEach((cmd, idx) => {
    const row = document.createElement('div');
    row.className = 'input-row init-command-row';
    row.innerHTML = `
      <input type="text" class="init-command-input" placeholder="e.g. mirv_streams add all" style="flex: 1;" value="${escAttr(cmd)}" />
      <button type="button" class="remove-init-command-btn">Remove</button>
    `;
    row.querySelector('.init-command-input').addEventListener('input', (e) => {
      initCommands[idx] = e.target.value;
    });
    row.querySelector('.remove-init-command-btn').addEventListener('click', () => {
      initCommands.splice(idx, 1);
      renderInitCommandsList();
    });
    container.appendChild(row);
  });
}

function renderCustomCommandsList() {
  const container = document.querySelector('#custom-commands-list');
  if (!container) return;
  container.innerHTML = '';
  customCommands.forEach((cmd, idx) => {
    const row = document.createElement('div');
    row.className = 'input-row custom-command-row';
    row.innerHTML = `
      <input type="text" class="custom-command-input" placeholder="Command" style="flex: 1;" value="${escAttr(cmd.command)}" />
      <select class="custom-command-relation">
        <option value="Before" ${cmd.relation === 'Before' ? 'selected' : ''}>Before</option>
        <option value="After" ${cmd.relation === 'After' ? 'selected' : ''}>After</option>
      </select>
      <input type="number" class="custom-command-offset" step="0.1" min="0" style="width: 70px;" value="${cmd.offsetSeconds}" />
      <button type="button" class="remove-custom-command-btn">Remove</button>
    `;
    row.querySelector('.custom-command-input').addEventListener('input', (e) => {
      cmd.command = e.target.value;
    });
    row.querySelector('.custom-command-relation').addEventListener('change', (e) => {
      cmd.relation = e.target.value;
    });
    row.querySelector('.custom-command-offset').addEventListener('input', (e) => {
      const v = parseFloat(e.target.value);
      cmd.offsetSeconds = Number.isNaN(v) ? 0 : v;
    });
    row.querySelector('.remove-custom-command-btn').addEventListener('click', () => {
      customCommands.splice(idx, 1);
      renderCustomCommandsList();
    });
    container.appendChild(row);
  });
}

export function initCaptureUI(getState) {
  const startBtn = document.querySelector('#start-capture-btn') || document.querySelector('#start-batch-btn');
  const cancelBtn = document.querySelector('#cancel-batch-btn');
  const statusEl = document.querySelector('#batch-status');
  const progressContainer = document.querySelector('#capture-progress-container');
  const progressBar = document.querySelector('#capture-progress-bar');

  renderInitCommandsList();
  renderCustomCommandsList();

  const addInitCommandBtn = document.querySelector('#add-init-command-btn');
  if (addInitCommandBtn) {
    addInitCommandBtn.addEventListener('click', () => {
      initCommands.push('');
      renderInitCommandsList();
    });
  }

  const addCustomCommandBtn = document.querySelector('#add-custom-command-btn');
  if (addCustomCommandBtn) {
    addCustomCommandBtn.addEventListener('click', () => {
      customCommands.push({ command: '', relation: 'Before', offsetSeconds: 2.0 });
      renderCustomCommandsList();
    });
  }

  if (!unlistenCaptureStatus) {
    listen('capture_status', (event) => {
      const payload = event.payload || {};
      if (payload.running) {
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
        if (startBtn) startBtn.disabled = false;
        if (cancelBtn) cancelBtn.disabled = true;

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
            if (streak.selected === true || streak.selected === undefined) {
              selectedStreaks.push(streak);
            }
          });
        }
      });
    }

    const captureFpsVal = parseInt(document.querySelector("#config-capture-fps")?.value, 10) || 300;
    const expectedFpsVal = parseFloat(document.querySelector("#config-expected-fps")?.value) || 100.0;
    const preRollVal = parseFloat(document.querySelector("#config-pre-roll")?.value) || 2.0;
    const postRollVal = parseFloat(document.querySelector("#config-post-roll")?.value) || 0.6;
    const recordStartLeadVal = parseFloat(document.querySelector("#config-record-start-lead")?.value) || 0.0;
    const recordStopTrailVal = parseFloat(document.querySelector("#config-record-stop-trail")?.value) || 0.0;
    const initialDelayVal = parseFloat(document.querySelector("#config-initial-delay")?.value) || 3.0;
    const fastForwardSpeedVal = parseFloat(document.querySelector("#config-fast-forward-speed")?.value) || 0.05;
    const allocationStrategyVal = document.getElementById('allocation-strategy')?.value || "MaximizeSpace";

    const hlaePathVal = document.querySelector("#hlae-path-input")?.value?.trim() || "";
    const hlPathVal = document.querySelector("#hl-path-input")?.value?.trim() || "";
    const ffmpegOverridePathVal = document.querySelector("#ffmpeg-override-path-input")?.value?.trim() || null;
    const primaryMediaDirVal = document.querySelector("#primary-media-dir-input")?.value?.trim() || null;
    const backupMediaDirVal = document.querySelector("#backup-media-dir-input")?.value?.trim() || null;

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
    // file aborts the batch. Source from the configured Target Output
    // Drives pool, falling back to the Primary/Backup Media Dir fields
    // when that pool is empty.
    const driveDirs = (state.targetDrives || []).filter(Boolean);
    const outputDrivePool = driveDirs.length > 0
      ? driveDirs
      : [primaryMediaDirVal, backupMediaDirVal].filter(Boolean);

    if (outputDrivePool.length === 0) {
      showToast("Configure at least one Target Output Drive (or a Primary Media Directory) before starting a capture.", 'error');
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
      primary_media_dir: primaryMediaDirVal,
      backup_media_dir: backupMediaDirVal,
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
      expected_fps: expectedFpsVal,
      drives: state.targetDrives || [],
      allocation_strategy: allocationStrategyVal,
      record_start_lead: recordStartLeadVal,
      record_stop_trail: recordStopTrailVal,
      initial_delay: initialDelayVal,
      fast_forward_speed: fastForwardSpeedVal,
      auto_clear_logs: autoClearLogsVal,
      auto_clear_previews: autoClearPreviewsVal,
      auto_clear_temp_demos: autoClearTempDemosVal,
      session_id: generateSessionId(),
      init_commands: initCommandsPayload,
      custom_commands: customCommandsPayload,
    };
  }

  if (startBtn) {
    startBtn.addEventListener('click', async () => {
      const state = getState ? getState() : { scanPaths: [], targetDrives: [], currentScannedDemos: [] };
      const activePayload = buildCapturePayload(state);
      if (!activePayload) return; // buildCapturePayload already toasted the reason

      const preRollVal = activePayload.pre_roll_seconds;
      const postRollVal = activePayload.post_roll_seconds;
      const captureFpsVal = activePayload.capture_fps;
      const resWidthVal = activePayload.resolution_width;
      const resHeightVal = activePayload.resolution_height;
      const selectedStreaks = activePayload.streaks;

      try {
        await validatePaths(activePayload.hlae_path, activePayload.game_path);
      } catch (err) {
        console.error("Executable path validation failed:", err);
        showToast(String(err), 'error');
        return;
      }

      try {
        const availableBytes = await calculateExportPoolSpace(state.targetDrives || []);
        const streakDurations = selectedStreaks.map(s => {
          const baseDuration = (s.end_tick - s.start_tick) / (s.demo_fps || 100);
          return baseDuration + preRollVal + postRollVal;
        });
        const bytesPerFrame = resWidthVal * resHeightVal * 3;
        
        const [projectedBytes, hasEnoughSpace] = await simulateAotCapacity(
          streakDurations,
          captureFpsVal,
          bytesPerFrame,
          availableBytes
        );

        if (!hasEnoughSpace && availableBytes > 0) {
          showToast(`Insufficient disk space. Projected: ${(projectedBytes / 1e9).toFixed(2)} GB, Available: ${(availableBytes / 1e9).toFixed(2)} GB`, 'warning');
          return;
        }
      } catch (err) {
        console.error("AOT capacity simulation failed:", err);
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
          if (startBtn) startBtn.disabled = false;
          if (cancelBtn) cancelBtn.disabled = true;
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
          if (startBtn) startBtn.disabled = false;
          if (progressBar) progressBar.style.width = '0%';
        })
        .catch((err) => {
          console.error("IPC Execution Error (cancel_capture_batch):", err);
          if (startBtn) startBtn.disabled = false;
        });
    });
  }
}
