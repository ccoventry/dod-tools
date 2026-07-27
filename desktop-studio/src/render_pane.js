import {
  scanRenderDirectories,
  executeRenderBatch,
  getRenderStatus,
  cancelRenderBatch,
  initRenderProgressListener
} from './ipc_bridge.js';

let renderStatusInterval = null;

function stopRenderPolling() {
  if (renderStatusInterval) {
    clearInterval(renderStatusInterval);
    renderStatusInterval = null;
  }
}

export function initRenderUI(getRenderFolders) {
  const scanRenderBtn = document.querySelector('#scan-render-btn');
  const startRenderBtn = document.querySelector('#start-render-btn');
  const cancelRenderBtn = document.querySelector('#cancel-render-btn');
  const renderStatusEl = document.querySelector('#render-status');
  const progressContainer = document.querySelector('#render-progress-container');
  const progressBar = document.querySelector('#render-progress-bar');

  // Register real-time progress listener
  initRenderProgressListener((payload) => {
    if (renderStatusEl) {
      renderStatusEl.textContent = typeof payload === 'string' ? `Status: ${payload}` : `Status: ${JSON.stringify(payload)}`;
    }
    if (progressContainer) progressContainer.style.display = 'block';
    if (progressBar && typeof payload === 'object' && payload.progress !== undefined) {
      progressBar.style.width = `${payload.progress}%`;
    }
  });

  if (scanRenderBtn) {
    scanRenderBtn.addEventListener('click', () => {
      const renderFolders = getRenderFolders ? getRenderFolders() : [];
      if (!renderFolders || renderFolders.length === 0) {
        console.warn("Please add at least one render directory.");
        return;
      }
      if (renderStatusEl) renderStatusEl.textContent = "Status: Scanning render directories...";
      scanRenderDirectories(renderFolders)
        .then((takes) => {
          if (renderStatusEl) renderStatusEl.textContent = `Status: Scanned ${takes ? takes.length : 0} render take(s).`;
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
        });
    });
  }

  if (startRenderBtn) {
    startRenderBtn.addEventListener('click', () => {
      const renderFolders = getRenderFolders ? getRenderFolders() : [];
      if (renderStatusEl) renderStatusEl.textContent = "Status: Initializing render batch...";
      startRenderBtn.disabled = true;
      if (cancelRenderBtn) cancelRenderBtn.disabled = false;
      if (progressContainer) progressContainer.style.display = 'block';
      if (progressBar) progressBar.style.width = '10%';

      const renderPayload = {
        render_directories: renderFolders,
        output_format: "mp4",
        crf: 18,
        preset: "medium"
      };

      executeRenderBatch(renderPayload)
        .then(() => {
          if (renderStatusEl) renderStatusEl.textContent = "Status: Render batch queued successfully!";
          stopRenderPolling();
          renderStatusInterval = setInterval(async () => {
            try {
              const statusText = await getRenderStatus();
              if (statusText.startsWith("Rendering") || statusText.startsWith("Scanning")) {
                if (renderStatusEl) renderStatusEl.textContent = `Status: ${statusText}`;
                if (progressBar) progressBar.style.width = '50%';
              } else {
                stopRenderPolling();
                if (renderStatusEl) renderStatusEl.textContent = `Status: ${statusText}`;
                if (progressBar) progressBar.style.width = '100%';
                if (startRenderBtn) startRenderBtn.disabled = false;
                if (cancelRenderBtn) cancelRenderBtn.disabled = true;
              }
            } catch (err) {
              console.error("IPC Execution Error (getRenderStatus):", err);
              stopRenderPolling();
              if (renderStatusEl) renderStatusEl.textContent = "IPC Execution Error: " + err;
              if (startRenderBtn) startRenderBtn.disabled = false;
              if (cancelRenderBtn) cancelRenderBtn.disabled = true;
            }
          }, 500);
        })
        .catch((err) => {
          console.error("IPC Execution Error (executeRenderBatch):", err);
          stopRenderPolling();
          if (renderStatusEl) renderStatusEl.textContent = "IPC Execution Error: " + err;
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
        .then(() => {
          stopRenderPolling();
          if (startRenderBtn) startRenderBtn.disabled = false;
          if (progressBar) progressBar.style.width = '0%';
        })
        .catch((err) => {
          console.error("IPC Execution Error (cancelRenderBatch):", err);
          stopRenderPolling();
          if (startRenderBtn) startRenderBtn.disabled = false;
        });
    });
  }
}
