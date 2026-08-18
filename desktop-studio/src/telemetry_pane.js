// telemetry_pane.js
// Renders the compact inline telemetry summary (map/ticks/score/chat count)
// shown in the Workspace detail pane when a demo is selected. The full report
// view (Summary/Scoreboard/Player Details/Team Details/Timeline/Rounds/Chat)
// lives in the standalone Demo Analyzer tab — see analyzer_pane.js.

/**
 * Renders the SerializedAnalysis DTO into the compact inline side-panel.
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

  container.innerHTML = _buildCompactSummary(data);
}

/** HTML-escape a string so user/demo content can't inject markup. */
function _esc(str) {
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function _buildCompactSummary(data) {
  if (!data) return '<p style="color:#888;">No data.</p>';

  const fi = data.file_info || {};
  const map     = fi.map_name || fi.map || '—';
  const ticks   = fi.playback_ticks || fi.total_frames || '—';
  const tickrate = fi.tickrate || '—';

  let scoreLine = '';
  if (Array.isArray(data.scoreboard) && data.scoreboard.length > 0) {
    const allies_score = data.allies_score || 0;
    const axis_score = data.axis_score || 0;
    const cmp = allies_score > axis_score ? '>' : (allies_score === axis_score ? '=' : '<');
    scoreLine = `<span style="color:#aaa;">${data.scoreboard.length} scoreboard entries (${allies_score} ${cmp} ${axis_score})</span>`;
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
