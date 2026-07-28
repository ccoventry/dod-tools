import { loadAndShowTelemetry } from './telemetry_pane.js';

let currentDemo = null;
let currentDemoIdx = null;

// Initialize event listeners for detail pane buttons
window.addEventListener("DOMContentLoaded", () => {
  const btnSelectAll = document.querySelector('#btn-select-all');
  const btnDeselectAll = document.querySelector('#btn-deselect-all');
  const inputMinKills = document.querySelector('#input-min-kills');
  const hideNonPovCheckbox = document.querySelector('#config-hide-non-pov');

  if (btnSelectAll) {
    btnSelectAll.addEventListener('click', () => {
      if (!currentDemo || !currentDemo.streaks) return;
      currentDemo.streaks.forEach(s => { s.selected = true; });
      const checkboxes = document.querySelectorAll('#detail-streaks-container input[type="checkbox"]');
      checkboxes.forEach(cb => { cb.checked = true; });
      renderDetailView(currentDemo, currentDemoIdx);
    });
  }

  if (btnDeselectAll) {
    btnDeselectAll.addEventListener('click', () => {
      if (!currentDemo || !currentDemo.streaks) return;
      currentDemo.streaks.forEach(s => { s.selected = false; });
      const checkboxes = document.querySelectorAll('#detail-streaks-container input[type="checkbox"]');
      checkboxes.forEach(cb => { cb.checked = false; });
      renderDetailView(currentDemo, currentDemoIdx);
    });
  }

  if (inputMinKills) {
    inputMinKills.addEventListener('input', () => {
      renderDetailView(currentDemo, currentDemoIdx);
    });
  }
});

export function renderDetailView(demo, selectedDemoIdx) {
  currentDemo = demo;
  currentDemoIdx = selectedDemoIdx;

  const titleEl = document.querySelector('#detail-demo-title');
  const container = document.querySelector('#detail-streaks-container');
  const telemetryBtn = document.querySelector('#view-telemetry-btn');

  if (telemetryBtn) {
    if (!demo || !demo.path) {
      telemetryBtn.disabled = true;
      telemetryBtn.onclick = null;
    } else {
      telemetryBtn.disabled = false;
      telemetryBtn.onclick = () => {
        loadAndShowTelemetry(demo.path);
      };
    }
  }

  if (!titleEl || !container) return;

  if (!demo) {
    titleEl.textContent = 'Highlight Details (Select a Demo)';
    container.innerHTML = '<p style="color: #888;">Select a demo in the Master List to view its killstreak details.</p>';
    return;
  }

  const minKills = parseInt(document.querySelector('#input-min-kills')?.value || "1", 10);
  titleEl.textContent = `Highlight Details: ${demo.name}`;
  container.innerHTML = '';

  if (!demo.streaks || demo.streaks.length === 0) {
    container.innerHTML = '<p style="color: #888;">No killstreak highlights detected in this demo.</p>';
    return;
  }

  const tableWrapper = document.createElement('div');
  tableWrapper.className = 'table-wrapper';
  const table = document.createElement('table');
  table.id = 'detail-streaks-table';
  table.innerHTML = `
    <thead>
      <tr>
        <th>Row #</th>
        <th>Sel</th>
        <th>Kill Range</th>
        <th>Kills</th>
        <th>Time</th>
        <th>Dur.</th>
        <th>Status</th>
        <th>Notes</th>
        <th>Details</th>
      </tr>
    </thead>
    <tbody></tbody>
  `;
  const tbody = table.querySelector('tbody');

  demo.streaks.forEach((streak, streakIdx) => {
    // 1. POV Filter
    if ((demo.is_pov || demo.recording_player) && streak.player !== demo.recording_player) {
      if (demo.is_pov !== false) { // Assuming if HLTV, we don't skip unless we want strictly recording_player
          // The prompt says: "If demo.pov === true or demo.recording_player exists, filter demo.streaks to only include streaks where streak.player === demo.recording_player. (If HLTV, display all)."
          // But actually, demo might use player_index instead of player name, wait.
      }
    }

    // Checking prompt again: "If demo.pov === true or demo.recording_player exists, filter demo.streaks to only include streaks where streak.player === demo.recording_player. (If HLTV, display all)."
    // Let's implement exact logic:
    const isHLTV = !demo.is_pov && !demo.recording_player;
    if (!isHLTV) {
       // if we have recording_player, we filter. But in `detail_pane.js` it previously used `streak.player_index !== demo.local_player_index`. Let's use `streak.player === demo.recording_player` as requested. If not present, fallback to local_player_index.
       const recPlayer = demo.recording_player || demo.local_player_index;
       const strPlayer = streak.player || streak.player_index;
       if (strPlayer !== recPlayer) return;
    }
    
    // 2. Min Kills filter
    if (streak.kill_count < minKills) {
      return;
    }

    // Opt-In Default
    if (streak.selected === undefined) {
      streak.selected = false;
    }

    const tr = document.createElement('tr');
    tr.style.borderBottom = '1px solid #333';
    
    const rowNum = streakIdx + 1;
    const durTicks = streak.end_tick - streak.start_tick;
    const tickrate = demo.tickrate || 100;
    const durSecs = (durTicks / tickrate).toFixed(1);
    
    // Time logic
    const total_seconds = Math.floor(streak.start_tick / (demo.tickrate || 100));
    const mins = Math.floor(total_seconds / 60);
    const secs = Math.floor(total_seconds % 60).toString().padStart(2, '0');
    const timeStr = `${mins}:${secs}`;
    
    // Kill Range: use the precomputed timeline_string from the backend
    // (e.g. "Rifle (+0:03) Rifle" — first kill weapon + gap + weapon chain).
    const killRange = streak.timeline_string || `${streak.kill_count} kills`;

    // Status badge colours matching HighlightStatus enum
    const statusColors = {
      Pending: '#888',
      Captured: '#4caf50',
      Rendered: '#2196f3',
      None: '#555',
    };
    const statusLabel = streak.status || 'Pending';
    const statusColor = statusColors[statusLabel] || '#888';

    tr.innerHTML = `
      <td style="padding: 8px;">${rowNum}</td>
      <td style="padding: 8px;">
        <input type="checkbox" class="streak-select-cb" ${streak.selected ? 'checked' : ''} />
      </td>
      <td style="padding: 8px; font-size: 0.8em; max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;" title="${killRange}">${killRange}</td>
      <td style="padding: 8px; font-weight: bold;">${streak.kill_count}</td>
      <td style="padding: 8px;">${timeStr}</td>
      <td style="padding: 8px;">${durSecs}s</td>
      <td style="padding: 8px;">
        <span style="color: ${statusColor}; font-size: 0.85em;">${statusLabel}</span>
      </td>
      <td style="padding: 8px;">
        <input type="text" placeholder="Add note..." style="background: #1a1a1a; color: #fff; border: 1px solid #444; border-radius: 3px; padding: 2px; width: 100%;" />
      </td>
      <td style="padding: 8px; font-size: 0.8em;">${(streak.kills || []).map(k => k[2] || 'kill').join(', ')}</td>
    `;

    const cb = tr.querySelector('.streak-select-cb');
    cb.addEventListener('change', (e) => {
      streak.selected = e.target.checked;
      renderTimeline(currentDemo);
    });

    tbody.appendChild(tr);
  });

  tableWrapper.appendChild(table);
  container.appendChild(tableWrapper);

  renderTimeline(demo);
}

export function renderTimeline(demo) {
  const canvas = document.querySelector('#streak-timeline-canvas');
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  if (!ctx) return;

  const width = canvas.clientWidth || 600;
  const height = canvas.clientHeight || 100;
  if (canvas.width !== width) canvas.width = width;
  if (canvas.height !== height) canvas.height = height;

  ctx.fillStyle = '#1e1e1e';
  ctx.fillRect(0, 0, width, height);

  if (!demo || !demo.streaks || demo.streaks.length === 0) {
    ctx.fillStyle = '#666666';
    ctx.font = '12px sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText('No streak timeline available', width / 2, height / 2);
    return;
  }

  const preRollSecs = parseFloat(document.querySelector("#config-pre-roll")?.value) || 2.0;
  const postRollSecs = parseFloat(document.querySelector("#config-post-roll")?.value) || 0.6;
  const tickrate = demo.tickrate || 100;
  const preRollTicks = preRollSecs * tickrate;
  const postRollTicks = postRollSecs * tickrate;

  let minTick = Infinity;
  let maxTick = -Infinity;
  demo.streaks.forEach(s => {
    if (s.start_tick - preRollTicks < minTick) minTick = s.start_tick - preRollTicks;
    if (s.end_tick + postRollTicks > maxTick) maxTick = s.end_tick + postRollTicks;
  });

  if (minTick === Infinity || maxTick === -Infinity || maxTick <= minTick) {
    minTick = 0;
    maxTick = 1000;
  }

  const padding = 20;
  const usableWidth = width - (padding * 2);
  const tickSpan = (maxTick - minTick) || 1;

  // Timeline axis
  ctx.strokeStyle = '#444444';
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(padding, height - 20);
  ctx.lineTo(width - padding, height - 20);
  ctx.stroke();

  ctx.fillStyle = '#888888';
  ctx.font = '10px monospace';
  ctx.textAlign = 'left';
  ctx.fillText(`Tick ${minTick}`, padding, height - 5);
  ctx.textAlign = 'right';
  ctx.fillText(`Tick ${maxTick}`, width - padding, height - 5);

  demo.streaks.forEach((streak) => {
    const isSelected = streak.selected !== false;
    const startX = padding + ((streak.start_tick - minTick) / tickSpan) * usableWidth;
    const endX = padding + ((streak.end_tick - minTick) / tickSpan) * usableWidth;
    const blockWidth = Math.max(endX - startX, 4);

    const preX = padding + (((streak.start_tick - preRollTicks) - minTick) / tickSpan) * usableWidth;
    const preWidth = Math.max(startX - preX, 0);

    const postX = endX;
    const postEndX = padding + (((streak.end_tick + postRollTicks) - minTick) / tickSpan) * usableWidth;
    const postWidth = Math.max(postEndX - postX, 0);

    // Pre-roll margin
    ctx.fillStyle = isSelected ? 'rgba(76, 175, 80, 0.15)' : 'rgba(255, 255, 255, 0.02)';
    ctx.fillRect(preX, 15, preWidth, height - 40);

    // Post-roll margin
    ctx.fillRect(postX, 15, postWidth, height - 40);

    // Core Span block
    ctx.fillStyle = isSelected ? 'rgba(76, 175, 80, 0.35)' : 'rgba(255, 255, 255, 0.05)';
    ctx.fillRect(startX, 15, blockWidth, height - 40);

    ctx.strokeStyle = isSelected ? '#4caf50' : '#444444';
    ctx.lineWidth = 1;
    ctx.strokeRect(startX, 15, blockWidth, height - 40);
    // Draw outer bounds for margins
    ctx.strokeStyle = isSelected ? 'rgba(76, 175, 80, 0.4)' : '#333333';
    ctx.strokeRect(preX, 15, preWidth + blockWidth + postWidth, height - 40);

    // Kill timestamp markers — kills are (tick, abs_time_secs, weapon) tuples
    if (streak.kills && Array.isArray(streak.kills) && streak.kills.length > 0) {
      streak.kills.forEach(k => {
        // k[0] = tick (integer), k[1] = abs_time_secs, k[2] = weapon name
        const kTick = k[0] !== undefined ? k[0] : streak.start_tick;
        const kX = padding + ((kTick - minTick) / tickSpan) * usableWidth;
        ctx.strokeStyle = isSelected ? '#ff4444' : '#773333';
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(kX, 15);
        ctx.lineTo(kX, height - 25);
        ctx.stroke();
      });
    } else {
      ctx.strokeStyle = isSelected ? '#ff9800' : '#664411';
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(startX, 15);
      ctx.lineTo(startX, height - 25);
      ctx.stroke();
    }
  });
}

