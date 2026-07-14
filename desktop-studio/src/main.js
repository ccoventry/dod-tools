import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';

window.addEventListener("DOMContentLoaded", () => {
  document.getElementById('select-demo-btn').addEventListener('click', async () => {
    const selected = await open({
      multiple: false,
      filters: [{
        name: 'Half-Life Demo',
        extensions: ['dem']
      }]
    });

    if (selected) {
      document.getElementById('selected-file-path').innerText = selected;
    }
  });

  let responseLogEl = document.querySelector("#response-log");
  document.querySelector("#test-btn").addEventListener("click", async () => {
    responseLogEl.textContent = await invoke("test_bridge", { path: "C:/demos/test.dem" });
  });

  let scanPaths = [];
  let targetDrives = [];
  let renderFolders = [];
  
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

  let currentScannedDemos = [];
  let currentRenderJobs = [];

  document.querySelector('#scan-dir-btn').addEventListener('click', () => {
    if (scanPaths.length === 0) {
      console.warn("Please add at least one folder path.");
      return;
    }
    
    console.log(`Scanning directories:`, scanPaths);
    invoke("scan_directory", { paths: scanPaths })
      .then((demos) => {
        console.log("Scan complete. Serialized demos received:", demos);
        currentScannedDemos = demos;
        
        const container = document.querySelector('#demo-list-container');
        container.innerHTML = '';
        
        demos.forEach((demo, demoIdx) => {
          const demoHeader = document.createElement('h4');
          demoHeader.textContent = `Demo: ${demo.name}`;
          container.appendChild(demoHeader);
          
          demo.streaks.forEach((streak, streakIdx) => {
            const wrapper = document.createElement('div');
            const checkbox = document.createElement('input');
            checkbox.type = 'checkbox';
            checkbox.checked = true;
            checkbox.dataset.demoIdx = demoIdx;
            checkbox.dataset.streakIdx = streakIdx;
            
            const label = document.createElement('label');
            label.textContent = ` ${streak.kill_count} kills by ${streak.target_player || 'Unknown'} (Ticks: ${streak.start_tick}-${streak.end_tick})`;
            
            wrapper.appendChild(checkbox);
            wrapper.appendChild(label);
            container.appendChild(wrapper);
          });
        });
      })
      .catch((err) => {
        console.error("Error scanning directories:", err);
      });
  });

  let statusInterval;
  const startBtn = document.querySelector('#start-batch-btn');
  const cancelBtn = document.querySelector('#cancel-batch-btn');
  const statusEl = document.querySelector('#batch-status');

  const updateStatusText = async () => {
    try {
      const isRunning = await invoke("capture_status");
      statusEl.textContent = "Status: " + (isRunning ? "Executing..." : "Stopped");
    } catch (err) {
      console.error("Error fetching capture status:", err);
    }
  };

  startBtn.addEventListener('click', () => {
    statusEl.textContent = "Status: Executing...";
    startBtn.disabled = true;
    cancelBtn.disabled = false;
    
    const selectedStreaks = [];
    const checkboxes = document.querySelectorAll('#demo-list-container input[type="checkbox"]:checked');
    checkboxes.forEach(cb => {
      const dIdx = cb.dataset.demoIdx;
      const sIdx = cb.dataset.streakIdx;
      selectedStreaks.push(currentScannedDemos[dIdx].streaks[sIdx]);
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
    
    console.log("Starting batch with payload:", activePayload);
    
    statusInterval = setInterval(updateStatusText, 500);

    invoke("start_capture_batch", { payload: activePayload })
      .then(() => {
        statusEl.textContent = "Status: Batch queued successfully!";
      })
      .catch((err) => {
        statusEl.textContent = "Error: " + err;
      })
      .finally(async () => {
        clearInterval(statusInterval);
        startBtn.disabled = false;
        cancelBtn.disabled = true;
        await updateStatusText();
      });
  });

  cancelBtn.addEventListener('click', () => {
    statusEl.textContent = "Status: Cancelling... Waiting for engine loop to terminate";
    cancelBtn.disabled = true;
    invoke("cancel_capture_batch")
      .catch((err) => {
        console.error("Error cancelling batch:", err);
      });
  });

  document.querySelector('#scan-render-btn').addEventListener('click', () => {
    if (renderFolders.length === 0) {
      console.warn("Please add at least one render folder path.");
      return;
    }

    console.log(`Scanning render directories:`, renderFolders);
    const container = document.querySelector('#render-job-container');
    container.innerHTML = 'Scanning...';

    invoke("scan_render_directories", { paths: renderFolders })
      .then((jobs) => {
        console.log("Render scan complete:", jobs);
        currentRenderJobs = jobs;
        container.innerHTML = '';
        if (jobs.length === 0) {
          container.textContent = 'No takes found.';
          return;
        }

        jobs.forEach((job) => {
          const wrapper = document.createElement('div');
          wrapper.style.marginBottom = '8px';
          wrapper.style.padding = '8px';
          wrapper.style.border = '1px solid #555';

          const title = document.createElement('strong');
          title.textContent = `Clip: ${job.base_name} (${job.clip_type})`;
          
          const info = document.createElement('p');
          info.style.margin = '4px 0 0 0';
          info.textContent = `Folder: ${job.take_folder} | Images: ${job.img_folder} | Frames: ${job.frame_count} | Date: ${job.date}`;

          wrapper.appendChild(title);
          wrapper.appendChild(info);
          container.appendChild(wrapper);
        });
      })
      .catch((err) => {
        console.error("Error scanning render directories:", err);
        currentRenderJobs = [];
        container.textContent = "Error: " + err;
      });
  });

  let renderStatusInterval;
  const startRenderBtn = document.querySelector('#start-render-btn');
  const cancelRenderBtn = document.querySelector('#cancel-render-btn');
  const renderStatusEl = document.querySelector('#render-status');

  const updateRenderStatusText = async () => {
    try {
      const statusText = await invoke("render_status");
      renderStatusEl.textContent = "Status: " + statusText;
    } catch (err) {
      console.error("Error fetching render status:", err);
    }
  };

  startRenderBtn.addEventListener('click', () => {
    if (currentRenderJobs.length === 0) {
      console.warn("No render jobs to execute.");
      return;
    }

    renderStatusEl.textContent = "Status: Executing...";
    startRenderBtn.disabled = true;
    cancelRenderBtn.disabled = false;

    console.log("Starting render batch with jobs:", currentRenderJobs);
    renderStatusInterval = setInterval(updateRenderStatusText, 500);

    invoke("execute_render_batch", { jobs: currentRenderJobs })
      .then(() => {
        renderStatusEl.textContent = "Status: Render batch completed successfully!";
      })
      .catch((err) => {
        renderStatusEl.textContent = "Error: " + err;
      })
      .finally(async () => {
        clearInterval(renderStatusInterval);
        startRenderBtn.disabled = false;
        cancelRenderBtn.disabled = true;
        await updateRenderStatusText();
      });
  });

  cancelRenderBtn.addEventListener('click', () => {
    renderStatusEl.textContent = "Status: Cancelling...";
    cancelRenderBtn.disabled = true;
    invoke("cancel_render_batch")
      .catch((err) => {
        console.error("Error cancelling render batch:", err);
      });
  });
});