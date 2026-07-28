import { listen } from '@tauri-apps/api/event';
import {
  scanRenderDirectories,
  executeRenderBatch,
  cancelRenderBatch,
} from './ipc_bridge.js';
import { showToast } from './toast.js';

// Single Tauri event listener, registered once per initRenderUI() call.
// Returns the unlisten fn so callers can tear it down if needed.
export function initRenderProgressListener(
  renderStatusEl,
  progressBar,
  progressContainer,
  startRenderBtn,
  cancelRenderBtn,
) {
  return listen('render_status', (event) => {
    const payload = event.payload;
    if (progressContainer) progressContainer.style.display = 'block';

    const statusText =
      typeof payload === 'object' && payload.status ? payload.status : payload;

    if (
      progressBar &&
      typeof payload === 'object' &&
      payload.progress !== undefined
    ) {
      progressBar.style.width = `${payload.progress}%`;
    }

    // Terminal state handling — re-enable start, disable cancel.
    const isTerminal =
      statusText === 'Finished' ||
      statusText === 'Cancelled' ||
      (typeof statusText === 'string' &&
        (statusText.startsWith('FFmpeg spawn error') ||
          statusText.startsWith('FFmpeg failed') ||
          statusText.startsWith('FFmpeg not found') ||
          statusText === 'No takes found to render'));

    if (isTerminal) {
      if (statusText === 'Finished') {
        showToast('Render batch completed successfully!', 'success');
      } else if (statusText === 'Cancelled') {
        showToast('Render batch cancelled.', 'info');
      } else {
        showToast(statusText, 'error');
      }
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

  // Register a single real-time progress listener via Tauri events.
  // NOTE: onRenderStatus from ipc_bridge is intentionally NOT used here to
  // prevent double-registration (bug I1/I2 fix).
  initRenderProgressListener(
    renderStatusEl,
    progressBar,
    progressContainer,
    startRenderBtn,
    cancelRenderBtn,
  );

  if (scanRenderBtn) {
    scanRenderBtn.addEventListener('click', () => {
      const renderFolders = getRenderFolders ? getRenderFolders() : [];
      if (!renderFolders || renderFolders.length === 0) {
        showToast('Please add at least one render directory.', 'error');
        return;
      }
      showToast('Scanning render directories...', 'info');
      scanRenderDirectories(renderFolders)
        .then((takes) => {
          const count = takes ? takes.length : 0;
          showToast(`Scanned ${count} render take(s).`, 'info');

          const container = document.querySelector('#render-job-container');
          if (container) {
            container.innerHTML =
              takes && takes.length > 0
                ? takes
                    .map(
                      (t) =>
                        // base_name is the canonical display field on SerializedRenderJob
                        `<div style="padding: 6px; border-bottom: 1px solid #444; font-family: monospace;">` +
                        `<span style="color:#aaa;">[${t.clip_type}]</span> ` +
                        `${t.base_name} &mdash; ${t.frame_count} frames` +
                        `</div>`,
                    )
                    .join('')
                : '<p style="color: #888;">No render takes detected.</p>';
          }
        })
        .catch((err) => {
          console.error('IPC Execution Error (scan_render_directories):', err);
          showToast('Error scanning render directories: ' + err, 'error');
        });
    });
  }

  if (startRenderBtn) {
    startRenderBtn.addEventListener('click', () => {
      const renderFolders = getRenderFolders ? getRenderFolders() : [];
      if (!renderFolders || renderFolders.length === 0) {
        showToast('Please add at least one render directory.', 'error');
        return;
      }

      // Read codec / fps / export settings from the Render Studio controls.
      const codecVal =
        document.querySelector('#render-codec-select')?.value || 'prores';
      const fpsVal =
        parseInt(document.querySelector('#render-fps-input')?.value, 10) || 300;
      const exportDirVal =
        document.querySelector('#render-export-dir-input')?.value?.trim() ||
        null;
      // FFmpeg override is shared with the capture config panel.
      const ffmpegPathVal =
        document.querySelector('#ffmpeg-override-path-input')?.value?.trim() ||
        null;

      showToast('Initializing render batch...', 'info');
      startRenderBtn.disabled = true;
      if (cancelRenderBtn) cancelRenderBtn.disabled = false;
      if (progressContainer) progressContainer.style.display = 'block';
      if (progressBar) progressBar.style.width = '5%';

      // Payload aligned with the expanded RenderBatchPayload struct.
      const renderPayload = {
        render_directories: renderFolders,
        codec: codecVal,
        fps: fpsVal,
        ffmpeg_path: ffmpegPathVal || null,
        export_directory: exportDirVal || null,
      };

      executeRenderBatch(renderPayload)
        .then(() => {
          showToast('Render batch queued successfully!', 'success');
        })
        .catch((err) => {
          console.error('IPC Execution Error (executeRenderBatch):', err);
          showToast('Error executing render batch: ' + err, 'error');
          if (startRenderBtn) startRenderBtn.disabled = false;
          if (cancelRenderBtn) cancelRenderBtn.disabled = true;
        });
    });
  }

  if (cancelRenderBtn) {
    cancelRenderBtn.addEventListener('click', () => {
      showToast('Cancelling render batch...', 'info');
      cancelRenderBtn.disabled = true;
      cancelRenderBatch().catch((err) => {
        console.error('IPC Execution Error (cancelRenderBatch):', err);
        if (startRenderBtn) startRenderBtn.disabled = false;
      });
    });
  }
}
