import { open, save, confirm } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  scanDirectory,
  calculateExportPoolSpace,
  cancelScan,
  getSettings,
  saveSettings,
  logFrontendEvent,
  openActivityLog
} from './ipc_bridge.js';
import { renderMasterList, initMasterPane } from './master_pane.js';
import { renderDetailView, initDetailPane } from './detail_pane.js';
import { initCaptureUI, getCommandsState, hydrateCommandsState, refreshLaunchGuard } from './capture_pane.js';
import { initRenderUI, checkRenderRecoveryOnStartup } from './render_pane.js';
import { initAuditorPane } from './auditor_pane.js';
import { initAnalyzerPane } from './analyzer_pane.js';
import { switchNavTab, setCaptureDetailSubtab } from './nav.js';
import { showToast } from './toast.js';
import { createListEditor } from './list_editor.js';
import { preserveHighlightState, streakUid, pruneTakeIndex, isDemoTracked } from './take_index.js';
import { getCheckedDemoPaths, clearCheckedPaths, setCheckedDemoPaths, getVisibleDemos, recordingPlayerStreaks } from './master_pane.js';
import { initErrorReporter } from './error_reporter.js';

// Registered at module load, before DOMContentLoaded — so it's catching
// from the earliest possible moment, not just once the app's own init
// logic gets around to it.
initErrorReporter();

window.addEventListener("DOMContentLoaded", async () => {
  let scanPaths = [];
  // Analyzer Explorer sidebar's "Recent" quick-links tier — most-recent-first,
  // capped at 10, pushed via recordDemoFolderVisit() below whenever browsing
  // into a folder yields a non-empty demo listing. Mirrors dev's
  // `settings.demo_folder_history` (see docs/tauri_parity_audit.md Area 3).
  let demoFolderHistory = [];
  // Gates the Analyzer Explorer tree's per-subfolder demo-count badge.
  // Mirrors dev's `settings.scan_folders_for_demos`, default false.
  let scanFoldersForDemos = false;
  // Analyzer Explorer sidebar's drag-to-resize width (analyzer_pane.js).
  let analyzerExplorerWidth = 260;
  let targetDrives = [];
  let renderFolders = [];
  let renderExportDirs = []; // JIT multi-drive export pool for Render Studio
  let currentScannedDemos = [];
  let selectedDemoIdx = null;
  // The project session file last loaded or saved in this window, if any —
  // once set, "Save Session" writes straight back to it instead of asking
  // Save-As every time (matches Ctrl+S's behavior in every other app).
  let currentSessionPath = null;
  // take_key -> uid[]. Recorded by capture_pane.js when a batch verifies a
  // block on disk; resolved by render_pane.js when that take finishes
  // rendering, so status can auto-advance even after a restart or re-scan
  // replaced the original streak objects. Persisted in the project file.
  let takeIndex = {};
  // Connected-workspace mode (Phase 4): "quick-clip" (nothing persists,
  // blunt clearing, re-scan replaces demos wholesale) or "workspace"
  // (project file + take index persist, clearing protects tracked demos,
  // re-scan merges by uid). Defaults to Quick-Clip on a fresh install —
  // Load Session or "Save as Workspace..." are what switch it to Workspace.
  let studioMode = 'quick-clip';

  const STUDIO_MODE_CAPTIONS = {
    'quick-clip': "Nothing is saved to disk automatically — for grabbing a one-off clip. A re-scan replaces demos wholesale instead of preserving your edits.",
    'workspace': "Saves a project file and preserves your progress across restarts and re-scans.",
  };

  function applyStudioModeUI() {
    const switchInput = document.querySelector('#studio-mode-switch-input');
    if (switchInput) switchInput.checked = studioMode === 'workspace';
    document.querySelectorAll('.studio-mode-label').forEach(label => {
      label.classList.toggle('active', label.dataset.mode === studioMode);
    });
    const captionEl = document.querySelector('#studio-mode-caption');
    if (captionEl) captionEl.textContent = STUDIO_MODE_CAPTIONS[studioMode] || '';
    const saveBtnEl = document.querySelector('#save-project-btn');
    if (saveBtnEl) {
      saveBtnEl.textContent = studioMode === 'quick-clip' ? 'Save as Workspace…' : 'Save Session';
      saveBtnEl.title = studioMode === 'quick-clip'
        ? 'Saves a project file and switches this window to Workspace mode.'
        : 'Save Project Session';
    }
  }

  function setStudioMode(mode) {
    if (mode !== 'quick-clip' && mode !== 'workspace') return;
    if (studioMode === mode) return;
    studioMode = mode;
    applyStudioModeUI();
    persistAppSettings();
  }

  /** Every highlight's durable uid across every currently-scanned demo — the
   *  "still exists" set pruneTakeIndex() checks the take index against on save. */
  function collectAllUids() {
    const uids = [];
    currentScannedDemos.forEach(demo => {
      (demo.streaks || []).forEach(streak => uids.push(streakUid(demo.path, streak)));
    });
    return uids;
  }

  function updateSessionFileIndicator() {
    const el = document.querySelector('#session-file-indicator');
    if (!el) return;
    if (currentSessionPath) {
      const filename = currentSessionPath.split(/[\\/]/).pop() || currentSessionPath;
      el.textContent = filename;
      el.title = currentSessionPath;
    } else {
      el.textContent = 'No session loaded';
      el.title = '';
    }
  }

  // Initialize modular UI panes
  initAuditorPane();

  async function pickTargetDrive() {
    try {
      return await open({ directory: true, multiple: false, title: 'Select Capture Output Directory' });
    } catch (err) {
      console.error("Error opening capture output directory dialog:", err);
      return null;
    }
  }

  async function pickRenderFolder() {
    try {
      return await open({ directory: true, multiple: false, title: 'Select Render Directory' });
    } catch (err) {
      console.error("Error opening render directory dialog:", err);
      return null;
    }
  }

  async function pickRenderExportDir() {
    try {
      return await open({ directory: true, multiple: false, title: 'Select Render Export Directory' });
    } catch (err) {
      console.error("Error opening render export directory dialog:", err);
      return null;
    }
  }

  // Shared editable-list widget (list_editor.js) for the three folder/drive
  // pools — Capture Output, Render Folders, and Render Studio's Export
  // Drives all get add/edit/remove/reorder/browse from one implementation.
  const driveOverridesEditor = createListEditor({
    container: document.querySelector('#target-drive-list'),
    getItems: () => targetDrives,
    fields: [{ key: 'value', type: 'text', primitive: true, placeholder: 'Capture output directory path...' }],
    unique: true,
    browse: pickTargetDrive,
    onChange: () => {
      updateExportPoolIndicator();
      persistAppSettings();
      refreshLaunchGuard({ targetDrives, currentScannedDemos });
    },
  });

  const renderFoldersEditor = createListEditor({
    container: document.querySelector('#render-folder-list'),
    getItems: () => renderFolders,
    fields: [{ key: 'value', type: 'text', primitive: true, placeholder: 'Render directory path...' }],
    unique: true,
    browse: pickRenderFolder,
    onChange: () => persistAppSettings(),
  });

  const renderExportDirsEditor = createListEditor({
    container: document.querySelector('#render-export-dir-list'),
    getItems: () => renderExportDirs,
    fields: [{ key: 'value', type: 'text', primitive: true, placeholder: 'Export drive/folder path...' }],
    unique: true,
    browse: pickRenderExportDir,
    onChange: () => persistAppSettings(),
  });

  // Helper to persist application settings
  async function persistAppSettings() {
    const hlaePath = document.querySelector('#hlae-path-input')?.value?.trim() || "";
    const hlPath = document.querySelector('#hl-path-input')?.value?.trim() || "";
    const ffmpegPath = document.querySelector('#ffmpeg-override-path-input')?.value?.trim() || null;
    const captureFps = parseInt(document.querySelector('#config-capture-fps')?.value, 10) || 300;
    const preRoll = parseFloat(document.querySelector('#config-pre-roll')?.value) || 2.0;
    const postRoll = parseFloat(document.querySelector('#config-post-roll')?.value) || 0.6;

    const resWidth = parseInt(document.querySelector('#config-res-width')?.value, 10) || 1280;
    const resHeight = parseInt(document.querySelector('#config-res-height')?.value, 10) || 720;
    const separateHud = document.querySelector('#config-separate-hud')?.checked || false;
    const addCondebug = document.querySelector('#config-add-condebug')?.checked || false;

    const autoClearLogs = document.querySelector('#config-auto-clear-logs')?.checked || false;
    const autoClearPreviews = document.querySelector('#config-auto-clear-previews')?.checked || false;
    const autoClearTempDemos = document.querySelector('#config-auto-clear-temp-demos')?.checked || false;

    const recordStartLead = parseFloat(document.querySelector('#config-record-start-lead')?.value) || 0.0;
    const recordStopTrail = parseFloat(document.querySelector('#config-record-stop-trail')?.value) || 0.0;
    const initialDelay = parseFloat(document.querySelector('#config-initial-delay')?.value) || 3.0;
    const fastForwardSpeed = parseFloat(document.querySelector('#config-fast-forward-speed')?.value) || 0.05;

    const saveLocalPatchedCopy = document.querySelector('#config-save-local-patched')?.checked || false;

    const renderCodec = document.querySelector('#render-codec-select')?.value || 'prores';
    const renderFps = parseInt(document.querySelector('#render-fps-input')?.value, 10) || 300;
    const renderMaxConcurrent = parseInt(document.querySelector('#render-max-concurrent-input')?.value, 10) || 2;

    const { init_commands, custom_commands } = getCommandsState();

    const settingsPayload = {
      hlae_path: hlaePath,
      hl_path: hlPath,
      ffmpeg_path: ffmpegPath,
      pinned_folders: scanPaths,
      demo_folder_history: demoFolderHistory,
      scan_folders_for_demos: scanFoldersForDemos,
      analyzer_explorer_width: analyzerExplorerWidth,
      language: "en",
      capture_fps: captureFps,
      pre_roll_seconds: preRoll,
      post_roll_seconds: postRoll,
      resolution_width: resWidth,
      resolution_height: resHeight,
      separate_hud: separateHud,
      add_condebug: addCondebug,
      auto_clear_logs: autoClearLogs,
      auto_clear_previews: autoClearPreviews,
      auto_clear_temp_demos: autoClearTempDemos,
      record_start_lead: recordStartLead,
      record_stop_trail: recordStopTrail,
      initial_delay: initialDelay,
      fast_forward_speed: fastForwardSpeed,
      target_drives: targetDrives,
      init_commands,
      custom_commands,
      save_local_patched_copy: saveLocalPatchedCopy,
      render_folders: renderFolders,
      render_codec: renderCodec,
      render_fps: renderFps,
      render_max_concurrent: renderMaxConcurrent,
      render_export_dirs: renderExportDirs,
      studio_mode: studioMode
    };
    try {
      await saveSettings(settingsPayload);
    } catch (err) {
      console.error("Error auto-saving settings:", err);
    }
  }

  // Load persistent settings on startup
  try {
    const settings = await getSettings();
    if (settings) {
      if (settings.hlae_path) {
        const inputEl = document.querySelector('#hlae-path-input');
        if (inputEl) inputEl.value = settings.hlae_path;
      }
      if (settings.hl_path) {
        const inputEl = document.querySelector('#hl-path-input');
        if (inputEl) inputEl.value = settings.hl_path;
      }
      if (settings.ffmpeg_path) {
        const inputEl = document.querySelector('#ffmpeg-override-path-input');
        if (inputEl) inputEl.value = settings.ffmpeg_path;
      }
      if (settings.capture_fps) {
        const inputEl = document.querySelector('#config-capture-fps');
        if (inputEl) inputEl.value = settings.capture_fps;
      }
      if (settings.pre_roll_seconds) {
        const inputEl = document.querySelector('#config-pre-roll');
        if (inputEl) inputEl.value = settings.pre_roll_seconds;
      }
      if (settings.post_roll_seconds) {
        const inputEl = document.querySelector('#config-post-roll');
        if (inputEl) inputEl.value = settings.post_roll_seconds;
      }
      if (settings.resolution_width) {
        const inputEl = document.querySelector('#config-res-width');
        if (inputEl) inputEl.value = settings.resolution_width;
      }
      if (settings.resolution_height) {
        const inputEl = document.querySelector('#config-res-height');
        if (inputEl) inputEl.value = settings.resolution_height;
      }
      const separateHudEl = document.querySelector('#config-separate-hud');
      if (separateHudEl) separateHudEl.checked = !!settings.separate_hud;
      const addCondebugEl = document.querySelector('#config-add-condebug');
      if (addCondebugEl) addCondebugEl.checked = !!settings.add_condebug;
      const autoClearLogsEl = document.querySelector('#config-auto-clear-logs');
      if (autoClearLogsEl) autoClearLogsEl.checked = !!settings.auto_clear_logs;
      const autoClearPreviewsEl = document.querySelector('#config-auto-clear-previews');
      if (autoClearPreviewsEl) autoClearPreviewsEl.checked = !!settings.auto_clear_previews;
      const autoClearTempDemosEl = document.querySelector('#config-auto-clear-temp-demos');
      if (autoClearTempDemosEl) autoClearTempDemosEl.checked = !!settings.auto_clear_temp_demos;
      if (settings.record_start_lead) {
        const inputEl = document.querySelector('#config-record-start-lead');
        if (inputEl) inputEl.value = settings.record_start_lead;
      }
      if (settings.record_stop_trail) {
        const inputEl = document.querySelector('#config-record-stop-trail');
        if (inputEl) inputEl.value = settings.record_stop_trail;
      }
      if (settings.initial_delay) {
        const inputEl = document.querySelector('#config-initial-delay');
        if (inputEl) inputEl.value = settings.initial_delay;
      }
      if (settings.fast_forward_speed) {
        const inputEl = document.querySelector('#config-fast-forward-speed');
        if (inputEl) inputEl.value = settings.fast_forward_speed;
      }
      const saveLocalPatchedEl = document.querySelector('#config-save-local-patched');
      if (saveLocalPatchedEl) saveLocalPatchedEl.checked = !!settings.save_local_patched_copy;
      if (settings.render_codec) {
        const inputEl = document.querySelector('#render-codec-select');
        if (inputEl) inputEl.value = settings.render_codec;
      }
      if (settings.render_fps) {
        const inputEl = document.querySelector('#render-fps-input');
        if (inputEl) inputEl.value = settings.render_fps;
      }
      if (settings.render_max_concurrent) {
        const inputEl = document.querySelector('#render-max-concurrent-input');
        if (inputEl) inputEl.value = settings.render_max_concurrent;
      }
      if (Array.isArray(settings.pinned_folders) && settings.pinned_folders.length > 0) {
        scanPaths = [...settings.pinned_folders];
      }
      if (Array.isArray(settings.demo_folder_history) && settings.demo_folder_history.length > 0) {
        demoFolderHistory = [...settings.demo_folder_history];
      }
      scanFoldersForDemos = !!settings.scan_folders_for_demos;
      if (settings.analyzer_explorer_width) {
        analyzerExplorerWidth = settings.analyzer_explorer_width;
      }
      if (Array.isArray(settings.target_drives) && settings.target_drives.length > 0) {
        targetDrives = [...settings.target_drives];
        driveOverridesEditor.render();
        updateExportPoolIndicator();
      }
      if (Array.isArray(settings.render_folders) && settings.render_folders.length > 0) {
        renderFolders = [...settings.render_folders];
        renderFoldersEditor.render();
      }
      if (Array.isArray(settings.render_export_dirs) && settings.render_export_dirs.length > 0) {
        renderExportDirs = [...settings.render_export_dirs];
        renderExportDirsEditor.render();
      }
      hydrateCommandsState(settings.init_commands, settings.custom_commands);
      if (settings.studio_mode === 'workspace' || settings.studio_mode === 'quick-clip') {
        studioMode = settings.studio_mode;
      }
    }
  } catch (err) {
    console.error("Error loading startup settings:", err);
  }
  applyStudioModeUI();

  const studioModeSwitchInput = document.querySelector('#studio-mode-switch-input');
  if (studioModeSwitchInput) {
    studioModeSwitchInput.addEventListener('change', (e) => {
      setStudioMode(e.target.checked ? 'workspace' : 'quick-clip');
    });
  }
  // Clicking either label also flips the switch — matches how most
  // dual-label toggle switches behave, not just the slider itself.
  document.querySelectorAll('.studio-mode-label').forEach(label => {
    label.addEventListener('click', () => setStudioMode(label.dataset.mode));
  });

  // Save Project Session — also called from the Clear All modal's "Save
  // Session First" action, so it lives here as a plain function rather than
  // only inline in the button's click handler. Returns whether it actually
  // wrote a file (false on "nothing to save" or a cancelled Save-As dialog).
  async function saveProjectSession() {
    if (currentScannedDemos.length === 0) {
      showToast("Nothing to save yet — add demo files or load a session first.", 'info');
      return false;
    }
    try {
      // Once a session's been loaded or saved once in this window, keep
      // writing back to that same file instead of asking Save-As again.
      const filePath = currentSessionPath || await save({
        title: 'Save Studio Project Session',
        defaultPath: 'dod_project.json',
        filters: [{ name: 'JSON Project File', extensions: ['json'] }]
      });
      if (!filePath) return false;

      const hlaePath = document.querySelector('#hlae-path-input')?.value || "";
      const hlPath = document.querySelector('#hl-path-input')?.value || "";
      const projectData = JSON.stringify({
        version: "0.12.0",
        scanPaths: scanPaths,
        demos: currentScannedDemos,
        hlaePath: hlaePath,
        hlPath: hlPath,
        // Pruned against what's actually still scanned so the index
        // doesn't accumulate uids for demos removed from the project.
        takeIndex: pruneTakeIndex(takeIndex, collectAllUids()),
        // Informational — Load Session always forces Workspace mode
        // regardless of this value, since loading a persistent file is
        // itself the thing that makes a window a Workspace.
        mode: 'workspace'
      }, null, 2);
      await invoke('save_project_session', { path: filePath, contents: projectData });
      currentSessionPath = filePath;
      updateSessionFileIndicator();
      showToast(`Project session saved successfully to ${filePath}`, 'success');
      // Saving a project file is what makes a window a Workspace — a save
      // from Quick-Clip mode is exactly the "Save as Workspace..." action.
      setStudioMode('workspace');
      return true;
    } catch (err) {
      console.error("Save project error:", err);
      showToast("Error saving project session.", 'error');
      return false;
    }
  }

  const viewLogsBtn = document.querySelector('#view-logs-btn');
  if (viewLogsBtn) {
    viewLogsBtn.addEventListener('click', () => openActivityLog());
  }

  const saveProjectBtn = document.querySelector('#save-project-btn');
  if (saveProjectBtn) {
    saveProjectBtn.addEventListener('click', () => saveProjectSession());
  }

  // Load Project Session
  const loadProjectBtn = document.querySelector('#load-project-btn');
  if (loadProjectBtn) {
    loadProjectBtn.addEventListener('click', async () => {
      try {
        const selected = await open({
          multiple: false,
          filters: [{ name: 'JSON Project File', extensions: ['json'] }]
        });
        if (selected) {
          const content = await invoke('load_project_session', { path: selected });
          const data = JSON.parse(content);
          if (data) {
            currentSessionPath = selected;
            updateSessionFileIndicator();
            // Loading a persistent project file is what makes a window a
            // Workspace, regardless of what `data.mode` says (a hand-edited
            // or older file might omit it or say otherwise).
            setStudioMode('workspace');
            clearCheckedPaths();
            if (data.hlaePath) {
              const hlaeInput = document.querySelector('#hlae-path-input');
              if (hlaeInput) hlaeInput.value = data.hlaePath;
            }
            if (data.hlPath) {
              const hlInput = document.querySelector('#hl-path-input');
              if (hlInput) hlInput.value = data.hlPath;
            }
            // Tolerant: a 0.10.0 project file has no takeIndex at all — load
            // as empty rather than reject the file. Auto-Rendered just won't
            // retroactively apply to takes captured before this existed.
            takeIndex = data.takeIndex || {};
            // Deliberately verbose: this is the only place takeIndex is ever
            // populated from disk, so logging it here — with exactly what
            // came out of the file, before anything else touches it — is
            // what makes it possible to prove a later auto-Rendered flip
            // came from this loaded data and not a leftover in-memory state.
            console.log(`[take-index] Loaded from ${selected}: ${Object.keys(takeIndex).length} take(s)`, takeIndex);
            if (data.demos) {
              currentScannedDemos = data.demos;
              selectedDemoIdx = currentScannedDemos.length > 0 ? 0 : null;
              renderMasterList(currentScannedDemos, selectedDemoIdx, selectDemoAndRenderDetail);
              if (currentScannedDemos.length > 0) {
                selectDemoAndRenderDetail(currentScannedDemos[0], selectedDemoIdx);
              }
              updateDemoFooter(currentScannedDemos);
              showToast(`Loaded ${currentScannedDemos.length} demos from project file`, 'success');
            }
          }
        }
      } catch (err) {
        console.error("Load project error:", err);
        showToast("Error loading project session.", 'error');
      }
    });
  }

  // Executable & Path Browse Dialog Pickers
  const hlaeBrowseBtn = document.querySelector('#hlae-browse-btn');
  if (hlaeBrowseBtn) {
    hlaeBrowseBtn.addEventListener('click', async () => {
      try {
        const selected = await open({
          multiple: false,
          filters: [{ name: 'Executable', extensions: ['exe'] }],
          title: 'Select HLAE Executable (hlae.exe)'
        });
        if (selected) {
          const path = Array.isArray(selected) ? selected[0] : selected;
          const inputEl = document.querySelector('#hlae-path-input');
          if (inputEl) inputEl.value = path;
          await persistAppSettings();
        }
      } catch (err) {
        console.error("Error selecting HLAE executable:", err);
      }
    });
  }

  const hlBrowseBtn = document.querySelector('#hl-browse-btn');
  if (hlBrowseBtn) {
    hlBrowseBtn.addEventListener('click', async () => {
      try {
        const selected = await open({
          multiple: false,
          filters: [{ name: 'Executable', extensions: ['exe'] }],
          title: 'Select Half-Life Executable (hl.exe)'
        });
        if (selected) {
          const path = Array.isArray(selected) ? selected[0] : selected;
          const inputEl = document.querySelector('#hl-path-input');
          if (inputEl) inputEl.value = path;
          await persistAppSettings();
        }
      } catch (err) {
        console.error("Error selecting Half-Life executable:", err);
      }
    });
  }

  const ffmpegBrowseBtn = document.querySelector('#ffmpeg-browse-btn');
  if (ffmpegBrowseBtn) {
    ffmpegBrowseBtn.addEventListener('click', async () => {
      try {
        const selected = await open({
          multiple: false,
          filters: [{ name: 'Executable', extensions: ['exe'] }],
          title: 'Select FFmpeg Executable (ffmpeg.exe)'
        });
        if (selected) {
          const path = Array.isArray(selected) ? selected[0] : selected;
          const inputEl = document.querySelector('#ffmpeg-override-path-input');
          if (inputEl) inputEl.value = path;
          await persistAppSettings();
        }
      } catch (err) {
        console.error("Error selecting FFmpeg executable:", err);
      }
    });
  }

  // ── Shared "a demo row was selected" handler ────────────────────────────────
  // Extracted so every renderMasterList call site behaves identically —
  // renderMasterList persists whichever callback it was last given
  // (master_pane.js's currentOnSelectDemo) as the fallback for re-renders
  // that don't pass one, so an inline callback that drifted from this one
  // would apply inconsistently depending on which code path rendered last.
  function selectDemoAndRenderDetail(demo, idx) {
    selectedDemoIdx = idx;
    renderDetailView(demo, selectedDemoIdx);
  }

  // ── Demo list footer helper ────────────────────────────────────────────────
  function updateDemoFooter(demos) {
    const footerEl = document.querySelector('#demo-list-footer');
    if (footerEl) {
      // Same recording-player filter the Highlights column uses (M2) — this
      // used to sum every player's streaks in the demo, not just the
      // recording player's, and could read in the hundreds where the
      // visible Highlights column summed to a fraction of that.
      const totalHighlights = (demos || []).reduce((sum, d) => sum + recordingPlayerStreaks(d).length, 0);
      footerEl.textContent = `Loaded Demos: ${(demos || []).length} | Total Highlights: ${totalHighlights}`;
    }
    // Clear Untracked/Clear All only make sense with something in the
    // queue — Clear Selected already gates on its own checkbox state
    // (master_pane.js), this is the same idea for the other two.
    const isEmpty = !demos || demos.length === 0;
    const clearUntrackedBtnEl = document.querySelector('#clear-untracked-btn');
    if (clearUntrackedBtnEl) clearUntrackedBtnEl.disabled = isEmpty;
    const clearAllBtnEl = document.querySelector('#clear-all-btn');
    if (clearAllBtnEl) clearAllBtnEl.disabled = isEmpty;
  }

  // ── scan_progress event listener (registered once on load) ────────────────
  let unlistenScanProgress = null;
  listen('scan_progress', (event) => {
    const p = event.payload || {};
    const scanStatusEl = document.querySelector('#scan-status');
    const cancelScanBtn = document.querySelector('#cancel-scan-btn');
    const masterTableBody = document.querySelector('#master-demo-table-body');

    if (p.cancelled) {
      if (scanStatusEl) scanStatusEl.textContent = `Status: Cancelled — ${p.found} demo(s) found before cancel`;
      if (cancelScanBtn) cancelScanBtn.disabled = true;
    } else if (p.status === 'Complete') {
      if (scanStatusEl) scanStatusEl.textContent = `Status: Ready — ${p.found} demo(s) found`;
      if (cancelScanBtn) cancelScanBtn.disabled = true;
    } else {
      if (scanStatusEl) scanStatusEl.textContent = `Status: ${p.status}`;
      if (cancelScanBtn) cancelScanBtn.disabled = false;
    }
  }).then(fn => { unlistenScanProgress = fn; });

  // ── Cancel Scan button ────────────────────────────────────────────────────
  const cancelScanBtn = document.querySelector('#cancel-scan-btn');
  if (cancelScanBtn) {
    cancelScanBtn.disabled = true;
    cancelScanBtn.addEventListener('click', async () => {
      cancelScanBtn.disabled = true;
      try {
        await cancelScan();
        showToast('Scan cancellation requested.', 'info');
      } catch (_) { /* already toasted in ipc_bridge */ }
    });
  }

  // Scans only the given paths and merges the results into the existing
  // master list (replacing entries with matching `path`, appending new ones).
  // `scanPaths` itself is a separately-persisted "known library" list reused
  // by the capture batch payload (`capture_directories`) — it must NOT be
  // re-walked on every add, or every scan re-processes every folder ever
  // added across the app's lifetime (dev only ever re-ingests the paths just
  // picked in that action; see views/capture/workspace.rs Add Files/Add Folder).
  async function triggerAutoScan(pathsToScan) {
    if (!pathsToScan || pathsToScan.length === 0) return;

    const scanStatusEl = document.querySelector('#scan-status');
    const addFilesBtn = document.querySelector('#add-files-btn');
    const addFolderBtn = document.querySelector('#add-folder-btn');
    const cancelScanBtnInner = document.querySelector('#cancel-scan-btn');

    if (addFilesBtn) addFilesBtn.disabled = true;
    if (addFolderBtn) addFolderBtn.disabled = true;
    if (cancelScanBtnInner) cancelScanBtnInner.disabled = false;
    if (scanStatusEl) scanStatusEl.textContent = 'Status: Scanning...';
    showToast("Scanning directories...", 'info');

    const masterTableBody = document.querySelector('#master-demo-table-body');
    if (masterTableBody) masterTableBody.innerHTML = '<tr style="text-align:center"><td colspan="7">Scanning... please wait.</td></tr>';

    try {
      const newlyScanned = await scanDirectory(pathsToScan);

      // Merge: replace any existing demo with the same path, append new ones.
      // (Prior behavior replaced the whole master list with the result of
      // re-scanning everything in `scanPaths`, which is what caused a single
      // "Add Demo Files" click to report hundreds of demos.)
      const indexByPath = new Map(currentScannedDemos.map((d, i) => [d.path, i]));
      newlyScanned.forEach((demo) => {
        const existingIdx = indexByPath.get(demo.path);
        if (existingIdx !== undefined) {
          // Workspace mode: a re-scan produces brand new streak objects, so
          // replacing outright would wipe every status, selection, note and
          // Kill Range edit on this demo — carry that user-owned state
          // across by highlight uid. Quick-Clip mode intentionally keeps the
          // old blunt behavior (nothing is meant to survive a re-scan there
          // anyway) — simpler and matches "nothing persists" for that mode.
          currentScannedDemos[existingIdx] = studioMode === 'workspace'
            ? preserveHighlightState(currentScannedDemos[existingIdx], demo)
            : demo;
        } else {
          indexByPath.set(demo.path, currentScannedDemos.length);
          currentScannedDemos.push(demo);
        }
      });

      // footer is also updated on the Complete scan_progress event, but set
      // it here in case the event arrives before renderMasterList finishes.
      updateDemoFooter(currentScannedDemos);
      showToast(`Scan complete (${newlyScanned.length} demo(s) found)`, 'success');
      selectedDemoIdx = newlyScanned.length > 0
        ? currentScannedDemos.indexOf(newlyScanned[0])
        : (currentScannedDemos.length > 0 ? 0 : null);
      // renderMasterList calls this with (null, null) whenever the queue is
      // empty (e.g. after Clear All) — selectDemoAndRenderDetail resets the
      // telemetry UI instead of dereferencing a demo that isn't there.
      renderMasterList(currentScannedDemos, selectedDemoIdx, selectDemoAndRenderDetail);
      if (selectedDemoIdx !== null) {
        selectDemoAndRenderDetail(currentScannedDemos[selectedDemoIdx], selectedDemoIdx);
      }
    } catch (err) {
      console.error("Error scanning directories:", err);
      showToast("Error: " + err, 'error');
      if (scanStatusEl) scanStatusEl.textContent = `Status: Error — ${err}`;
    } finally {
      if (addFilesBtn) addFilesBtn.disabled = false;
      if (addFolderBtn) addFolderBtn.disabled = false;
      if (cancelScanBtnInner) cancelScanBtnInner.disabled = true;
    }
  }

  // Native Demo Files Ingestion (+ Add Demo Files)
  const addFilesBtn = document.querySelector('#add-files-btn');
  if (addFilesBtn) {
    addFilesBtn.addEventListener('click', async () => {
      try {
        const selected = await open({
          multiple: true,
          filters: [{ name: 'Demo Files', extensions: ['dem'] }],
          title: 'Select Demo Files (.dem)'
        });
        if (selected) {
          const files = Array.isArray(selected) ? selected : [selected];
          files.forEach(f => {
            if (!scanPaths.includes(f)) {
              scanPaths.push(f);
            }
          });
          await persistAppSettings();
          // Scan only the files just picked, not the full accumulated
          // scanPaths history — see triggerAutoScan's doc comment.
          await triggerAutoScan(files);
        }
      } catch (err) {
        console.error("Error opening demo files dialog:", err);
      }
    });
  }

  // Native Demo Folder Ingestion (+ Add Folder)
  const addFolderBtn = document.querySelector('#add-folder-btn');
  if (addFolderBtn) {
    addFolderBtn.addEventListener('click', async () => {
      try {
        const selected = await open({
          directory: true,
          multiple: false,
          title: 'Select Demo Folder'
        });
        if (selected) {
          const folder = Array.isArray(selected) ? selected[0] : selected;
          if (!scanPaths.includes(folder)) {
            scanPaths.push(folder);
            await persistAppSettings();
            await triggerAutoScan([folder]);
          }
        }
      } catch (err) {
        console.error("Error opening demo directory dialog:", err);
      }
    });
  }

  async function updateExportPoolIndicator() {
    const indicator = document.querySelector('#export-pool-free-indicator');
    if (!indicator) return;
    if (targetDrives.length === 0) {
      indicator.textContent = "Capture Output Free: 0.0 GB";
      return;
    }
    try {
      const bytes = await calculateExportPoolSpace(targetDrives);
      const gb = bytes / (1024 * 1024 * 1024);
      indicator.textContent = `Capture Output Free: ${gb.toFixed(1)} GB`;
    } catch (err) {
      console.error("Error calculating export pool space:", err);
      indicator.textContent = "Capture Output Free: Error calculating space";
    }
  }

  // Target drives management
  document.querySelector('#add-drive-btn').addEventListener('click', () => {
    const driveEl = document.querySelector('#drive-path-input');
    const drivePath = driveEl.value.trim();
    if (drivePath && driveOverridesEditor.addItem(drivePath)) {
      driveEl.value = "";
    }
  });

  const browseDriveBtn = document.querySelector('#browse-drive-btn');
  if (browseDriveBtn) {
    browseDriveBtn.addEventListener('click', async () => {
      const selected = await pickTargetDrive();
      if (selected) driveOverridesEditor.addItem(selected);
    });
  }

  // Render folders management
  document.querySelector('#add-render-folder-btn').addEventListener('click', () => {
    const inputEl = document.querySelector('#render-path-input');
    const path = inputEl.value.trim();
    if (path && renderFoldersEditor.addItem(path)) {
      inputEl.value = "";
    }
  });

  const browseRenderFolderBtn = document.querySelector('#browse-render-folder-btn');
  if (browseRenderFolderBtn) {
    browseRenderFolderBtn.addEventListener('click', async () => {
      const selected = await pickRenderFolder();
      if (selected) renderFoldersEditor.addItem(selected);
    });
  }

  // JIT multi-drive export pool for Render Studio
  const addRenderExportDirBtn = document.querySelector('#add-render-export-dir-btn');
  if (addRenderExportDirBtn) {
    addRenderExportDirBtn.addEventListener('click', () => {
      const inputEl = document.querySelector('#render-export-dir-input');
      const path = inputEl?.value?.trim();
      if (path && renderExportDirsEditor.addItem(path) && inputEl) {
        inputEl.value = '';
      }
    });
  }

  const browseRenderExportBtn = document.querySelector('#browse-render-export-btn');
  if (browseRenderExportBtn) {
    browseRenderExportBtn.addEventListener('click', async () => {
      const selected = await pickRenderExportDir();
      if (selected) renderExportDirsEditor.addItem(selected);
    });
  }

  // Export Configuration tab switching
  const tabBtns = document.querySelectorAll('.config-tab-btn');
  tabBtns.forEach(btn => {
    btn.addEventListener('click', () => {
      tabBtns.forEach(b => b.classList.remove('active'));
      document.querySelectorAll('.config-tab-content').forEach(content => {
        content.style.display = 'none';
      });
      btn.classList.add('active');
      const targetId = btn.getAttribute('data-tab');
      const targetEl = document.getElementById(targetId);
      if (targetEl) targetEl.style.display = 'block';
    });
  });

  // Top nav bar view routing (shared with detail_pane.js — see nav.js)
  switchNavTab('workspace');

  const navTabBtns = document.querySelectorAll('.nav-tab-btn');
  navTabBtns.forEach(btn => {
    btn.addEventListener('click', () => switchNavTab(btn.getAttribute('data-nav')));
  });

  // Capture Studio in-workflow phase switch (Highlights <-> Configuration) —
  // replaces the old "Batch Capture Config" top-level nav tab, see nav.js.
  document.querySelectorAll('.capture-detail-subtab-btn').forEach((btn) => {
    btn.addEventListener('click', () => setCaptureDetailSubtab(btn.dataset.captureSubtab));
  });

  // Initialize Capture Batch UI
  // Shared by both auto-status paths (a verified capture, a finished render):
  // both tables read status and neither observes the streak objects on its
  // own, so re-render both — the Master Queue for its Pending/Captured/
  // Rendered counts, and the detail view for the per-row status dropdowns.
  const onHighlightStatusChange = () => {
    renderMasterList(currentScannedDemos, selectedDemoIdx);
    if (selectedDemoIdx !== null && currentScannedDemos[selectedDemoIdx]) {
      renderDetailView(currentScannedDemos[selectedDemoIdx], selectedDemoIdx);
    }
  };

  initCaptureUI(() => ({
    scanPaths,
    targetDrives,
    currentScannedDemos
  }), persistAppSettings, onHighlightStatusChange, () => takeIndex);

  // Initialize Render Studio UI
  initRenderUI(() => renderFolders, () => renderExportDirs, persistAppSettings, {
    getTakeIndex: () => takeIndex,
    getAllDemos: () => currentScannedDemos,
    onStatusChange: onHighlightStatusChange
  });

  // Render-batch crash-recovery prompt — checked once on startup, same
  // pattern as dev's StartupState::PendingRenderRecovery.
  checkRenderRecoveryOnStartup(() => switchNavTab('render-studio'));

  // Delete callback: remove a demo from the active scan list and re-render.
  // Called by master_pane.js when the 🗑 button is clicked on a row. Returns
  // the new selectedDemoIdx so the caller's own renderMasterList call can
  // pass it through for the row-highlight — master_pane.js doesn't otherwise
  // know this file's selectedDemoIdx, and previously always re-rendered with
  // no selection at all (visually dropping the highlight) even when the
  // deleted row wasn't the selected one and the selection should have held.
  const onDeleteDemo = (deletedOriginalIdx, updatedDemos) => {
    currentScannedDemos = updatedDemos;
    // If the deleted demo was the selected one, clear the detail view.
    if (selectedDemoIdx === deletedOriginalIdx) {
      selectedDemoIdx = currentScannedDemos.length > 0 ? 0 : null;
      if (selectedDemoIdx !== null) {
        renderDetailView(currentScannedDemos[0], selectedDemoIdx);
      } else {
        renderDetailView(null, null);
      }
    } else if (selectedDemoIdx !== null && selectedDemoIdx > deletedOriginalIdx) {
      // Shift selection index down if a demo above it was removed.
      selectedDemoIdx -= 1;
    }
    updateDemoFooter(currentScannedDemos);
    return selectedDemoIdx;
  };

  // Shared by all three Clear actions below: swaps the scanned-demo list,
  // resets checkboxes, and re-renders both the queue and the detail view.
  //
  // Selection is preserved by object identity, not just reset to row 0:
  // Clear Untracked/Selected/All can all leave the previously-selected demo
  // still in the queue (it just wasn't the one removed), and jumping the
  // selection to whatever's now first is jarring — you were looking at one
  // demo's highlights and suddenly a different one's are shown. Only falls
  // back to row 0 (or nothing) when the selected demo was actually the one
  // that got removed.
  function replaceScannedDemos(newDemos) {
    const previouslySelectedDemo = selectedDemoIdx !== null ? currentScannedDemos[selectedDemoIdx] : null;
    currentScannedDemos = newDemos;
    const preservedIdx = previouslySelectedDemo ? currentScannedDemos.indexOf(previouslySelectedDemo) : -1;
    selectedDemoIdx = preservedIdx !== -1 ? preservedIdx : (currentScannedDemos.length > 0 ? 0 : null);
    clearCheckedPaths();
    updateDemoFooter(currentScannedDemos);
    renderMasterList(currentScannedDemos, selectedDemoIdx);
    renderDetailView(selectedDemoIdx !== null ? currentScannedDemos[selectedDemoIdx] : null, selectedDemoIdx);
  }

  // One-line callout appended to Clear actions' toasts/summaries whenever an
  // active search filter narrowed what got acted on, so "Clear All" (etc.)
  // doesn't silently do less than its name implies without the user noticing.
  function filterScopeNote(visibleCount, totalCount) {
    return visibleCount < totalCount
      ? ` (search filter active — only considered ${visibleCount} of ${totalCount} demo(s) in the queue)`
      : '';
  }

  // Clear Untracked — removes only demos with no tracked work (isDemoTracked),
  // same in both modes. Its own name already means "never touches tracked
  // demos," so unlike Clear All there's nothing mode-dependent to decide
  // here — Quick-Clip doesn't need a separate blunt-wipe branch. Scoped to
  // the currently search-filtered demos, matching the select-all checkbox —
  // a demo hidden by the search box is left untouched no matter its status.
  const clearUntrackedBtn = document.querySelector('#clear-untracked-btn');
  if (clearUntrackedBtn) {
    clearUntrackedBtn.addEventListener('click', () => {
      if (currentScannedDemos.length === 0) {
        showToast('Queue is already empty.', 'info');
        return;
      }
      const visible = getVisibleDemos();
      if (visible.length === 0) {
        showToast('No demos match the current search.', 'info');
        return;
      }
      const trackedVisibleCount = visible.filter(isDemoTracked).length;
      const untrackedVisible = new Set(visible.filter((d) => !isDemoTracked(d)).map((d) => d.path));
      if (untrackedVisible.size === 0) {
        showToast('Nothing to clear — every visible demo has tracked work on it.', 'info');
        return;
      }
      const totalCount = currentScannedDemos.length;
      const removedNames = currentScannedDemos.filter((d) => untrackedVisible.has(d.path)).map((d) => d.name || d.path);
      replaceScannedDemos(currentScannedDemos.filter((d) => !untrackedVisible.has(d.path)));
      // "Kept N with tracked work" only ever refers to visible demos that
      // were actually evaluated and found tracked — never demos hidden by
      // the search filter, which weren't touched for a completely different
      // reason and would otherwise get mislabeled as "kept ... tracked".
      const keptNote = trackedVisibleCount > 0 ? `, kept ${trackedVisibleCount} with tracked work` : '';
      const scopeNote = filterScopeNote(visible.length, totalCount);
      showToast(
        `Removed ${untrackedVisible.size} untracked demo(s)${keptNote}.${scopeNote}`,
        'success'
      );
      logFrontendEvent(`[queue] Clear Untracked: removed ${untrackedVisible.size} demo(s)${keptNote}.${scopeNote} — ${removedNames.join(', ')}`);
    });
  }

  // Shared "tracked work at risk" confirmation modal — a pure yes/no
  // primitive (with an optional Save-First detour), awaited by every caller
  // that needs to warn before removing tracked demos: Clear Selected, Clear
  // All, and the row-level delete button (master_pane.js, via
  // requestTrackedDeleteConfirm below). It does NOT perform the removal
  // itself — Clear All/Selected replace the whole scanned-demo list, while
  // the row delete button needs to preserve its own selection-shift logic
  // instead, so "how to remove" stays with each caller; the modal only
  // answers "should we." Resolves `false` on Cancel, `'confirm'` on Confirm,
  // `'save-first'` once a save actually succeeded — the last two are both
  // truthy but let callers word their success toast accordingly.
  let pendingConfirmResolve = null;
  const clearAllModal = document.querySelector('#clear-all-modal');

  function requestTrackedClearConfirmation(targets, { title, verb, filterNote, confirmLabel }) {
    const trackedCount = targets.filter(isDemoTracked).length;
    const plural = targets.length === 1 ? 'demo' : 'demos';
    const titleEl = document.querySelector('#clear-all-title');
    if (titleEl) titleEl.textContent = title;
    const confirmBtnEl = document.querySelector('#clear-all-confirm-btn');
    if (confirmBtnEl) confirmBtnEl.textContent = confirmLabel || 'Clear Anyway';
    const summaryEl = document.querySelector('#clear-all-summary');
    if (summaryEl) {
      summaryEl.textContent = (trackedCount > 0
        ? `This ${verb} ${targets.length} ${plural} — ${trackedCount} of them have tracked work (Captured/Rendered status, a note, or an edited kill range) that will be lost. This cannot be undone.`
        : `This ${verb} ${targets.length} ${plural}. None currently have tracked work on them. This cannot be undone.`
      ) + (filterNote || '');
    }
    if (clearAllModal) clearAllModal.style.display = 'flex';
    return new Promise(resolve => { pendingConfirmResolve = resolve; });
  }

  if (clearAllModal) {
    document.querySelector('#clear-all-cancel-btn')?.addEventListener('click', () => {
      clearAllModal.style.display = 'none';
      pendingConfirmResolve?.(false);
      pendingConfirmResolve = null;
    });
    document.querySelector('#clear-all-confirm-btn')?.addEventListener('click', () => {
      clearAllModal.style.display = 'none';
      pendingConfirmResolve?.('confirm');
      pendingConfirmResolve = null;
    });
    document.querySelector('#clear-all-save-first-btn')?.addEventListener('click', async () => {
      // Saves the whole current queue (not just whatever's about to be
      // removed) — the point is that everything at risk is still
      // recoverable from the saved file afterward, whether this is a
      // single tracked delete, Clear Selected, or Clear All.
      const saved = await saveProjectSession();
      if (!saved) return; // leave the modal open — nothing was lost yet
      clearAllModal.style.display = 'none';
      pendingConfirmResolve?.('save-first');
      pendingConfirmResolve = null;
    });
  }

  // Clear Selected — removes checked rows regardless of status, same in
  // both modes (the user explicitly checked them). Whenever any checked
  // demo is tracked, escalate from a plain confirm() to the shared modal.
  // Scoped to visible rows too: a row checked, then hidden by a later
  // search, is left in the queue AND stays checked — the action never saw
  // it, so it shouldn't lose that selection just because clearCheckedPaths()
  // (inside replaceScannedDemos) resets everything by default.
  const clearSelectedBtn = document.querySelector('#clear-selected-btn');
  if (clearSelectedBtn) {
    clearSelectedBtn.addEventListener('click', async () => {
      const checkedPaths = new Set(getCheckedDemoPaths());
      if (checkedPaths.size === 0) {
        showToast('No demos selected — check rows in the queue first.', 'info');
        return;
      }
      const visiblePaths = new Set(getVisibleDemos().map((d) => d.path));
      const targets = currentScannedDemos.filter(d => checkedPaths.has(d.path) && visiblePaths.has(d.path));
      if (targets.length === 0) {
        showToast(`All ${checkedPaths.size} selected demo(s) are hidden by the current search — nothing visible to remove.`, 'info');
        return;
      }
      const hiddenCheckedCount = checkedPaths.size - targets.length;
      const hiddenNote = hiddenCheckedCount > 0
        ? ` (${hiddenCheckedCount} other selected demo(s) hidden by the search filter were left untouched)`
        : '';
      let savedFirst = false;
      if (targets.some(isDemoTracked)) {
        const outcome = await requestTrackedClearConfirmation(targets, { title: 'Clear Selected Demos', verb: 'removes', filterNote: hiddenNote, confirmLabel: 'Clear Selected Anyway' });
        if (!outcome) return;
        savedFirst = outcome === 'save-first';
      } else if (!(await confirm(`Remove ${targets.length} selected demo(s) from the queue?${hiddenNote}`))) {
        return;
      }
      const removePaths = new Set(targets.map((d) => d.path));
      const removedNames = targets.map((d) => d.name || d.path);
      // Preserve checkboxes on rows the action never touched (checked, but
      // hidden by the search filter) — captured before replaceScannedDemos
      // wipes the whole checked set via clearCheckedPaths().
      const survivingHiddenChecked = Array.from(checkedPaths).filter((p) => !removePaths.has(p));
      replaceScannedDemos(currentScannedDemos.filter(d => !removePaths.has(d.path)));
      if (survivingHiddenChecked.length > 0) setCheckedDemoPaths(survivingHiddenChecked);
      showToast(`${savedFirst ? 'Saved, then removed' : 'Removed'} ${targets.length} demo(s) from the queue.${hiddenNote}`, 'success');
      logFrontendEvent(`[queue] Clear Selected: removed ${targets.length} demo(s)${savedFirst ? ' (saved session first)' : ''}.${hiddenNote} — ${removedNames.join(', ')}`);
    });
  }

  // Clear All — escalates to the shared modal (enumerating what would be
  // lost, offering to save first) whenever something tracked is actually at
  // risk, same threshold as Clear Selected/row delete — in *either* mode.
  // Quick-Clip only ever meant "nothing persists to disk automatically," not
  // "no warning before losing work you set five seconds ago," so this no
  // longer branches on studioMode at all. Also scoped to the search filter,
  // same as the other two Clear actions — "All" means "all visible," with
  // an explicit callout whenever that's fewer than the full queue, so it
  // never silently does less than its name implies.
  const clearAllBtn = document.querySelector('#clear-all-btn');
  if (clearAllBtn) {
    clearAllBtn.addEventListener('click', async () => {
      if (currentScannedDemos.length === 0) {
        showToast('Queue is already empty.', 'info');
        return;
      }
      const targets = getVisibleDemos();
      if (targets.length === 0) {
        showToast('No demos match the current search.', 'info');
        return;
      }
      const note = filterScopeNote(targets.length, currentScannedDemos.length);
      let savedFirst = false;
      if (targets.some(isDemoTracked)) {
        const outcome = await requestTrackedClearConfirmation(targets, { title: 'Clear All Demos', verb: 'removes', filterNote: note, confirmLabel: 'Clear All Anyway' });
        if (!outcome) return;
        savedFirst = outcome === 'save-first';
      } else if (!(await confirm(`Remove ${targets.length} demo(s) from the queue? None have tracked work on them.${note}`))) {
        return;
      }
      const removePaths = new Set(targets.map((d) => d.path));
      const removedNames = targets.map((d) => d.name || d.path);
      replaceScannedDemos(currentScannedDemos.filter((d) => !removePaths.has(d.path)));
      showToast(`${savedFirst ? 'Saved, then cleared' : 'Cleared'} ${targets.length} demo(s) from the queue.${note}`, 'success');
      logFrontendEvent(`[queue] Clear All: removed ${targets.length} demo(s)${savedFirst ? ' (saved session first)' : ''}.${note} — ${removedNames.join(', ')}`);
    });
  }

  // Single-row tracked delete (master_pane.js's 🗑 button) reuses the same
  // modal via this thin wrapper, so a tracked demo gets the exact same
  // Save-First affordance as Clear Selected/All instead of a lesser plain
  // confirm() just because it's one row. Returns whether the caller should
  // proceed — master_pane.js still owns the actual splice + selection-shift
  // logic, since that's specific to a single-row delete.
  async function requestTrackedDeleteConfirm(demo) {
    const outcome = await requestTrackedClearConfirmation([demo], { title: 'Remove Tracked Demo', verb: 'removes', confirmLabel: 'Remove Anyway' });
    return !!outcome;
  }

  initMasterPane(onDeleteDemo, requestTrackedDeleteConfirm);
  initDetailPane(() => currentScannedDemos, () => {
    // Fired on both streak selection and status edits — selection moves
    // required capture bytes (refreshLaunchGuard) and both move the Master
    // Queue's Highlights/Pending/Captured/Rendered columns (renderMasterList).
    refreshLaunchGuard({ targetDrives, currentScannedDemos });
    renderMasterList(currentScannedDemos, selectedDemoIdx);
  });

  // Demo Analyzer's Explorer sidebar Pinned tier shares the same
  // pinned_folders/scanPaths state as Capture Studio (matches dev's real
  // design — its own pinned_folders field is app-wide, not analyzer-scoped).
  // Pinning/unpinning here doesn't trigger Capture Studio's heavier
  // highlight-scan pipeline, just persists the path.
  async function pinAnalyzerFolder(folder) {
    if (!scanPaths.includes(folder)) {
      scanPaths.push(folder);
      await persistAppSettings();
    }
  }
  async function unpinAnalyzerFolder(folder) {
    scanPaths = scanPaths.filter((f) => f !== folder);
    await persistAppSettings();
  }
  // Recent tier: most-recent-first, capped at 10, deduped — mirrors dev's
  // `demo_folder_history` push-front/truncate logic exactly.
  async function recordDemoFolderVisit(folder) {
    demoFolderHistory = demoFolderHistory.filter((f) => f !== folder);
    demoFolderHistory.unshift(folder);
    if (demoFolderHistory.length > 10) demoFolderHistory.length = 10;
    await persistAppSettings();
  }
  // A pinned/recent folder that no longer exists on disk is silently
  // dropped from history when clicked, matching dev's Quick Links behavior.
  async function forgetDemoFolderVisit(folder) {
    if (demoFolderHistory.includes(folder)) {
      demoFolderHistory = demoFolderHistory.filter((f) => f !== folder);
      await persistAppSettings();
    }
  }
  async function setScanFoldersForDemos(enabled) {
    scanFoldersForDemos = enabled;
    await persistAppSettings();
  }
  async function setAnalyzerExplorerWidth(px) {
    analyzerExplorerWidth = px;
    await persistAppSettings();
  }
  initAnalyzerPane({
    getPinnedFolders: () => scanPaths,
    pinFolder: pinAnalyzerFolder,
    unpinFolder: unpinAnalyzerFolder,
    getDemoFolderHistory: () => demoFolderHistory,
    recordDemoFolderVisit,
    forgetDemoFolderVisit,
    getScanFoldersForDemos: () => scanFoldersForDemos,
    setScanFoldersForDemos,
    getAnalyzerExplorerWidth: () => analyzerExplorerWidth,
    setAnalyzerExplorerWidth,
  });

  // Context-Aware Shortcut Dispatcher
  window.addEventListener('keydown', (e) => {
    const activeTab = document.querySelector('.nav-tab-btn.active')?.dataset.nav;
    const isCtrlO = (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'o';
    const isCtrlS = (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's';
    const isCtrlN = (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n';
    const isCtrlW = (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'w';

    if (isCtrlO || isCtrlS || isCtrlN || isCtrlW) {
      e.preventDefault();
    }

    if (activeTab === 'workspace') {
      // Ctrl+O always opens a project session file (Ctrl+S always saves one,
      // unconditionally, below) — it previously depended on which Capture
      // Studio sub-tab was active and opened the Add Demo Files picker
      // instead when on Highlights, which doesn't match Ctrl+O's meaning
      // in every other app.
      if (isCtrlO) document.querySelector('#load-project-btn')?.click();
      if (isCtrlS) document.querySelector('#save-project-btn')?.click();
    }
  });
});