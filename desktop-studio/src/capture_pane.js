import { startCaptureBatch, cancelCaptureBatch, getCaptureStatus } from './ipc_bridge.js';

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
          updateRowBadges("Completed", "#4caf50");
          if (progressBar) progressBar.style.width = '100%';
          if (startBtn) startBtn.disabled = false;
          if (cancelBtn) cancelBtn.disabled = true;
        }
      } catch (err) {
        console.error("IPC Error polling capture status:", err);
        stopStatusPolling();
        statusEl.textContent = "Error checking capture status: " + err;
        updateRowBadges("Error", "#f44336");
        if (startBtn) startBtn.disabled = false;
        if (cancelBtn) cancelBtn.disabled = true;
      }
    }, 500);
  }

  if (startBtn) {
    startBtn.addEventListener('click', () => {
      const state = getState ? getState() : { scanPaths: [], targetDrives: [], currentScannedDemos: [] };
      const selectedStreaks = [];
      const checkboxes = document.querySelectorAll('#detail-streaks-container input[type="checkbox"]:checked');
      checkboxes.forEach(cb => {
        const dIdx = cb.dataset.demoIdx;
        const sIdx = cb.dataset.streakIdx;
        if (state.currentScannedDemos && state.currentScannedDemos[dIdx] && state.currentScannedDemos[dIdx].streaks[sIdx]) {
          selectedStreaks.push(state.currentScannedDemos[dIdx].streaks[sIdx]);
        }
      });

      const captureFpsVal = parseInt(document.querySelector("#config-capture-fps")?.value, 10) || 60;
      const expectedFpsVal = parseFloat(document.querySelector("#config-expected-fps")?.value) || 100.0;
      const preRollVal = parseFloat(document.querySelector("#config-pre-roll")?.value) || 3.0;
      const postRollVal = parseFloat(document.querySelector("#config-post-roll")?.value) || 2.0;
      const recordStartLeadVal = parseFloat(document.querySelector("#config-record-start-lead")?.value) || 0.0;
      const recordStopTrailVal = parseFloat(document.querySelector("#config-record-stop-trail")?.value) || 0.0;
      const initialDelayVal = parseFloat(document.querySelector("#config-initial-delay")?.value) || 3.0;
      const fastForwardSpeedVal = parseFloat(document.querySelector("#config-fast-forward-speed")?.value) || 10.0;
      const allocationStrategyVal = document.getElementById('allocation-strategy')?.value || "MaximizeSpace";

      const activePayload = {
        hlae_path: "C:\\dummy\\hlae.exe",
        game_path: "C:\\dummy\\hl.exe",
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
        fast_forward_speed: fastForwardSpeedVal
      };

      statusEl.textContent = "Status: Initializing capture batch...";
      startBtn.disabled = true;
      if (cancelBtn) cancelBtn.disabled = false;
      updateRowBadges("Queued", "#2196f3");

      startCaptureBatch(activePayload)
        .then(() => {
          statusEl.textContent = "Status: Batch queued successfully!";
          startStatusPolling();
        })
        .catch((err) => {
          console.error("IPC Execution Error (start_capture_batch):", err);
          stopStatusPolling();
          statusEl.textContent = "Error starting batch: " + err;
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
