// telemetry_pane.js
// Renders match scoreboard, mortality matrix, chat logs, and file info
// into the Telemetry modal.  Called from main.js after analyzeDemo() resolves.

// ── Modal lifecycle ──────────────────────────────────────────────────────────

export function initTelemetryPane() {
  const closeBtn = document.querySelector('#telemetry-modal-close-btn');
  const modal = document.querySelector('#telemetry-modal');
  if (closeBtn && modal) {
    closeBtn.addEventListener('click', () => {
      modal.style.display = 'none';
    });
    // Close on backdrop click (outside modal-content)
    modal.addEventListener('click', (e) => {
      if (e.target === modal) modal.style.display = 'none';
    });
  }

  // Wire the "View Match Telemetry" button in the detail pane header.
  // It is re-bound whenever main.js calls renderDetailView() so we only
  // attach the handler once here using event delegation on the document.
  document.addEventListener('click', (e) => {
    if (e.target && e.target.id === 'view-telemetry-btn') {
      const demoPath = e.target.dataset.demoPath;
      if (demoPath) {
        loadAndShowTelemetry(demoPath);
      }
    }
  });
}

// ── Public: called by main.js after analyzeDemo() returns ───────────────────

/**
 * Renders the SerializedAnalysis DTO into the telemetry modal sections.
 * Also accepts `null` to reset the panel to its empty state.
 */
export function renderTelemetry(data) {
  const container = document.getElementById('telemetry-container');
  if (!container) return;

  if (!data) {
    container.innerHTML =
      '<p style="color: #888; padding: 6px;">Select a demo to view telemetry data.</p>';
    return;
  }

  // Inline panel rendering (not the modal — this is the compact side-pane).
  container.innerHTML = _buildCompactSummary(data);
}

// ── Modal flow ────────────────────────────────────────────────────────────────

export async function loadAndShowTelemetry(demoPath) {
  const { analyzeDemo } = await import('./ipc_bridge.js');

  const modal = document.querySelector('#telemetry-modal');
  const loadingEl = document.querySelector('#telemetry-loading-state');
  const contentContainer = document.querySelector('#telemetry-content-container');

  if (modal) modal.style.display = 'flex';
  if (loadingEl) loadingEl.style.display = 'block';
  if (contentContainer) contentContainer.style.display = 'none';

  try {
    const analysis = await analyzeDemo(demoPath);

    if (loadingEl) loadingEl.style.display = 'none';
    if (contentContainer) contentContainer.style.display = 'block';

    _renderModalSections(analysis);
  } catch (err) {
    console.error('Telemetry analysis error:', err);
    if (loadingEl) loadingEl.style.display = 'none';
    if (contentContainer) {
      contentContainer.style.display = 'block';
      contentContainer.innerHTML = `
        <div style="color: #f44336; padding: 15px; background: #2a1515;
                    border: 1px solid #5a2020; border-radius: 3px;">
          <strong>Analysis failed:</strong> ${_esc(String(err))}
        </div>`;
    }
  }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/** HTML-escape a string so user/demo content can't inject markup. */
function _esc(str) {
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/**
 * Render into the three fixed modal sections (scoreboard / chat / mortality).
 * Sections are identified by the IDs set in index.html.
 */
function _renderModalSections(analysis) {
  _setBox('#telemetry-scoreboard .telemetry-data-box',        analysis?.scoreboard);
  _setBox('#telemetry-chat-logs .telemetry-data-box',         analysis?.chat_logs);
  _setBox('#telemetry-mortality-metrics .telemetry-data-box', analysis?.mortality_metrics);
  _setBox('#telemetry-round-chronologies .telemetry-data-box', analysis?.round_chronologies);

  // File info — inject into modal header sub-line if the element exists.
  const fileInfoEl = document.querySelector('#telemetry-file-info');
  if (fileInfoEl && analysis?.file_info) {
    const fi = analysis.file_info;
    const map   = fi.map_name  || fi.map   || '—';
    const ticks = fi.playback_ticks || fi.total_frames || '—';
    const fps   = fi.tickrate  || '—';
    fileInfoEl.textContent = `Map: ${map}  ·  Ticks: ${ticks}  ·  Tickrate: ${fps}`;
  }
}

/** Write rendered HTML into a CSS-selector-identified box element. */
function _setBox(selector, data) {
  const el = document.querySelector(selector);
  if (!el) return;
  el.innerHTML = _renderSection(data);
}

/**
 * Compact summary rendered into the inline #telemetry-container side pane.
 * Shows file_info + first-pass scoreboard row count only — not the full modal.
 */
function _buildCompactSummary(data) {
  if (!data) return '<p style="color:#888;">No data.</p>';

  const fi = data.file_info || {};
  const map     = fi.map_name || fi.map || '—';
  const ticks   = fi.playback_ticks || fi.total_frames || '—';
  const tickrate = fi.tickrate || '—';

  let scoreLine = '';
  if (Array.isArray(data.scoreboard) && data.scoreboard.length > 0) {
    scoreLine = `<span style="color:#aaa;">${data.scoreboard.length} scoreboard entries</span>`;
  } else if (data.scoreboard && typeof data.scoreboard === 'object') {
    scoreLine = `<span style="color:#aaa;">Scoreboard data present</span>`;
  }

  let chatLine = '';
  if (Array.isArray(data.chat_logs) && data.chat_logs.length > 0) {
    chatLine = `<span style="color:#aaa;">${data.chat_logs.length} chat messages</span>`;
  }

  return `
    <div style="font-size:11px; line-height:1.6;">
      <div><span style="color:#888;">Map:</span> ${_esc(map)}</div>
      <div><span style="color:#888;">Ticks:</span> ${_esc(String(ticks))} @ ${_esc(String(tickrate))}</div>
      ${scoreLine ? `<div>${scoreLine}</div>` : ''}
      ${chatLine  ? `<div>${chatLine}</div>`  : ''}
    </div>`;
}

/**
 * Universal section renderer — dispatches on the shape of the data:
 *   Array<string>           → <ul> list
 *   Array<object>           → <table> with auto-detected columns
 *   object (non-array)      → key/value grid
 *   primitive / null / empty → placeholder text
 */
function _renderSection(data) {
  if (data === null || data === undefined) {
    return '<p style="color:#888; font-style:italic; padding:4px;">No data available.</p>';
  }

  // Null JSON value
  if (typeof data === 'string' || typeof data === 'number' || typeof data === 'boolean') {
    return `<pre style="color:#dcdcaa;font-family:monospace;font-size:12px;margin:0;">${_esc(String(data))}</pre>`;
  }

  if (Array.isArray(data)) {
    if (data.length === 0) {
      return '<p style="color:#888; font-style:italic; padding:4px;">Empty.</p>';
    }
    if (typeof data[0] === 'string') {
      return _renderStringList(data);
    }
    if (typeof data[0] === 'object' && data[0] !== null) {
      return _renderObjectTable(data);
    }
    // Fallback: mixed array
    return `<pre style="color:#dcdcaa;font-family:monospace;font-size:12px;margin:0;">${_esc(JSON.stringify(data, null, 2))}</pre>`;
  }

  if (typeof data === 'object') {
    return _renderKeyValueGrid(data);
  }

  return `<pre style="color:#dcdcaa;font-family:monospace;font-size:12px;margin:0;">${_esc(JSON.stringify(data, null, 2))}</pre>`;
}

function _renderStringList(arr) {
  const items = arr
    .map((s) => `<li style="padding:2px 0;">${_esc(String(s))}</li>`)
    .join('');
  return `<ul style="margin:0;padding-left:18px;font-size:12px;">${items}</ul>`;
}

function _renderObjectTable(arr) {
  // Collect the union of all keys across all rows for robust column detection.
  const keySet = new Set();
  arr.forEach((row) => {
    if (row && typeof row === 'object') Object.keys(row).forEach((k) => keySet.add(k));
  });
  const keys = [...keySet];

  const thead = keys
    .map(
      (k) =>
        `<th style="padding:5px 8px;background:#222;color:#aaa;` +
        `font-weight:600;border-bottom:1px solid #383838;white-space:nowrap;">${_esc(k)}</th>`,
    )
    .join('');

  const tbody = arr
    .map((row) => {
      const cells = keys
        .map((k) => {
          const v = row ? row[k] : undefined;
          const display =
            v === null || v === undefined
              ? ''
              : typeof v === 'object'
              ? JSON.stringify(v)
              : String(v);
          return `<td style="padding:5px 8px;border-bottom:1px solid #282828;">${_esc(display)}</td>`;
        })
        .join('');
      return `<tr style="cursor:default;">${cells}</tr>`;
    })
    .join('');

  return `
    <div style="overflow-x:auto;">
      <table style="width:100%;border-collapse:collapse;text-align:left;font-size:12px;">
        <thead><tr>${thead}</tr></thead>
        <tbody>${tbody}</tbody>
      </table>
    </div>`;
}

function _renderKeyValueGrid(obj) {
  const rows = Object.entries(obj)
    .map(([k, v]) => {
      const display =
        v === null || v === undefined
          ? '<span style="color:#555;">—</span>'
          : typeof v === 'object'
          ? `<pre style="margin:0;font-family:monospace;font-size:11px;color:#dcdcaa;">${_esc(JSON.stringify(v, null, 2))}</pre>`
          : `<span>${_esc(String(v))}</span>`;
      return `
        <tr>
          <td style="padding:4px 8px;color:#888;white-space:nowrap;font-size:11px;
                     border-bottom:1px solid #222;min-width:140px;">${_esc(k)}</td>
          <td style="padding:4px 8px;border-bottom:1px solid #222;font-size:12px;">${display}</td>
        </tr>`;
    })
    .join('');

  return `
    <table style="width:100%;border-collapse:collapse;">
      <tbody>${rows}</tbody>
    </table>`;
}
