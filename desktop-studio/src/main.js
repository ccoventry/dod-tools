import { open, save } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import {
  scanDirectory,
  calculateExportPoolSpace
} from './ipc_bridge.js';
import { renderMasterList } from './master_pane.js';
import { renderDetailView } from './detail_pane.js';
import { initCaptureUI } from './capture_pane.js';
import { initRenderUI } from './render_pane.js';

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
          const projectData = JSON.stringify({
            version: "0.10.0",
            scanPaths: scanPaths,
            demos: currentScannedDemos
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
          if (data && data.demos) {
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
      } catch (err) {
        console.error("Load project error:", err);
      }
    });
  }

  // Folder paths management
  document.querySelector('#add-folder-btn').addEventListener('click', () => {
    const inputEl = document.querySelector('#scan-path-input');
    const inputPath = inputEl.value.trim();
    if (inputPath && !scanPaths.includes(inputPath)) {
      scanPaths.push(inputPath);
      inputEl.value = "";
      
      const li = document.createElement('li');
      li.textContent = inputPath;
      document.querySelector('#folder-list').appendChild(li);
    }
  });

  const browseDirBtn = document.querySelector('#browse-dir-btn');
  if (browseDirBtn) {
    browseDirBtn.addEventListener('click', async () => {
      try {
        const selected = await open({
          directory: true,
          multiple: true,
          title: 'Select Demo Directory'
        });
        if (selected) {
          const paths = Array.isArray(selected) ? selected : [selected];
          paths.forEach(p => {
            if (!scanPaths.includes(p)) {
              scanPaths.push(p);
              const li = document.createElement('li');
              li.textContent = p;
              document.querySelector('#folder-list').appendChild(li);
            }
          });
        }
      } catch (err) {
        console.error("Error opening directory dialog:", err);
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

  const scanBtn = document.querySelector('#scan-dir-btn');
  const scanStatusEl = document.querySelector('#scan-status');

  scanBtn.addEventListener('click', () => {
    if (scanPaths.length === 0) {
      console.warn("Please add at least one folder path.");
      return;
    }
    
    scanBtn.disabled = true;
    scanStatusEl.textContent = "Status: Scanning directories...";
    
    scanDirectory(scanPaths)
      .then((demos) => {
        currentScannedDemos = demos;
        scanStatusEl.textContent = `Status: Scan complete (${demos.length} demos found)`;
        selectedDemoIdx = demos.length > 0 ? 0 : null;
        renderMasterList(demos, selectedDemoIdx, (demo, idx) => {
          selectedDemoIdx = idx;
          renderDetailView(demo, selectedDemoIdx);
        });
        if (demos.length > 0) {
          renderDetailView(demos[0], selectedDemoIdx);
        }
      })
      .catch((err) => {
        console.error("Error scanning directories:", err);
        scanStatusEl.textContent = "Error: " + err;
      })
      .finally(() => {
        scanBtn.disabled = false;
      });
  });

  document.querySelector('#config-hide-non-pov').addEventListener('change', () => {
    if (selectedDemoIdx !== null && currentScannedDemos[selectedDemoIdx]) {
      renderDetailView(currentScannedDemos[selectedDemoIdx], selectedDemoIdx);
    }
  });

  // Initialize Capture Batch UI
  initCaptureUI(() => ({
    scanPaths,
    targetDrives,
    currentScannedDemos
  }));

  // Initialize Render Studio UI
  initRenderUI(() => renderFolders);
});