import { listen } from '@tauri-apps/api/event';
import { checkForUpdate, downloadAndInstallUpdate, restartApp } from './ipc_bridge.js';
import { notify } from './os_notifications.js';
import { STRINGS } from './strings.js';

function currentChannel() {
  return document.querySelector('#config-update-channel')?.value || 'stable';
}

function setStatus(text) {
  const el = document.querySelector('#update-status-text');
  if (el) el.textContent = text;
}

// Reflected on the persistent footer button (never hidden — see index.html's
// #update-check-btn) rather than a separate show/hide badge, so "check for
// updates" and "an update is waiting" are the same one-click affordance from
// any tab, not two different footer elements.
function setFooterButtonState(updateAvailable) {
  const btn = document.querySelector('#update-check-btn');
  if (!btn) return;
  if (updateAvailable) {
    btn.textContent = STRINGS.FOOTER.UPDATE_AVAILABLE_BUTTON;
    btn.title = STRINGS.FOOTER.UPDATE_AVAILABLE_TITLE;
    btn.style.color = '#5fb85f';
    btn.style.borderColor = '#5fb85f';
  } else {
    btn.textContent = STRINGS.FOOTER.CHECK_UPDATES_BUTTON;
    btn.title = STRINGS.FOOTER.CHECK_UPDATES_TITLE;
    btn.style.color = '';
    btn.style.borderColor = '';
  }
}

export async function checkForUpdatesNow(channel = currentChannel()) {
  setStatus(STRINGS.UPDATE_MODAL.STATUS_CHECKING);
  const downloadBtn = document.querySelector('#download-install-update-btn');

  let info;
  try {
    info = await checkForUpdate(channel);
  } catch (err) {
    // ipc_bridge.js already toasted the failure — this just reflects it
    // into the modal's inline status line too.
    setStatus(STRINGS.UPDATE_MODAL.STATUS_CHECK_FAILED(err));
    return;
  }

  if (info) {
    setStatus(STRINGS.UPDATE_MODAL.STATUS_AVAILABLE(info.version));
    if (downloadBtn) downloadBtn.style.display = 'inline-block';
    setFooterButtonState(true);
    notify(
      'updates',
      STRINGS.FOOTER.UPDATE_AVAILABLE_BUTTON,
      STRINGS.UPDATE_MODAL.STATUS_AVAILABLE(info.version),
    );
  } else {
    setStatus(STRINGS.UPDATE_MODAL.STATUS_UP_TO_DATE);
    if (downloadBtn) downloadBtn.style.display = 'none';
    setFooterButtonState(false);
  }
}

async function beginDownloadAndInstall() {
  const downloadBtn = document.querySelector('#download-install-update-btn');
  const progressContainer = document.querySelector('#update-progress-container');
  setStatus(STRINGS.UPDATE_MODAL.STATUS_DOWNLOADING);
  if (downloadBtn) downloadBtn.disabled = true;
  if (progressContainer) progressContainer.style.display = 'block';
  try {
    await downloadAndInstallUpdate();
  } catch {
    // ipc_bridge.js already toasted; leave the button re-enabled so the user
    // can retry rather than getting stuck on a dead "Download & Install".
    if (downloadBtn) downloadBtn.disabled = false;
  }
}

/** Wires the footer's Check for Updates button and the standalone update
 *  modal's controls, listens for download progress/completion, and —
 *  unless `settings.auto_check_updates` is off — runs one background check
 *  on startup against `settings.update_channel`. Called once from main.js's
 *  DOMContentLoaded, after settings have loaded. */
export function initUpdater(settings) {
  const modal = document.querySelector('#update-modal');

  document.querySelector('#update-check-btn')?.addEventListener('click', () => {
    if (modal) modal.style.display = 'flex';
  });

  document.querySelector('#update-modal-close-btn')?.addEventListener('click', () => {
    if (modal) modal.style.display = 'none';
  });

  document.querySelector('#check-updates-now-btn')
    ?.addEventListener('click', () => checkForUpdatesNow());

  document.querySelector('#download-install-update-btn')
    ?.addEventListener('click', () => beginDownloadAndInstall());

  document.querySelector('#restart-to-apply-btn')
    ?.addEventListener('click', () => restartApp());

  listen('update_download_progress', (event) => {
    const { downloaded, total } = event.payload || {};
    const bar = document.querySelector('#update-progress-bar');
    if (bar && total) {
      bar.max = total;
      bar.value = downloaded;
    }
  });

  listen('update_ready', () => {
    setStatus(STRINGS.UPDATE_MODAL.STATUS_READY);
    const downloadBtn = document.querySelector('#download-install-update-btn');
    if (downloadBtn) downloadBtn.style.display = 'none';
    const restartBtn = document.querySelector('#restart-to-apply-btn');
    if (restartBtn) restartBtn.style.display = 'inline-block';
    notify(
      'updates',
      STRINGS.UPDATE_MODAL.RESTART_BUTTON,
      STRINGS.UPDATE_MODAL.STATUS_READY,
    );
  });

  if (settings?.auto_check_updates === false) return;
  checkForUpdatesNow(settings?.update_channel || 'stable');
}
