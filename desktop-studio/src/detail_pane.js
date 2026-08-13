import { switchNavTab } from './nav.js';
import { loadAnalyzerDemo } from './analyzer_pane.js';
import { launchDemoPreview, generateAllPreviews } from './ipc_bridge.js';
import { showToast } from './toast.js';

let currentDemo = null;
let currentDemoIdx = null;
// Getter supplied by main.js so "Generate All Previews" can aggregate selected
// streaks across every loaded demo, not just the one currently displayed.
let currentGetAllDemos = null;
// Optional callback supplied by main.js — re-runs capture_pane.js's disk
// space launch guard, since toggling a streak's selection changes the
// required-bytes side of that comparison.
let currentOnSelectionChange = null;

export function initDetailPane(getAllDemos, onSelectionChange) {
  currentGetAllDemos = getAllDemos;
  currentOnSelectionChange = onSelectionChange || null;
}

/** Reflects current selection state onto the Launch Preview (per-demo) and
 *  Generate All Previews (global) buttons — disabled whenever there are zero
 *  selected highlights in their respective scope. */
function updatePreviewButtonStates() {
  const launchBtn = document.querySelector('#btn-launch-preview');
  if (launchBtn) {
    const hasLocalSelection = !!(currentDemo && currentDemo.streaks && currentDemo.streaks.some(s => s.selected));
    launchBtn.disabled = !hasLocalSelection;
  }
  const generateAllBtn = document.querySelector('#btn-generate-all-previews');
  if (generateAllBtn) {
    const allDemos = currentGetAllDemos ? currentGetAllDemos() : [];
    const hasGlobalSelection = (allDemos || []).some(d => (d.streaks || []).some(s => s.selected));
    generateAllBtn.disabled = !hasGlobalSelection;
  }
  if (currentOnSelectionChange) currentOnSelectionChange();
}

/**
 * Recomputes start_tick/end_tick/kill_count/duration_string/timeline_string
 * from streak.kills[start_index..=end_index]. Mirrors
 * `CaptureStreak::update_visuals` in native/src/patch/types.rs — must be
 * called after any mutation of start_index/end_index so the display and the
 * eventual capture payload (which is built from these same fields) agree.
 */
function updateStreakVisuals(streak) {
  if (!streak.kills || streak.kills.length === 0) return;

  const end = Math.min(streak.end_index, streak.kills.length - 1);
  const start = Math.min(streak.start_index, end);
  streak.start_index = start;
  streak.end_index = end;

  const slice = streak.kills.slice(start, end + 1);
  streak.start_tick = slice[0][0];
  streak.end_tick = slice[slice.length - 1][0];
  streak.kill_count = slice.length;

  const totalSecs = Math.round(Math.max(slice[slice.length - 1][1] - slice[0][1], 0));
  streak.duration_string = `${Math.floor(totalSecs / 60)}:${String(totalSecs % 60).padStart(2, '0')}`;

  const parts = slice.map(([, absTime, weapon], i) => {
    // Falls back to a labelled placeholder rather than an empty string so an
    // unresolved weapon name (e.g. a missing localization key) can never
    // leave a blank array element — `Array.prototype.join` would otherwise
    // render that as an orphaned leading/embedded separator with no name.
    const weaponClean = String(weapon || '').replace(/^Weapon::/, '').trim() || 'Unknown';
    if (i === 0) return weaponClean;
    const gapSec = Math.round(Math.max(absTime - slice[i - 1][1], 0));
    return `(+${Math.floor(gapSec / 60)}:${String(gapSec % 60).padStart(2, '0')}) ${weaponClean}`;
  });
  streak.timeline_string = parts.join(', ');
}

// Initialize event listeners for detail pane buttons
window.addEventListener("DOMContentLoaded", () => {
  const btnSelectAll = document.querySelector('#btn-select-all');
  const btnDeselectAll = document.querySelector('#btn-deselect-all');
  const inputMinKills = document.querySelector('#input-min-kills');
  const btnLaunchPreview = document.querySelector('#btn-launch-preview');
  const btnGenerateAllPreviews = document.querySelector('#btn-generate-all-previews');

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

  if (btnLaunchPreview) {
    btnLaunchPreview.addEventListener('click', async () => {
      const hlaePath = document.querySelector('#hlae-path-input')?.value?.trim();
      const hlPath = document.querySelector('#hl-path-input')?.value?.trim();
      if (!hlaePath || !hlPath) {
        showToast('Configure the HLAE and Half-Life executable paths in Batch Capture Config before previewing.', 'error');
        return;
      }
      if (!currentDemo || !currentDemo.streaks) return;
      const selected = currentDemo.streaks.filter(s => s.selected);
      if (selected.length === 0) return;

      btnLaunchPreview.disabled = true;
      const originalLabel = btnLaunchPreview.textContent;
      btnLaunchPreview.textContent = 'Launching…';
      try {
        await launchDemoPreview(hlaePath, hlPath, selected);
        showToast('Preview launching in HLAE...', 'info');
      } catch (err) {
        // Already toasted by ipc_bridge.js.
      } finally {
        btnLaunchPreview.textContent = originalLabel;
        updatePreviewButtonStates();
      }
    });
  }

  if (btnGenerateAllPreviews) {
    btnGenerateAllPreviews.addEventListener('click', async () => {
      const hlaePath = document.querySelector('#hlae-path-input')?.value?.trim();
      const hlPath = document.querySelector('#hl-path-input')?.value?.trim();
      if (!hlaePath || !hlPath) {
        showToast('Configure the HLAE and Half-Life executable paths in Batch Capture Config before previewing.', 'error');
        return;
      }
      const allDemos = currentGetAllDemos ? currentGetAllDemos() : [];
      const allSelected = (allDemos || []).flatMap(d => (d.streaks || []).filter(s => s.selected));
      if (allSelected.length === 0) return;

      btnGenerateAllPreviews.disabled = true;
      const originalLabel = btnGenerateAllPreviews.textContent;
      btnGenerateAllPreviews.textContent = 'Generating…';
      try {
        const count = await generateAllPreviews(hlaePath, hlPath, allSelected);
        showToast(`Generated ${count} preview demo(s). Load them manually via HLAE.`, 'success');
      } catch (err) {
        // Already toasted by ipc_bridge.js.
      } finally {
        btnGenerateAllPreviews.textContent = originalLabel;
        updatePreviewButtonStates();
      }
    });
  }
});

export function renderDetailView(demo, selectedDemoIdx) {
  currentDemo = demo;
  currentDemoIdx = selectedDemoIdx;
  updatePreviewButtonStates();

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
        switchNavTab('demo-analyzer');
        loadAnalyzerDemo(demo.path);
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

  // Sequential display numbering is tracked separately from the streak's
  // position in demo.streaks — POV/min-kills filtering below skips entries,
  // and Row # must count only rows actually rendered (matches dev's
  // `filtered_indices` + `row_idx + 1` behavior), not the raw array index.
  let renderedRowNum = 0;

  demo.streaks.forEach((streak, streakIdx) => {
    // 1. POV Filter
    const isHLTV = !demo.is_pov && !demo.recording_player;
    if (!isHLTV) {
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
    if (streak.start_index === undefined) streak.start_index = 0;
    if (streak.end_index === undefined) {
      streak.end_index = Math.max((streak.kills || []).length - 1, 0);
    }

    renderedRowNum++;
    const rowNum = renderedRowNum;

    const tr = document.createElement('tr');
    tr.style.borderBottom = '1px solid #333';

    const durTicks = streak.end_tick - streak.start_tick;
    const tickrate = demo.tickrate || 100;
    const durSecs = (durTicks / tickrate).toFixed(1);

    // Time logic
    const total_seconds = Math.floor(streak.start_tick / (demo.tickrate || 100));
    const mins = Math.floor(total_seconds / 60);
    const secs = Math.floor(total_seconds % 60).toString().padStart(2, '0');
    const timeStr = `${mins}:${secs}`;

    // Details: precomputed weapon/timing chain from the backend
    // (e.g. "Rifle (+0:03) Rifle" — first kill weapon + gap + weapon chain).
    const timelineText = streak.timeline_string || `${streak.kill_count} kills`;

    // Status badge colours matching HighlightStatus enum
    const statusColors = {
      Pending: '#888',
      Captured: '#4caf50',
      Rendered: '#2196f3',
      None: '#555',
    };
    const statusLabel = streak.status || 'Pending';
    const statusColor = statusColors[statusLabel] || '#888';

    const maxKillIdx = Math.max((streak.kills || []).length - 1, 0);
    const isRangeModified = streak.start_index > 0 || streak.end_index < maxKillIdx;

    tr.innerHTML = `
      <td style="padding: 8px;">${rowNum}</td>
      <td style="padding: 8px;">
        <input type="checkbox" class="streak-select-cb" data-index="${streakIdx}" ${streak.selected ? 'checked' : ''} />
      </td>
      <td style="padding: 8px;">
        <div style="display:flex;align-items:center;gap:4px;${isRangeModified ? 'color:#ff9800;' : ''}">
          <input type="number" class="kr-start-input" min="1" max="${maxKillIdx + 1}"
                 value="${streak.start_index + 1}" style="width:38px;background:#1a1a1a;color:inherit;border:1px solid #444;border-radius:2px;" />
          <span>-</span>
          <input type="number" class="kr-end-input" min="1" max="${maxKillIdx + 1}"
                 value="${streak.end_index + 1}" style="width:38px;background:#1a1a1a;color:inherit;border:1px solid #444;border-radius:2px;" />
          ${isRangeModified ? '<button type="button" class="kr-reset-btn" title="Reset to full range" style="background:transparent;border:1px solid #555;border-radius:2px;color:#aaa;cursor:pointer;">↺</button>' : ''}
        </div>
      </td>
      <td style="padding: 8px; font-weight: bold;">${streak.kill_count}</td>
      <td style="padding: 8px;">${timeStr}</td>
      <td style="padding: 8px;">${durSecs}s</td>
      <td style="padding: 8px;">
        <select class="streak-status-select" style="color: ${statusColor}; font-size: 0.85em;">
          ${['None', 'Pending', 'Captured', 'Rendered'].map(s =>
            `<option value="${s}" ${s === statusLabel ? 'selected' : ''}>${s}</option>`
          ).join('')}
        </select>
      </td>
      <td style="padding: 8px;">
        <input type="text" class="streak-notes-input" placeholder="Add note..." value="${(streak.notes || '').replace(/"/g, '&quot;')}" style="background: #1a1a1a; color: #fff; border: 1px solid #444; border-radius: 3px; padding: 2px; width: 100%;" />
      </td>
      <td class="details-cell" title="${timelineText}">${timelineText}</td>
    `;

    const cb = tr.querySelector('.streak-select-cb');
    cb.addEventListener('change', (e) => {
      streak.selected = e.target.checked;
      renderTimeline(currentDemo);
      updatePreviewButtonStates();
    });

    const startInput = tr.querySelector('.kr-start-input');
    const endInput = tr.querySelector('.kr-end-input');
    startInput.addEventListener('change', () => {
      const v = Math.min(Math.max(parseInt(startInput.value, 10) - 1, 0), streak.end_index);
      streak.start_index = Number.isNaN(v) ? 0 : v;
      updateStreakVisuals(streak);
      renderDetailView(currentDemo, currentDemoIdx);
    });
    endInput.addEventListener('change', () => {
      const v = Math.max(Math.min(parseInt(endInput.value, 10) - 1, maxKillIdx), streak.start_index);
      streak.end_index = Number.isNaN(v) ? maxKillIdx : v;
      updateStreakVisuals(streak);
      renderDetailView(currentDemo, currentDemoIdx);
    });

    const resetBtn = tr.querySelector('.kr-reset-btn');
    if (resetBtn) {
      resetBtn.addEventListener('click', () => {
        streak.start_index = 0;
        streak.end_index = maxKillIdx;
        updateStreakVisuals(streak);
        renderDetailView(currentDemo, currentDemoIdx);
      });
    }

    const statusSelect = tr.querySelector('.streak-status-select');
    statusSelect.addEventListener('change', (e) => {
      streak.status = e.target.value;
      statusSelect.style.color = statusColors[e.target.value] || '#888';
    });

    const notesInput = tr.querySelector('.streak-notes-input');
    notesInput.addEventListener('input', (e) => {
      streak.notes = e.target.value;
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

