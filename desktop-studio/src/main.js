import { open, save } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { invoke } from '@tauri-apps/api/core';

window.addEventListener("DOMContentLoaded", () => {
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
            renderMasterList(currentScannedDemos);
            if (currentScannedDemos.length > 0) {
              renderDetailView(currentScannedDemos[0]);
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

  // Render Master List Table (Top Pane)
  function renderMasterList(demos) {
    const tableBody = document.querySelector('#master-demo-table-body');
    tableBody.innerHTML = '';

    if (!demos || demos.length === 0) {
      tableBody.innerHTML = '<tr><td colspan="6" style="padding: 12px; text-align: center; color: #888;">No demos found in specified directories.</td></tr>';
      return;
    }

    demos.forEach((demo, idx) => {
      const tr = document.createElement('tr');
      tr.style.borderBottom = '1px solid #333';
      tr.style.cursor = 'pointer';
      if (selectedDemoIdx === idx) {
        tr.style.background = 'rgba(255, 255, 255, 0.1)';
      }

      tr.innerHTML = `
        <td style="padding: 8px; font-weight: bold;">${demo.name}</td>
        <td style="padding: 8px; font-family: monospace; font-size: 0.85em; color: #aaa;">${demo.path}</td>
        <td style="padding: 8px;">${demo.tickrate || 100} Hz</td>
        <td style="padding: 8px;">${demo.is_pov ? 'POV' : 'HLTV / STV'}</td>
        <td style="padding: 8px;">${demo.streaks ? demo.streaks.length : 0} Streaks</td>
        <td style="padding: 8px;"><span style="color: #4caf50;">Pending</span></td>
      `;

      tr.addEventListener('click', () => {
        selectedDemoIdx = idx;
        renderMasterList(currentScannedDemos);
        renderDetailView(demo);
      });

      tableBody.appendChild(tr);
    });
  }

  // Render Detail View (Bottom Pane)
  function renderDetailView(demo) {
    const titleEl = document.querySelector('#detail-demo-title');
    const container = document.querySelector('#detail-streaks-container');
    const hideNonPov = document.querySelector('#config-hide-non-pov').checked;

    titleEl.textContent = `Highlight Details: ${demo.name}`;
    container.innerHTML = '';

    if (!demo.streaks || demo.streaks.length === 0) {
      container.innerHTML = '<p style="color: #888;">No killstreak highlights detected in this demo.</p>';
      return;
    }

    demo.streaks.forEach((streak, streakIdx) => {
      if (hideNonPov && !demo.is_pov && streak.player_index !== demo.local_player_index) {
        return;
      }

      const card = document.createElement('div');
      card.style.border = '1px solid #444';
      card.style.borderRadius = '4px';
      card.style.padding = '8px 12px';
      card.style.marginBottom = '8px';
      card.style.background = 'rgba(255, 255, 255, 0.05)';
      card.style.display = 'flex';
      card.style.alignItems = 'center';
      card.style.gap = '12px';

      const checkbox = document.createElement('input');
      checkbox.type = 'checkbox';
      checkbox.checked = true;
      checkbox.dataset.demoIdx = selectedDemoIdx;
      checkbox.dataset.streakIdx = streakIdx;

      const label = document.createElement('label');
      label.style.flex = '1';
      label.innerHTML = `<strong>${streak.kill_count} Kills</strong> (${streak.target_player || 'Player ' + streak.player_index}) &nbsp;|&nbsp; <em>${streak.timeline_string}</em> &nbsp;|&nbsp; <span style="font-family: monospace; color: #888;">Ticks: ${streak.start_tick} - ${streak.end_tick}</span>`;

      card.appendChild(checkbox);
      card.appendChild(label);
      container.appendChild(card);
    });
  }

  scanBtn.addEventListener('click', () => {
    if (scanPaths.length === 0) {
      console.warn("Please add at least one folder path.");
      return;
    }
    
    scanBtn.disabled = true;
    scanStatusEl.textContent = "Status: Scanning directories...";
    
    invoke("scan_directory", { paths: scanPaths })
      .then((demos) => {
        currentScannedDemos = demos;
        scanStatusEl.textContent = `Status: Scan complete (${demos.length} demos found)`;
        selectedDemoIdx = demos.length > 0 ? 0 : null;
        renderMasterList(demos);
        if (demos.length > 0) {
          renderDetailView(demos[0]);
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
      renderDetailView(currentScannedDemos[selectedDemoIdx]);
    }
  });

  // Capture Batch Control
  let statusInterval = null;
  const startBtn = document.querySelector('#start-capture-btn') || document.querySelector('#start-batch-btn');
  const cancelBtn = document.querySelector('#cancel-batch-btn');
  const statusEl = document.querySelector('#batch-status');

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

  function startStatusPolling() {
    if (statusInterval) clearInterval(statusInterval);
    statusInterval = setInterval(async () => {
      try {
        const isRunning = await invoke("capture_status");
        if (isRunning) {
          statusEl.textContent = "Status: Capturing batch in progress...";
          updateRowBadges("Capturing...", "#ff9800");
        } else {
          clearInterval(statusInterval);
          statusInterval = null;
          statusEl.textContent = "Status: Batch completed";
          updateRowBadges("Completed", "#4caf50");
          if (startBtn) startBtn.disabled = false;
          if (cancelBtn) cancelBtn.disabled = true;
        }
      } catch (err) {
        console.error("Error polling capture status:", err);
      }
    }, 500);
  }

  if (startBtn) {
    startBtn.addEventListener('click', () => {
      statusEl.textContent = "Status: Initializing capture batch...";
      startBtn.disabled = true;
      if (cancelBtn) cancelBtn.disabled = false;
      updateRowBadges("Queued", "#2196f3");
      
      const selectedStreaks = [];
      const checkboxes = document.querySelectorAll('#detail-streaks-container input[type="checkbox"]:checked');
      checkboxes.forEach(cb => {
        const dIdx = cb.dataset.demoIdx;
        const sIdx = cb.dataset.streakIdx;
        if (currentScannedDemos[dIdx] && currentScannedDemos[dIdx].streaks[sIdx]) {
          selectedStreaks.push(currentScannedDemos[dIdx].streaks[sIdx]);
        }
      });
      
      const captureFpsVal = parseInt(document.querySelector("#config-capture-fps").value, 10) || 60;
      const expectedFpsVal = parseFloat(document.querySelector("#config-expected-fps").value) || 100.0;
      const preRollVal = parseFloat(document.querySelector("#config-pre-roll").value) || 3.0;
      const postRollVal = parseFloat(document.querySelector("#config-post-roll").value) || 2.0;
      const allocationStrategyVal = document.getElementById('allocation-strategy').value;

      const activePayload = {
        hlae_path: "C:\\dummy\\hlae.exe",
        game_path: "C:\\dummy\\hl.exe",
        streaks: selectedStreaks,
        pre_roll_seconds: preRollVal,
        post_roll_seconds: postRollVal,
        capture_directories: scanPaths,
        capture_fps: captureFpsVal,
        expected_fps: expectedFpsVal,
        drives: targetDrives,
        allocation_strategy: allocationStrategyVal
      };

      invoke("start_capture_batch", { payload: activePayload })
        .then(() => {
          statusEl.textContent = "Status: Batch queued successfully!";
          startStatusPolling();
        })
        .catch((err) => {
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
      invoke("cancel_capture_batch")
        .then(() => {
          updateRowBadges("Cancelled", "#f44336");
        })
        .catch(err => console.error("Error cancelling batch:", err));
    });
  }

  // Render Studio Batch Controls
  let renderStatusInterval = null;
  const startRenderBtn = document.querySelector('#start-render-btn');
  const cancelRenderBtn = document.querySelector('#cancel-render-btn');
  const renderStatusEl = document.querySelector('#render-status');

  if (startRenderBtn) {
    startRenderBtn.addEventListener('click', () => {
      renderStatusEl.textContent = "Status: Initializing render batch...";
      startRenderBtn.disabled = true;
      if (cancelRenderBtn) cancelRenderBtn.disabled = false;

      const renderPayload = {
        render_directories: renderFolders,
        output_format: "mp4",
        crf: 18,
        preset: "medium"
      };

      invoke("execute_render_batch", { payload: renderPayload })
        .then(() => {
          renderStatusEl.textContent = "Status: Render batch queued successfully!";
          if (renderStatusInterval) clearInterval(renderStatusInterval);
          renderStatusInterval = setInterval(async () => {
            try {
              const statusText = await invoke("render_status");
              if (statusText.startsWith("Rendering") || statusText.startsWith("Scanning")) {
                renderStatusEl.textContent = `Status: ${statusText}`;
              } else {
                clearInterval(renderStatusInterval);
                renderStatusInterval = null;
                renderStatusEl.textContent = `Status: ${statusText}`;
                if (startRenderBtn) startRenderBtn.disabled = false;
                if (cancelRenderBtn) cancelRenderBtn.disabled = true;
              }
            } catch (err) {
              console.error("Error polling render status:", err);
            }
          }, 500);
        })
        .catch((err) => {
          renderStatusEl.textContent = "Error starting render: " + err;
          if (startRenderBtn) startRenderBtn.disabled = false;
          if (cancelRenderBtn) cancelRenderBtn.disabled = true;
        });
    });
  }

  if (cancelRenderBtn) {
    cancelRenderBtn.addEventListener('click', () => {
      renderStatusEl.textContent = "Status: Cancelling render batch...";
      cancelRenderBtn.disabled = true;
      invoke("cancel_render_batch")
        .catch(err => console.error("Error cancelling render batch:", err));
    });
  }
});