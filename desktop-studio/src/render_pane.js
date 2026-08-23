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
  revealInExplorer,
} from './ipc_bridge.js';
import { showToast } from './toast.js';
import { streakUid, resolveTake } from './take_index.js';
import { STRINGS } from './strings.js';

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

function updateFooterQueueSummary() {
  const el = document.querySelector('#render-footer-queue-summary');
  if (!el) return;
  const queued = jobs.filter((j) => j.status === 'Queued').length;
  const rendering = jobs.filter((j) => j.status === 'Rendering').length;
  const done = jobs.filter((j) => j.status === 'Finished' || j.status === 'Error' || j.status === 'Cancelled').length;
  el.textContent = STRINGS.RENDER.queueSummary(queued, rendering, done);
}

function renderJobsTable() {
  updateFooterQueueSummary();
  const tbody = document.querySelector('#render-jobs-tbody');
  if (!tbody) return;

  if (jobs.length === 0) {
    tbody.innerHTML = `<tr><td colspan="9" class="table-empty">${STRINGS.RENDER.TABLE_EMPTY}</td></tr>`;
    return;
  }

  tbody.innerHTML = jobs.map((j) => `
    <tr data-job-id="${esc(j.id)}">
      <td>${esc(j.name)}</td>
      <td>${esc(j.stream)}</td>
      <td>${j.frames}</td>
      <td>${esc(j.date)}</td>
      <td>${esc(j.settings_summary)}</td>
      <td style="color:${statusColor(j.status)};">${esc(j.status)}</td>
      <td>${esc(j.speed)}</td>
      <td>
        <div class="progress-bar-container" style="margin-top:0;">
          <div class="progress-bar-fill" style="width:${j.progress}%;"></div>
        </div>
      </td>
      <td>
        ${(j.status === 'Rendering' || j.status === 'Queued')
          ? `<button class="render-job-cancel-btn" data-job-id="${esc(j.id)}" title="${STRINGS.RENDER.CANCEL_JOB_TITLE}">✖</button>`
          : (j.status === 'Cancelled' || j.status === 'Finished' || j.status === 'Error')
            ? `<button class="render-job-reset-btn" data-job-id="${esc(j.id)}" title="${STRINGS.RENDER.RESET_JOB_TITLE}">🔄</button>`
            : ''}
        ${j.error_log ? `<button class="render-job-view-log-btn" data-job-id="${esc(j.id)}" title="${STRINGS.RENDER.VIEW_LOG_TITLE}">${STRINGS.RENDER.VIEW_LOG_BUTTON}</button>` : ''}
        ${(j.status === 'Finished' && j.output_path) || j.take_folder
          ? `<button class="render-job-reveal-btn" data-job-id="${esc(j.id)}" title="${j.status === 'Finished' && j.output_path ? STRINGS.RENDER.OPEN_OUTPUT_FOLDER_TITLE : STRINGS.RENDER.OPEN_TAKE_FOLDER_TITLE}">${j.status === 'Finished' && j.output_path ? STRINGS.RENDER.OPEN_OUTPUT_BUTTON : STRINGS.RENDER.OPEN_TAKE_FOLDER_BUTTON}</button>`
          : ''}
      </td>
    </tr>`).join('');

  tbody.querySelectorAll('.render-job-cancel-btn').forEach((btn) => {
    btn.addEventListener('click', () => cancelRenderJob(btn.dataset.jobId).catch(() => {}));
  });
  tbody.querySelectorAll('.render-job-reset-btn').forEach((btn) => {
    btn.addEventListener('click', () => resetRenderJob(btn.dataset.jobId).catch(() => {}));
  });
  tbody.querySelectorAll('.render-job-reveal-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const job = jobs.find((j) => j.id === btn.dataset.jobId);
      if (!job) return;
      const target = (job.status === 'Finished' && job.output_path) ? job.output_path : job.take_folder;
      if (target) revealInExplorer(target).catch(() => {});
    });
  });
  tbody.querySelectorAll('.render-job-view-log-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const job = jobs.find((j) => j.id === btn.dataset.jobId);
      const modal = document.querySelector('#render-error-log-modal');
      const title = document.querySelector('#render-error-log-title');
      const body = document.querySelector('#render-error-log-body');
      if (title) title.textContent = job ? STRINGS.RENDER.errorLogTitleForJob(job.name) : STRINGS.RENDER.ERROR_LOG_TITLE_DEFAULT;
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
  const footerFreeEl = document.querySelector('#render-footer-pool-free');
  if (dirs.length === 0) {
    if (freeEl) freeEl.textContent = STRINGS.RENDER.EXPORT_POOL_FREE_DEFAULT;
    if (footerFreeEl) footerFreeEl.textContent = STRINGS.RENDER.RENDER_POOL_FREE_DEFAULT;
    return;
  }
  const gb = await getExportPoolFreeGb(dirs);
  if (freeEl) freeEl.textContent = STRINGS.RENDER.exportPoolFreeGb(gb.toFixed(1));
  if (footerFreeEl) footerFreeEl.textContent = STRINGS.RENDER.exportPoolFreeFooter(gb.toFixed(1));
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
      showToast(STRINGS.RENDER.recoveredJobsToast(summary.completed_count, summary.pending_count), 'info');
      if (onRecovered) onRecovered();
    } catch (err) {
      console.error('Error recovering render batch:', err);
      showToast(STRINGS.RENDER.recoverFailed(err), 'error');
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

export function initRenderUI(getRenderFolders, getExportDirs, onSettingsChange, takeTracking) {
  const scanRenderBtn = document.querySelector('#scan-render-btn');
  const startRenderBtn = document.querySelector('#start-render-btn');
  const cancelRenderBtn = document.querySelector('#cancel-render-btn');
  const renderStatusEl = document.querySelector('#render-status');

  const getTakeIndex = takeTracking?.getTakeIndex || null;
  const getAllDemos = takeTracking?.getAllDemos || null;
  const onTakeStatusChange = takeTracking?.onStatusChange || null;

  initErrorLogModal();

  // A take finishing renders — advances every highlight the take index says
  // fed that take_key to Rendered. The take index is what makes this work
  // even after a restart or re-scan replaced the original streak objects:
  // it was recorded by uid at capture time, not by a live object reference.
  listen('render_take_finished', (event) => {
    const { take_key: takeKey, take_folder: takeFolder } = event.payload || {};
    if (!takeKey) {
      // Folder has no session parent to derive a key from (e.g. copied out
      // of its session folder) — documented limitation, not an error.
      console.warn(`[render] Take finished with no resolvable take_key: ${takeFolder}`);
      return;
    }
    const takeIndex = getTakeIndex ? getTakeIndex() : null;
    const demos = getAllDemos ? getAllDemos() : null;
    if (!takeIndex || !demos) return;

    const uids = new Set(resolveTake(takeIndex, takeKey));
    // Logged unconditionally (hit or miss) so a lookup against a real,
    // already-loaded index is visible even when it resolves nothing — the
    // index's total entry count here is exactly what the previous
    // "[take-index] Loaded from ..." log reported, proving this lookup runs
    // against that same loaded data rather than something rebuilt in memory.
    console.log(`[take-index] Resolving ${takeKey} against ${Object.keys(takeIndex).length} loaded take(s) — found ${uids.size} uid(s)`, Array.from(uids));
    if (uids.size === 0) return;

    let advanced = 0;
    demos.forEach(demo => {
      (demo.streaks || []).forEach(streak => {
        if (!uids.has(streakUid(demo.path, streak))) return;
        // Idempotent: separate_hud produces two render jobs (all + hudcolor)
        // sharing one take_key, so this fires twice per take — the second
        // pass is just a no-op instead of a double-toast.
        if (streak.status === 'Rendered') return;
        streak.status = 'Rendered';
        advanced += 1;
      });
    });

    if (advanced > 0) {
      showToast(STRINGS.RENDER.highlightsMarkedRendered(advanced), 'success');
      if (onTakeStatusChange) onTakeStatusChange();
    }
  });

  // Real-time per-job state, pushed by the backend scheduler. This is the
  // single source of truth for whether a batch is actually running — driving
  // Start/Cancel off it (rather than only off the Start button's own click
  // handler and render_batch_finished) matters because reset_render_job can
  // resume the scheduler on its own, bypassing both of those. Without this,
  // Start Render Batch stayed clickable during a reset-triggered resume and
  // just repeatedly failed with "Render batch already in progress".
  listen('render_jobs_snapshot', (event) => {
    jobs = event.payload || [];
    renderJobsTable();
    const activeOrQueued = jobs.filter((j) => j.status === 'Rendering' || j.status === 'Queued').length;
    if (renderStatusEl) {
      renderStatusEl.textContent = activeOrQueued > 0
        ? STRINGS.RENDER.renderingStatus(jobs.length - activeOrQueued, jobs.length)
        : STRINGS.RENDER.STATUS_WAITING;
    }
    if (startRenderBtn) startRenderBtn.disabled = activeOrQueued > 0;
    if (cancelRenderBtn) cancelRenderBtn.disabled = activeOrQueued === 0;
  });

  listen('render_batch_finished', (event) => {
    const status = event.payload && event.payload.status;
    if (startRenderBtn) startRenderBtn.disabled = false;
    if (cancelRenderBtn) cancelRenderBtn.disabled = true;
    if (status === 'No takes found to render') {
      // status is backend-emitted text (out of scope for centralization) —
      // shown verbatim as it's the sole source of truth for this message.
      showToast(status, 'error');
      if (renderStatusEl) renderStatusEl.textContent = STRINGS.MAIN.statusGeneric(status);
      return;
    }
    const errored = jobs.some((j) => j.status === 'Error');
    const cancelled = jobs.some((j) => j.status === 'Cancelled');
    if (errored) {
      showToast(STRINGS.RENDER.BATCH_FINISHED_WITH_ERRORS, 'error');
    } else if (cancelled) {
      showToast(STRINGS.RENDER.BATCH_CANCELLED, 'info');
    } else if (jobs.length > 0) {
      showToast(STRINGS.RENDER.BATCH_COMPLETED, 'success');
    }
    if (renderStatusEl) renderStatusEl.textContent = STRINGS.RENDER.STATUS_FINISHED;
  });

  if (scanRenderBtn) {
    let scanFoundCount = 0;
    listen('render_scan_status', () => {
      // Payload is just "Found take: <name>" per take — the count is what
      // the status line needs, not the name, so this stays a cheap counter
      // rather than parsing/echoing every take's name into the DOM.
      scanFoundCount += 1;
      if (renderStatusEl) renderStatusEl.textContent = STRINGS.RENDER.scanFoundSoFar(scanFoundCount);
    });

    scanRenderBtn.addEventListener('click', () => {
      const renderFolders = getRenderFolders ? getRenderFolders() : [];
      if (!renderFolders || renderFolders.length === 0) {
        showToast(STRINGS.RENDER.ADD_RENDER_DIR_REQUIRED, 'error');
        return;
      }
      scanFoundCount = 0;
      showToast(STRINGS.RENDER.SCANNING_TOAST, 'info');
      if (renderStatusEl) renderStatusEl.textContent = STRINGS.RENDER.STATUS_SCANNING;
      scanRenderDirectories(renderFolders)
        .then((takes) => {
          const count = takes ? takes.length : 0;
          showToast(STRINGS.RENDER.scannedTakesToast(count), 'info');
          if (renderStatusEl) renderStatusEl.textContent = STRINGS.RENDER.scanCompleteStatus(count);

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
                : `<p style="color: #888;">${STRINGS.RENDER.NO_RENDER_TAKES_DETECTED}</p>`;
          }
        })
        .catch((err) => {
          console.error('IPC Execution Error (scan_render_directories):', err);
          showToast(STRINGS.RENDER.scanDirError(err), 'error');
          if (renderStatusEl) renderStatusEl.textContent = STRINGS.RENDER.STATUS_SCAN_FAILED;
        });
    });
  }

  if (startRenderBtn) {
    startRenderBtn.addEventListener('click', () => {
      const renderFolders = getRenderFolders ? getRenderFolders() : [];
      if (!renderFolders || renderFolders.length === 0) {
        showToast(STRINGS.RENDER.ADD_RENDER_DIR_REQUIRED, 'error');
        return;
      }

      const codecVal = document.querySelector('#render-codec-select')?.value || 'prores';
      const fpsVal = parseInt(document.querySelector('#render-fps-input')?.value, 10) || 300;
      const maxConcurrentVal = Math.min(8, Math.max(1, parseInt(document.querySelector('#render-max-concurrent-input')?.value, 10) || 2));
      checkNvencConcurrencyWarning();
      const exportDirs = (getExportDirs ? getExportDirs() : []).filter(Boolean);
      // FFmpeg override is shared with the capture config panel.
      const ffmpegPathVal = document.querySelector('#ffmpeg-override-path-input')?.value?.trim() || null;

      showToast(STRINGS.RENDER.INITIALIZING_RENDER_BATCH, 'info');
      startRenderBtn.disabled = true;
      if (cancelRenderBtn) cancelRenderBtn.disabled = false;
      if (renderStatusEl) renderStatusEl.textContent = STRINGS.RENDER.STATUS_SCANNING_FOR_TAKES;

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
          showToast(STRINGS.RENDER.RENDER_BATCH_QUEUED, 'success');
        })
        .catch((err) => {
          console.error('IPC Execution Error (executeRenderBatch):', err);
          showToast(STRINGS.RENDER.renderBatchError(err), 'error');
          if (startRenderBtn) startRenderBtn.disabled = false;
          if (cancelRenderBtn) cancelRenderBtn.disabled = true;
        });
    });
  }

  if (cancelRenderBtn) {
    cancelRenderBtn.addEventListener('click', () => {
      showToast(STRINGS.RENDER.CANCELLING_RENDER_BATCH, 'info');
      cancelRenderBtn.disabled = true;
      cancelRenderBatch().catch((err) => {
        console.error('IPC Execution Error (cancelRenderBatch):', err);
        if (startRenderBtn) startRenderBtn.disabled = false;
      });
    });
  }

  // Codec/FPS/max-concurrent aren't read until Start Render Batch is
  // clicked, but they're still persisted settings — nothing previously
  // wired their edits to a save.
  const codecEl = document.querySelector('#render-codec-select');
  const maxConcurrentEl = document.querySelector('#render-max-concurrent-input');
  // NVENC's concurrent-session limit is unlocked on Quadro/RTX 40-series+
  // but commonly capped at 3-5 on consumer GeForce cards — exceeding it
  // doesn't fail upfront, it surfaces as an opaque FFmpeg error buried in
  // a job's error log. Warn eagerly rather than let that be a mystery.
  function checkNvencConcurrencyWarning() {
    const isNvenc = (codecEl?.value || '') === 'h264_nvenc';
    const maxConcurrent = parseInt(maxConcurrentEl?.value, 10) || 0;
    if (isNvenc && maxConcurrent > 3) {
      showToast(
        STRINGS.RENDER.nvencWarning(maxConcurrent),
        'warning',
        6000
      );
    }
  }
  if (onSettingsChange) {
    if (codecEl) codecEl.addEventListener('change', () => onSettingsChange());
    const fpsEl = document.querySelector('#render-fps-input');
    if (fpsEl) fpsEl.addEventListener('input', () => onSettingsChange());
    if (maxConcurrentEl) maxConcurrentEl.addEventListener('input', () => onSettingsChange());
  }
  if (codecEl) codecEl.addEventListener('change', checkNvencConcurrencyWarning);
  if (maxConcurrentEl) maxConcurrentEl.addEventListener('change', checkNvencConcurrencyWarning);

  refreshExportPoolFree(getExportDirs);
  // Re-check free space periodically while the pane is open — cheap
  // filesystem calls, avoids the readout going stale during a long batch.
  setInterval(() => refreshExportPoolFree(getExportDirs), 15000);
}
