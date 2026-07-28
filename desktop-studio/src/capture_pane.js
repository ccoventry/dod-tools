import { startCaptureBatch, cancelCaptureBatch, getCaptureStatus, validatePaths } from './ipc_bridge.js';
import { showToast } from './toast.js';

let statusInterval = null;

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

function stopStatusPolling() {
  if (statusInterval) {
    clearInterval(statusInterval);
    statusInterval = null;
  }
}

export function initCaptureUI(getState) {
  const startBtn = document.querySelector('#start-capture-btn') || document.querySelector('#start-batch-btn');
  const cancelBtn = document.querySelector('#cancel-batch-btn');
  const statusEl = document.querySelector('#batch-status');
  const progressContainer = document.querySelector('#capture-progress-container');
  const progressBar = document.querySelector('#capture-progress-bar');

  function startStatusPolling() {
    stopStatusPolling();

    if (progressContainer) progressContainer.style.display = 'block';
    if (progressBar) progressBar.style.width = '25%';

    statusInterval = setInterval(async () => {
      try {
        const isRunning = await getCaptureStatus();
        if (isRunning) {
          statusEl.textContent = "Status: Capturing batch in progress...";
          updateRowBadges("Capturing...", "#ff9800");
          if (progressBar) progressBar.style.width = '65%';
        } else {
          stopStatusPolling();
          statusEl.textContent = "Status: Batch completed";
          showToast("Batch capture completed successfully!", "success");
          updateRowBadges("Completed", "#4caf50");
          if (progressBar) progressBar.style.width = '100%';
          if (startBtn) startBtn.disabled = false;
          if (cancelBtn) cancelBtn.disabled = true;
        }
      } catch (err) {
        console.error("IPC Error polling capture status:", err);
        stopStatusPolling();
        statusEl.textContent = "Error checking capture status: " + err;
        showToast("Error checking capture status: " + err, "error");
        updateRowBadges("Error", "#f44336");
        if (startBtn) startBtn.disabled = false;
        if (cancelBtn) cancelBtn.disabled = true;
      }
    }, 500);
  }

  if (startBtn) {
    startBtn.addEventListener('click', async () => {
      const state = getState ? getState() : { scanPaths: [], targetDrives: [], currentScannedDemos: [] };
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
        if (statusEl) statusEl.textContent = "Error: " + errorMsg;
        return;
      }

      try {
        await validatePaths(hlaePathVal, hlPathVal);
      } catch (err) {
        console.error("Executable path validation failed:", err);
        showToast(String(err), 'error');
        if (statusEl) statusEl.textContent = "Error: " + err;
        return;
      }

      const activePayload = {
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
        capture_directories: state.scanPaths || [],
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
        auto_clear_temp_demos: autoClearTempDemosVal
      };

      statusEl.textContent = "Status: Initializing capture batch...";
      showToast("Initializing capture batch...", "info");
      startBtn.disabled = true;
      if (cancelBtn) cancelBtn.disabled = false;
      updateRowBadges("Queued", "#2196f3");

      startCaptureBatch(activePayload)
        .then(() => {
          statusEl.textContent = "Status: Batch queued successfully!";
          showToast("Batch capture queued successfully!", "success");
          startStatusPolling();
        })
        .catch((err) => {
          console.error("IPC Execution Error (start_capture_batch):", err);
          stopStatusPolling();
          statusEl.textContent = "Error starting batch: " + err;
          showToast("Error starting batch: " + err, "error");
          updateRowBadges("Failed", "#f44336");
          if (startBtn) startBtn.disabled = false;
          if (cancelBtn) cancelBtn.disabled = true;
        });
    });
  }

  if (cancelBtn) {
    cancelBtn.addEventListener('click', () => {
      statusEl.textContent = "Status: Cancelling batch...";
      cancelBtn.disabled = true;
      cancelCaptureBatch()
        .then(() => {
          stopStatusPolling();
          updateRowBadges("Cancelled", "#f44336");
          if (startBtn) startBtn.disabled = false;
          if (progressBar) progressBar.style.width = '0%';
        })
        .catch((err) => {
          console.error("IPC Execution Error (cancel_capture_batch):", err);
          stopStatusPolling();
          if (startBtn) startBtn.disabled = false;
        });
    });
  }
}
