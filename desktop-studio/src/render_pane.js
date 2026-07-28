import { listen } from '@tauri-apps/api/event';
import {
  scanRenderDirectories,
  executeRenderBatch,
  cancelRenderBatch,
  onRenderStatus
} from './ipc_bridge.js';
import { showToast } from './toast.js';

export function initRenderProgressListener(renderStatusEl, progressBar, progressContainer, startRenderBtn, cancelRenderBtn) {
  return listen('render_status', (event) => {
    const payload = event.payload;
    if (progressContainer) progressContainer.style.display = 'block';

    const statusText = typeof payload === 'object' && payload.status ? payload.status : payload;
    if (renderStatusEl) {
      renderStatusEl.textContent = typeof statusText === 'string' ? `Status: ${statusText}` : `Status: ${JSON.stringify(statusText)}`;
    }

    if (progressBar && typeof payload === 'object' && payload.progress !== undefined) {
      progressBar.style.width = `${payload.progress}%`;
    }

    if (statusText === 'Finished') {
      showToast("Render batch completed successfully!", "success");
      if (startRenderBtn) startRenderBtn.disabled = false;
      if (cancelRenderBtn) cancelRenderBtn.disabled = true;
    } else if (statusText === 'Cancelled') {
      showToast("Render batch cancelled.", "info");
      if (startRenderBtn) startRenderBtn.disabled = false;
      if (cancelRenderBtn) cancelRenderBtn.disabled = true;
    } else if (typeof statusText === 'string' && statusText.startsWith('FFmpeg spawn error')) {
      showToast(statusText, "error");
      if (startRenderBtn) startRenderBtn.disabled = false;
      if (cancelRenderBtn) cancelRenderBtn.disabled = true;
    }
  });
}

export function initRenderUI(getRenderFolders) {
  const scanRenderBtn = document.querySelector('#scan-render-btn');
  const startRenderBtn = document.querySelector('#start-render-btn');
  const cancelRenderBtn = document.querySelector('#cancel-render-btn');
  const renderStatusEl = document.querySelector('#render-status');
  const progressContainer = document.querySelector('#render-progress-container');
  const progressBar = document.querySelector('#render-progress-bar');

  // Register real-time progress listener via Tauri events
  initRenderProgressListener(renderStatusEl, progressBar, progressContainer, startRenderBtn, cancelRenderBtn);

  if (scanRenderBtn) {
    scanRenderBtn.addEventListener('click', () => {
      const renderFolders = getRenderFolders ? getRenderFolders() : [];
      if (!renderFolders || renderFolders.length === 0) {
        showToast("Please add at least one render directory.", "error");
        return;
      }
      if (renderStatusEl) renderStatusEl.textContent = "Status: Scanning render directories...";
      scanRenderDirectories(renderFolders)
        .then((takes) => {
          const count = takes ? takes.length : 0;
          if (renderStatusEl) renderStatusEl.textContent = `Status: Scanned ${count} render take(s).`;
          showToast(`Scanned ${count} render take(s).`, "info");
          const container = document.querySelector('#render-job-container');
          if (container) {
            container.innerHTML = takes && takes.length > 0
              ? takes.map(t => `<div style="padding: 6px; border-bottom: 1px solid #444; font-family: monospace;">Take: ${t.take_name || t.path || JSON.stringify(t)}</div>`).join('')
              : '<p style="color: #888;">No render takes detected.</p>';
          }
        })
        .catch((err) => {
          console.error("IPC Execution Error (scan_render_directories):", err);
          if (renderStatusEl) renderStatusEl.textContent = "IPC Execution Error: " + err;
          showToast("Error scanning render directories: " + err, "error");
        });
    });
  }

  if (startRenderBtn) {
    startRenderBtn.addEventListener('click', () => {
      const renderFolders = getRenderFolders ? getRenderFolders() : [];
      if (renderStatusEl) renderStatusEl.textContent = "Status: Initializing render batch...";
      showToast("Initializing render batch...", "info");
      startRenderBtn.disabled = true;
      if (cancelRenderBtn) cancelRenderBtn.disabled = false;
      if (progressContainer) progressContainer.style.display = 'block';
      if (progressBar) progressBar.style.width = '5%';

      const renderPayload = {
        render_directories: renderFolders,
        output_format: "mp4",
        crf: 18,
        preset: "medium"
      };

      executeRenderBatch(renderPayload)
        .then(() => {
          if (renderStatusEl) renderStatusEl.textContent = "Status: Render batch queued successfully!";
          showToast("Render batch queued successfully!", "success");
        })
        .catch((err) => {
          console.error("IPC Execution Error (executeRenderBatch):", err);
          if (renderStatusEl) renderStatusEl.textContent = "IPC Execution Error: " + err;
          showToast("Error executing render batch: " + err, "error");
          if (startRenderBtn) startRenderBtn.disabled = false;
          if (cancelRenderBtn) cancelRenderBtn.disabled = true;
        });
    });
  }

  if (cancelRenderBtn) {
    cancelRenderBtn.addEventListener('click', () => {
      if (renderStatusEl) renderStatusEl.textContent = "Status: Cancelling render batch...";
      cancelRenderBtn.disabled = true;
      cancelRenderBatch()
        .catch((err) => {
          console.error("IPC Execution Error (cancelRenderBatch):", err);
          if (startRenderBtn) startRenderBtn.disabled = false;
        });
    });
  }
}
