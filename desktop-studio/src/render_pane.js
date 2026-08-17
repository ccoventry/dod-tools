import { listen } from '@tauri-apps/api/event';
import {
  scanRenderDirectories,
  executeRenderBatch,
  cancelRenderBatch,
  cancelRenderJob,
  resetRenderJob,
  getExportPoolFreeGb,
  checkRenderAutosave,
  discardRenderAutosave,
  recoverRenderBatch,
} from './ipc_bridge.js';
import { showToast } from './toast.js';

let jobs = []; // RenderJobView[] — latest snapshot from 'render_jobs_snapshot'

function esc(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function statusColor(status) {
  switch (status) {
    case 'Finished': return '#4caf50';
    case 'Error': return '#f44336';
    case 'Cancelled': return '#ffeb3b';
    case 'Rendering': return '#7ec8e3';
    default: return '#ffffff';
  }
}

function renderJobsTable() {
  const tbody = document.querySelector('#render-jobs-tbody');
  if (!tbody) return;

  if (jobs.length === 0) {
    tbody.innerHTML = '<tr><td colspan="8" class="table-empty">No render jobs queued. Scan a folder, then click Start Render Batch.</td></tr>';
    return;
  }

  tbody.innerHTML = jobs.map((j) => `
    <tr data-job-id="${esc(j.id)}">
      <td>${esc(j.name)}</td>
      <td>${esc(j.stream)}</td>
      <td>${j.frames}</td>
      <td>${esc(j.date)}</td>
      <td style="color:${statusColor(j.status)};">${esc(j.status)}</td>
      <td>${esc(j.speed)}</td>
      <td>
        <div class="progress-bar-container" style="margin-top:0;">
          <div class="progress-bar-fill" style="width:${j.progress}%;"></div>
        </div>
      </td>
      <td>
        ${(j.status === 'Rendering' || j.status === 'Queued')
          ? `<button class="render-job-cancel-btn" data-job-id="${esc(j.id)}" title="Cancel this job">✖</button>`
          : (j.status === 'Cancelled' || j.status === 'Finished' || j.status === 'Error')
            ? `<button class="render-job-reset-btn" data-job-id="${esc(j.id)}" title="Reset to Queued">🔄</button>`
            : ''}
        ${j.error_log ? `<button class="render-job-view-log-btn" data-job-id="${esc(j.id)}" title="View error log">⚠️ View Log</button>` : ''}
      </td>
    </tr>`).join('');

  tbody.querySelectorAll('.render-job-cancel-btn').forEach((btn) => {
    btn.addEventListener('click', () => cancelRenderJob(btn.dataset.jobId).catch(() => {}));
  });
  tbody.querySelectorAll('.render-job-reset-btn').forEach((btn) => {
    btn.addEventListener('click', () => resetRenderJob(btn.dataset.jobId).catch(() => {}));
  });
  tbody.querySelectorAll('.render-job-view-log-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const job = jobs.find((j) => j.id === btn.dataset.jobId);
      const modal = document.querySelector('#render-error-log-modal');
      const title = document.querySelector('#render-error-log-title');
      const body = document.querySelector('#render-error-log-body');
      if (title) title.textContent = job ? `FFmpeg Error Log — ${job.name}` : 'FFmpeg Error Log';
      if (body) body.textContent = (job && job.error_log) || '';
      if (modal) modal.style.display = 'flex';
    });
  });
}

function initErrorLogModal() {
  const modal = document.querySelector('#render-error-log-modal');
  const closeBtn = document.querySelector('#render-error-log-close-btn');
  closeBtn?.addEventListener('click', () => { if (modal) modal.style.display = 'none'; });
}

// ── JIT multi-drive export pool ──────────────────────────────────────────────

async function refreshExportPoolFree(getExportDirs) {
  const dirs = getExportDirs ? getExportDirs() : [];
  const freeEl = document.querySelector('#render-export-pool-free');
  if (!freeEl) return;
  if (dirs.length === 0) {
    freeEl.textContent = '0.0 GB';
    return;
  }
  const gb = await getExportPoolFreeGb(dirs);
  freeEl.textContent = `${gb.toFixed(1)} GB`;
}

// ── Startup crash-recovery prompt ────────────────────────────────────────────

export async function checkRenderRecoveryOnStartup(onRecovered) {
  const summary = await checkRenderAutosave();
  if (!summary) return;

  const modal = document.querySelector('#render-recovery-modal');
  if (!modal) return;
  document.querySelector('#render-recovery-source').textContent = summary.source_folder || '(unknown)';
  document.querySelector('#render-recovery-completed').textContent = String(summary.completed_count);
  document.querySelector('#render-recovery-pending').textContent = String(summary.pending_count);
  modal.style.display = 'flex';

  document.querySelector('#render-recovery-recover-btn').addEventListener('click', async () => {
    try {
      jobs = await recoverRenderBatch();
      renderJobsTable();
      showToast(`Recovered ${summary.completed_count} completed, ${summary.pending_count} pending render job(s).`, 'info');
      if (onRecovered) onRecovered();
    } catch (err) {
      console.error('Error recovering render batch:', err);
      showToast('Failed to recover render batch: ' + err, 'error');
    } finally {
      modal.style.display = 'none';
    }
  }, { once: true });

  document.querySelector('#render-recovery-discard-btn').addEventListener('click', async () => {
    try {
      await discardRenderAutosave();
    } catch (err) {
      console.error('Error discarding render autosave:', err);
    } finally {
      modal.style.display = 'none';
    }
  }, { once: true });
}

export function initRenderUI(getRenderFolders, getExportDirs) {
  const scanRenderBtn = document.querySelector('#scan-render-btn');
  const startRenderBtn = document.querySelector('#start-render-btn');
  const cancelRenderBtn = document.querySelector('#cancel-render-btn');
  const renderStatusEl = document.querySelector('#render-status');

  initErrorLogModal();

  // Real-time per-job state, pushed by the backend scheduler.
  listen('render_jobs_snapshot', (event) => {
    jobs = event.payload || [];
    renderJobsTable();
    const activeOrQueued = jobs.filter((j) => j.status === 'Rendering' || j.status === 'Queued').length;
    if (renderStatusEl) {
      renderStatusEl.textContent = activeOrQueued > 0
        ? `Status: Rendering (${jobs.length - activeOrQueued}/${jobs.length} done)`
        : 'Status: Waiting...';
    }
  });

  listen('render_batch_finished', (event) => {
    const status = event.payload && event.payload.status;
    if (startRenderBtn) startRenderBtn.disabled = false;
    if (cancelRenderBtn) cancelRenderBtn.disabled = true;
    if (status === 'No takes found to render') {
      showToast(status, 'error');
      if (renderStatusEl) renderStatusEl.textContent = 'Status: ' + status;
      return;
    }
    const errored = jobs.some((j) => j.status === 'Error');
    const cancelled = jobs.some((j) => j.status === 'Cancelled');
    if (errored) {
      showToast('Render batch finished with errors — check job rows for details.', 'error');
    } else if (cancelled) {
      showToast('Render batch cancelled.', 'info');
    } else if (jobs.length > 0) {
      showToast('Render batch completed successfully!', 'success');
    }
    if (renderStatusEl) renderStatusEl.textContent = 'Status: Finished';
  });

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

      const codecVal = document.querySelector('#render-codec-select')?.value || 'prores';
      const fpsVal = parseInt(document.querySelector('#render-fps-input')?.value, 10) || 300;
      const maxConcurrentVal = Math.min(8, Math.max(1, parseInt(document.querySelector('#render-max-concurrent-input')?.value, 10) || 2));
      const exportDirs = (getExportDirs ? getExportDirs() : []).filter(Boolean);
      // FFmpeg override is shared with the capture config panel.
      const ffmpegPathVal = document.querySelector('#ffmpeg-override-path-input')?.value?.trim() || null;

      showToast('Initializing render batch...', 'info');
      startRenderBtn.disabled = true;
      if (cancelRenderBtn) cancelRenderBtn.disabled = false;
      if (renderStatusEl) renderStatusEl.textContent = 'Status: Scanning for takes...';

      const renderPayload = {
        render_directories: renderFolders,
        codec: codecVal,
        fps: fpsVal,
        ffmpeg_path: ffmpegPathVal || null,
        export_directories: exportDirs,
        max_concurrent_renders: maxConcurrentVal,
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

  refreshExportPoolFree(getExportDirs);
  // Re-check free space periodically while the pane is open — cheap
  // filesystem calls, avoids the readout going stale during a long batch.
  setInterval(() => refreshExportPoolFree(getExportDirs), 15000);
}
