import { open, save } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import {
  scanDirectory,
  calculateExportPoolSpace
} from './ipc_bridge.js';
import { renderMasterList, initMasterPane } from './master_pane.js';
import { renderDetailView } from './detail_pane.js';
import { initCaptureUI } from './capture_pane.js';
import { initRenderUI } from './render_pane.js';
import { initTelemetryPane, renderTelemetry } from './telemetry_pane.js';
import { analyzeDemo } from './ipc_bridge.js';

window.addEventListener("DOMContentLoaded", async () => {
  let scanPaths = [];
  let targetDrives = [];
  let renderFolders = [];
  let currentScannedDemos = [];
  let selectedDemoIdx = null;

  // Save Project Session
  const saveProjectBtn = document.querySelector('#save-project-btn');
  if (saveProjectBtn) {
    saveProjectBtn.addEventListener('click', async () => {
      if (currentScannedDemos.length === 0) {
        console.warn("No scanned demo state to save.");
        return;
      }
      try {
        const filePath = await save({
          title: 'Save Studio Project Session',
          defaultPath: 'dod_project.json',
          filters: [{ name: 'JSON Project File', extensions: ['json'] }]
        });
        if (filePath) {
          const hlaePath = document.querySelector('#hlae-path-input')?.value || "";
          const hlPath = document.querySelector('#hl-path-input')?.value || "";
          const projectData = JSON.stringify({
            version: "0.10.0",
            scanPaths: scanPaths,
            demos: currentScannedDemos,
            hlaePath: hlaePath,
            hlPath: hlPath
          }, null, 2);
          await writeTextFile(filePath, projectData);
          document.querySelector('#scan-status').textContent = `Status: Project session saved successfully to ${filePath}`;
        }
      } catch (err) {
        console.error("Save project error:", err);
      }
    });
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
          const content = await readTextFile(selected);
          const data = JSON.parse(content);
          if (data) {
            if (data.hlaePath) {
              const hlaeInput = document.querySelector('#hlae-path-input');
              if (hlaeInput) hlaeInput.value = data.hlaePath;
            }
            if (data.hlPath) {
              const hlInput = document.querySelector('#hl-path-input');
              if (hlInput) hlInput.value = data.hlPath;
            }
            if (data.demos) {
              currentScannedDemos = data.demos;
              selectedDemoIdx = currentScannedDemos.length > 0 ? 0 : null;
              renderMasterList(currentScannedDemos, selectedDemoIdx, (demo, idx) => {
                selectedDemoIdx = idx;
                renderDetailView(demo, selectedDemoIdx);
              });
              if (currentScannedDemos.length > 0) {
                renderDetailView(currentScannedDemos[0], selectedDemoIdx);
              }
              document.querySelector('#scan-status').textContent = `Status: Loaded ${currentScannedDemos.length} demos from project file`;
            }
          }
        }
      } catch (err) {
        console.error("Load project error:", err);
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
        }
      } catch (err) {
        console.error("Error selecting FFmpeg executable:", err);
      }
    });
  }

  const primaryMediaBrowseBtn = document.querySelector('#primary-media-browse-btn');
  if (primaryMediaBrowseBtn) {
    primaryMediaBrowseBtn.addEventListener('click', async () => {
      try {
        const selected = await open({
          directory: true,
          multiple: false,
          title: 'Select Primary Media Directory'
        });
        if (selected) {
          const path = Array.isArray(selected) ? selected[0] : selected;
          const inputEl = document.querySelector('#primary-media-dir-input');
          if (inputEl) inputEl.value = path;
        }
      } catch (err) {
        console.error("Error selecting primary media directory:", err);
      }
    });
  }

  const backupMediaBrowseBtn = document.querySelector('#backup-media-browse-btn');
  if (backupMediaBrowseBtn) {
    backupMediaBrowseBtn.addEventListener('click', async () => {
      try {
        const selected = await open({
          directory: true,
          multiple: false,
          title: 'Select Backup Media Directory'
        });
        if (selected) {
          const path = Array.isArray(selected) ? selected[0] : selected;
          const inputEl = document.querySelector('#backup-media-dir-input');
          if (inputEl) inputEl.value = path;
        }
      } catch (err) {
        console.error("Error selecting backup media directory:", err);
      }
    });
  }

  // Auto-scan helper function
  async function triggerAutoScan() {
    if (scanPaths.length === 0) return;

    const scanStatusEl = document.querySelector('#scan-status');
    const addFilesBtn = document.querySelector('#add-files-btn');
    const addFolderBtn = document.querySelector('#add-folder-btn');

    if (addFilesBtn) addFilesBtn.disabled = true;
    if (addFolderBtn) addFolderBtn.disabled = true;
    if (scanStatusEl) scanStatusEl.textContent = "Status: Scanning directories...";
    
    const masterTableBody = document.querySelector('#master-demo-table-body');
    if (masterTableBody) masterTableBody.innerHTML = '<tr style="text-align:center"><td colspan="6">Loading... scanning directories...</td></tr>';

    try {
      const demos = await scanDirectory(scanPaths);
      currentScannedDemos = demos;
      if (scanStatusEl) scanStatusEl.textContent = `Status: Scan complete (${demos.length} demos found)`;
      selectedDemoIdx = demos.length > 0 ? 0 : null;
      renderMasterList(demos, selectedDemoIdx, async (demo, idx) => {
        selectedDemoIdx = idx;
        renderDetailView(demo, selectedDemoIdx);
        try {
          const telemetryData = await analyzeDemo(demo.path);
          renderTelemetry(telemetryData);
        } catch (e) {
          console.error("Analysis failed for", demo.path, e);
          renderTelemetry(null);
        }
      });
      if (demos.length > 0) {
        renderDetailView(demos[0], selectedDemoIdx);
        analyzeDemo(demos[0].path).then(renderTelemetry).catch(() => renderTelemetry(null));
      }
    } catch (err) {
      console.error("Error scanning directories:", err);
      if (scanStatusEl) scanStatusEl.textContent = "Error: " + err;
    } finally {
      if (addFilesBtn) addFilesBtn.disabled = false;
      if (addFolderBtn) addFolderBtn.disabled = false;
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
          await triggerAutoScan();
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
            await triggerAutoScan();
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
      indicator.textContent = "Total Export Pool Free: 0.0 GB";
      return;
    }
    try {
      const bytes = await calculateExportPoolSpace(targetDrives);
      const gb = bytes / (1024 * 1024 * 1024);
      indicator.textContent = `Total Export Pool Free: ${gb.toFixed(1)} GB`;
    } catch (err) {
      console.error("Error calculating export pool space:", err);
      indicator.textContent = "Total Export Pool Free: Error calculating space";
    }
  }

  // Target drives management
  document.querySelector('#add-drive-btn').addEventListener('click', () => {
    const driveEl = document.querySelector('#drive-path-input');
    const drivePath = driveEl.value.trim();
    if (drivePath && !targetDrives.includes(drivePath)) {
      targetDrives.push(drivePath);
      driveEl.value = "";
      const li = document.createElement('li');
      li.textContent = drivePath;
      document.querySelector('#target-drive-list').appendChild(li);
      updateExportPoolIndicator();
    }
  });

  const browseDriveBtn = document.querySelector('#browse-drive-btn');
  if (browseDriveBtn) {
    browseDriveBtn.addEventListener('click', async () => {
      try {
        const selected = await open({
          directory: true,
          multiple: false,
          title: 'Select Target Output Drive/Directory'
        });
        if (selected && !targetDrives.includes(selected)) {
          targetDrives.push(selected);
          const li = document.createElement('li');
          li.textContent = selected;
          document.querySelector('#target-drive-list').appendChild(li);
          updateExportPoolIndicator();
        }
      } catch (err) {
        console.error("Error opening target drive dialog:", err);
      }
    });
  }

  // Render folders management
  document.querySelector('#add-render-folder-btn').addEventListener('click', () => {
    const inputEl = document.querySelector('#render-path-input');
    const path = inputEl.value.trim();
    if (path && !renderFolders.includes(path)) {
      renderFolders.push(path);
      inputEl.value = "";
      const li = document.createElement('li');
      li.textContent = path;
      document.querySelector('#render-folder-list').appendChild(li);
    }
  });

  const browseRenderFolderBtn = document.querySelector('#browse-render-folder-btn');
  if (browseRenderFolderBtn) {
    browseRenderFolderBtn.addEventListener('click', async () => {
      try {
        const selected = await open({
          directory: true,
          multiple: false,
          title: 'Select Render Directory'
        });
        if (selected && !renderFolders.includes(selected)) {
          renderFolders.push(selected);
          const li = document.createElement('li');
          li.textContent = selected;
          document.querySelector('#render-folder-list').appendChild(li);
        }
      } catch (err) {
        console.error("Error opening render directory dialog:", err);
      }
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

  // Wizard Navigation: top nav bar view routing
  function activateWizardStep(navKey) {
    const workspacePane = document.querySelector('#pane-workspace');
    const detailsPane = document.querySelector('#pane-details-config');
    const detailPane = document.querySelector('#detail-pane');
    const advancedPanel = document.querySelector('#advanced-diagnostics-details');
    const exportPanel = document.querySelector('#export-config-panel');
    const renderPanel = document.querySelector('#render-studio-panel');

    // Strict display override to hide all panels initially
    if (workspacePane) workspacePane.style.display = 'none';
    if (detailsPane) detailsPane.style.display = 'none';
    if (detailPane) detailPane.style.display = 'none';
    if (advancedPanel) advancedPanel.style.display = 'none';
    if (exportPanel) exportPanel.style.display = 'none';
    if (renderPanel) renderPanel.style.display = 'none';

    // Selectively display panels based on wizard step
    if (navKey === 'workspace') {
      if (workspacePane) workspacePane.style.display = 'flex';
      if (detailsPane) detailsPane.style.display = 'flex';
      if (detailPane) detailPane.style.display = 'block';
      if (advancedPanel) advancedPanel.style.display = 'block';
    } else if (navKey === 'export-config') {
      if (detailsPane) detailsPane.style.display = 'flex';
      if (exportPanel) exportPanel.style.display = 'block';
    } else if (navKey === 'render-studio') {
      if (detailsPane) detailsPane.style.display = 'flex';
      if (renderPanel) renderPanel.style.display = 'block';
    }
  }

  // Set initial wizard state
  activateWizardStep('workspace');

  const navTabBtns = document.querySelectorAll('.nav-tab-btn');
  navTabBtns.forEach(btn => {
    btn.addEventListener('click', () => {
      navTabBtns.forEach(b => b.classList.remove('active'));
      btn.classList.add('active');
      activateWizardStep(btn.getAttribute('data-nav'));
    });
  });

  // Proceed to Capture button in footer
  const proceedBtn = document.querySelector('#proceed-capture-nav-btn');
  if (proceedBtn) {
    proceedBtn.addEventListener('click', () => {
      const captureNavBtn = document.querySelector('.nav-tab-btn[data-nav="export-config"]');
      navTabBtns.forEach(b => {
        if (b === captureNavBtn) {
          b.classList.add('active');
        } else {
          b.classList.remove('active');
        }
      });
      activateWizardStep('export-config');
    });
  }

  // Initialize Capture Batch UI
  initCaptureUI(() => ({
    scanPaths,
    targetDrives,
    currentScannedDemos
  }));

  // Initialize Render Studio UI
  initRenderUI(() => renderFolders);

  initMasterPane();
  initTelemetryPane();
});