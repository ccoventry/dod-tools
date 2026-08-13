// analyzer_pane.js
// Standalone "Demo Analyzer" tab — a JS port of the egui report_ui views
// (Summary / Scoreboard / Player Details / Team Details / Timeline / Rounds /
// Chat Log) from the `dev` branch. Reads the full analysis::{DemoInfo,
// AnalyzerState} payload from `analyze_demo_full` rather than the flattened
// generic JSON used by the compact inline telemetry summary.

import { open } from '@tauri-apps/plugin-dialog';
import { analyzeDemoFull } from './ipc_bridge.js';

let report = null;
let activeSubTab = 'summary';
let highlightedPlayerId = null; // shared selection: Scoreboard row <-> Player Details dropdown
let selectedPlayerId = null;
let chatFilters = { showMm1: true, showMm2: true, showSystem: true, team: 'All', search: '' };

const TEAM_COLORS = {
  Allies: '#4caf50',
  British: '#daa520',
  Axis: '#e05252',
  Spectators: '#cccc33',
  Unassigned: '#aaaaaa',
};

// ── Small formatting helpers ─────────────────────────────────────────────────

function teamColor(team) { return TEAM_COLORS[team] || '#ffffff'; }

function teamLabel(team, alliesAreBritish) {
  if (team === 'Allies' || team === 'British') return alliesAreBritish ? 'British' : 'Allies';
  if (team === 'Axis') return 'Axis';
  if (team === 'Spectators') return 'Spectators';
  return 'Unassigned';
}

function durSecs(d) { return d ? (d.secs || 0) + (d.nanos || 0) / 1e9 : 0; }

function formatDuration(totalSecs) {
  totalSecs = Math.max(0, Math.floor(totalSecs || 0));
  const h = Math.floor(totalSecs / 3600);
  const m = Math.floor((totalSecs % 3600) / 60);
  const s = totalSecs % 60;
  if (h > 0) return `${h}h ${m}m ${s}s`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

function formatMMSS(totalSecs) {
  totalSecs = Math.max(0, Math.floor(totalSecs || 0));
  const m = Math.floor(totalSecs / 60);
  const s = totalSecs % 60;
  return `${m}:${String(s).padStart(2, '0')}`;
}

function formatGameTime(totalSecs) {
  totalSecs = Math.max(0, totalSecs || 0);
  const m = Math.floor(totalSecs / 60);
  const s = Math.floor(totalSecs % 60);
  const cc = Math.floor((totalSecs - Math.floor(totalSecs)) * 100);
  return `${m}:${String(s).padStart(2, '0')}:${String(cc).padStart(2, '0')}`;
}

// Simplified stand-in for the egui app's two-tier localization lookup: space
// out the Rust enum variant name (e.g. "ScopedK98" -> "Scoped K98").
function weaponName(w) {
  if (!w) return 'Unknown';
  return String(w).replace(/([a-z0-9])([A-Z])/g, '$1 $2');
}

function esc(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

// SteamID64 -> classic STEAM_0:X:YYYY. Falls back to the raw id for
// non-numeric PlayerGlobalId values (e.g. "PLAYER_<fid>").
function steamIdDisplay(id) {
  if (!id || !/^\d{15,20}$/.test(id)) return id || '—';
  try {
    const big = BigInt(id);
    const base = 76561197960265728n;
    if (big < base) return id;
    const accountId = big - base;
    return `STEAM_0:${accountId % 2n}:${accountId / 2n}`;
  } catch {
    return id;
  }
}

function isConnected(player) {
  return player.connection && typeof player.connection === 'object' && 'Connected' in player.connection;
}

function playerClientId(player) {
  return isConnected(player) ? player.connection.Connected.client_id : null;
}

function getTeamScore(matchFn) {
  const timeline = (report.state.team_scores && report.state.team_scores.timeline) || [];
  for (let i = timeline.length - 1; i >= 0; i--) {
    if (matchFn(timeline[i][1])) return timeline[i][2];
  }
  return 0;
}

function mortalityLifespans(mortality) {
  if (!mortality || mortality.length === 0) return [];
  const spans = [];
  let aliveAt = null;
  for (const change of mortality) {
    const [time, state] = change;
    const secs = durSecs(time.viewdemo_offset);
    if (state === 'Alive') aliveAt = secs;
    else if (state === 'Dead' && aliveAt !== null) {
      if (secs >= aliveAt) spans.push(secs - aliveAt);
      aliveAt = null;
    }
  }
  return spans;
}

function groupConsecutiveWeapons(names) {
  const parts = [];
  let i = 0;
  while (i < names.length) {
    let j = i;
    while (j + 1 < names.length && names[j + 1] === names[i]) j++;
    const count = j - i + 1;
    parts.push(count > 1 ? `${names[i]} x${count}` : names[i]);
    i = j + 1;
  }
  return parts.join(', ');
}

// ── Init / entry points ──────────────────────────────────────────────────────

export function initAnalyzerPane() {
  const browseBtn = document.querySelector('#analyzer-browse-btn');
  if (browseBtn) {
    browseBtn.addEventListener('click', async () => {
      try {
        const selected = await open({
          multiple: false,
          filters: [{ name: 'Demo Files', extensions: ['dem'] }],
          title: 'Select Demo to Analyze',
        });
        if (selected) {
          const path = Array.isArray(selected) ? selected[0] : selected;
          loadAnalyzerDemo(path);
        }
      } catch (err) {
        console.error('Error opening demo file dialog:', err);
      }
    });
  }

  document.querySelectorAll('.analyzer-subtab-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      document.querySelectorAll('.analyzer-subtab-btn').forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');
      activeSubTab = btn.dataset.subtab;
      renderActiveTab();
    });
  });
}

export async function loadAnalyzerDemo(path) {
  const container = document.querySelector('#analyzer-tab-content');
  const titleEl = document.querySelector('#analyzer-current-file');
  if (container) container.innerHTML = '<p class="analyzer-empty">Analyzing demo…</p>';
  if (titleEl) titleEl.textContent = 'Analyzing…';
  try {
    report = await analyzeDemoFull(path);
    highlightedPlayerId = null;
    selectedPlayerId = null;
    if (titleEl) titleEl.textContent = report.file_name;
    renderActiveTab();
  } catch (err) {
    if (container) {
      container.innerHTML = `<p class="analyzer-empty" style="color:#f44336;">Failed to analyze demo: ${esc(String(err))}</p>`;
    }
    if (titleEl) titleEl.textContent = '';
  }
}

function renderActiveTab() {
  const container = document.querySelector('#analyzer-tab-content');
  if (!container) return;
  if (!report) {
    container.innerHTML = '<p class="analyzer-empty">Browse for a demo file, or select one from the Workspace and click "View Match Telemetry".</p>';
    return;
  }
  switch (activeSubTab) {
    case 'summary': renderSummaryTab(container); break;
    case 'scoreboard': renderScoreboardTab(container); break;
    case 'player-details': renderPlayerDetailsTab(container); break;
    case 'team-details': renderTeamDetailsTab(container); break;
    case 'timeline': renderTimelineTab(container); break;
    case 'rounds': renderRoundsTab(container); break;
    case 'chat': renderChatTab(container); break;
  }
}

// ── 1. Summary ────────────────────────────────────────────────────────────────

function renderSummaryTab(container) {
  const r = report;
  const di = r.demo_info;
  const st = r.state;

  const section = (title, rows) => `
    <div class="analyzer-section">
      <h4 class="analyzer-section-title">${esc(title)}</h4>
      <table class="analyzer-kv-table"><tbody>
        ${rows.map(([label, val]) => `<tr><td class="kv-label">${esc(label)}</td><td class="kv-value">${val}</td></tr>`).join('')}
      </tbody></table>
    </div>`;

  const createdDate = new Date(r.file_created_unix_secs * 1000);
  const createdStr = isFinite(createdDate.getTime()) && r.file_created_unix_secs > 0 ? createdDate.toLocaleString() : '—';

  const gameModMap = { dod: 'Day of Defeat', cstrike: 'Counter-Strike', valve: 'Half-Life' };
  const gameMod = gameModMap[di.game_directory] || di.game_directory;

  const recordedBy = (() => {
    if (di.demo_type === 'HLTV') return st.hltv_name || 'HLTV';
    const p = (st.players || []).find((p) => playerClientId(p) === st.pov_player_index);
    return p ? p.name : 'Unknown';
  })();

  const matchType = (() => {
    if (!st.clan_match_detected) return 'Public / Pickup';
    const hasCompletedRound = (st.rounds || []).some((r) => !!r.Completed);
    if (!st.match_start_witnessed && !hasCompletedRound) return 'Clan Match (Pre-game)';
    if (st.started_late || st.ended_early) return 'Clan Match (Incomplete Recording)';
    return 'Clan Match (Fully Recorded)';
  })();

  const demoDuration = formatDuration(durSecs(st.current_time && st.current_time.viewdemo_offset));

  const matchDuration = (() => {
    const rounds = st.rounds || [];
    if (rounds.length === 0) return '—';
    const first = rounds[0];
    const last = rounds[rounds.length - 1];
    const startTime = first.Active ? first.Active.start_time : first.Completed.start_time;
    const endTime = last.Completed ? last.Completed.end_time : st.current_time;
    return formatDuration(Math.max(0, durSecs(endTime.viewdemo_offset) - durSecs(startTime.viewdemo_offset)));
  })();

  container.innerHTML = `
    <div class="analyzer-summary-grid">
      ${section('File Information', [
        ['File name', esc(r.file_name)],
        ['File path', esc(r.file_dir)],
        ['File size', `${r.file_size_mb.toFixed(2)} MB`],
        ['File created', esc(createdStr)],
      ])}
      ${section('Game Details', [
        ['Game mod', esc(gameMod)],
        ['Map name', esc(di.map_name)],
        ['Map checksum', String(di.map_checksum)],
      ])}
      ${section('Server Information', [
        ['Server name', esc(st.server_name || '—')],
        ['Server address', esc(st.server_address || '—')],
      ])}
      ${section('Demo & Match Details', [
        ['Recorded by', esc(recordedBy)],
        ['Demo type', esc(di.demo_type)],
        ['Match type', esc(matchType)],
        ['Demo duration', demoDuration],
        ['Match duration', matchDuration],
      ])}
      ${section('Technical Specifications', [
        ['Demo protocol', String(di.demo_protocol)],
        ['Network protocol', String(di.network_protocol)],
      ])}
    </div>`;
}

// ── 2. Scoreboard ────────────────────────────────────────────────────────────

function buildScoreboardGroups() {
  const players = report.state.players || [];
  const groups = { allies: [], axis: [], spec: [], unassigned: [] };
  players.forEach((p) => {
    if (p.team === 'Allies' || p.team === 'British') groups.allies.push(p);
    else if (p.team === 'Axis') groups.axis.push(p);
    else if (p.team === 'Spectators') groups.spec.push(p);
    else groups.unassigned.push(p);
  });
  // Score DESC, Kills DESC, Deaths ASC, Name ASC, id ASC — matches dev's ScoreboardCache sort key.
  const sortFn = (a, b) => {
    if (b.stats[0] !== a.stats[0]) return b.stats[0] - a.stats[0];
    if (b.stats[1] !== a.stats[1]) return b.stats[1] - a.stats[1];
    if (a.stats[2] !== b.stats[2]) return a.stats[2] - b.stats[2];
    const na = a.name.toLowerCase(), nb = b.name.toLowerCase();
    if (na !== nb) return na < nb ? -1 : 1;
    return (a.id || '') < (b.id || '') ? -1 : 1;
  };
  Object.values(groups).forEach((g) => g.sort(sortFn));
  const totals = (arr) => arr.reduce((acc, p) => [acc[0] + p.stats[0], acc[1] + p.stats[1], acc[2] + p.stats[2]], [0, 0, 0]);
  return {
    groups,
    totals: { allies: totals(groups.allies), axis: totals(groups.axis), spec: totals(groups.spec), unassigned: totals(groups.unassigned) },
  };
}

function renderScoreboardTab(container) {
  const st = report.state;
  const { groups, totals } = buildScoreboardGroups();
  const alliesLabel = teamLabel('Allies', st.allies_are_british);
  const alliesColor = teamColor(st.allies_are_british ? 'British' : 'Allies');

  const alliesScore = getTeamScore((t) => t === 'Allies' || t === 'British');
  const axisScore = getTeamScore((t) => t === 'Axis');
  const cmp = alliesScore > axisScore ? '>' : (alliesScore === axisScore ? '=' : '<');

  let banner = '';
  if (st.started_late || st.ended_early) {
    const fmt = (d) => (d ? formatMMSS(durSecs(d)) : '??:??');
    let msg;
    if (st.started_late && st.ended_early) msg = `Partial recording — demo started with ${fmt(st.first_time_left)} remaining and ended before the match concluded.`;
    else if (st.started_late) msg = `Partial recording — demo started with ${fmt(st.first_time_left)} remaining on the clock.`;
    else msg = `Partial recording — demo ended before the match concluded (${fmt(st.last_time_left)} remaining at cutoff).`;
    banner = `<div class="analyzer-warning-banner">${esc(msg)}</div>`;
  }

  const groupBlock = (label, color, players, tot, key) => {
    if (players.length === 0 && (key === 'spec' || key === 'unassigned')) return '';
    const rows = players.map((p) => renderScoreboardRow(p, color)).join('');
    return `
      <tr class="scoreboard-group-header" style="color:${color};">
        <td colspan="2">${esc(label)} &mdash; ${players.length} player(s)</td>
        <td style="text-align:right;">${tot[0]}</td>
        <td style="text-align:right;">${tot[1]}</td>
        <td style="text-align:right;">${tot[2]}</td>
      </tr>
      ${rows}
      <tr class="scoreboard-spacer-row"><td colspan="5"></td></tr>`;
  };

  container.innerHTML = `
    <h3 class="analyzer-heading">Scoreboard: ${esc(alliesLabel)} (${alliesScore}) ${cmp} Axis (${axisScore})</h3>
    ${banner}
    <div class="table-wrapper">
      <table class="analyzer-table">
        <thead><tr><th>Name</th><th>Class</th><th style="text-align:right;">Score</th><th style="text-align:right;">Kills</th><th style="text-align:right;">Deaths</th></tr></thead>
        <tbody>
          ${groupBlock(alliesLabel, alliesColor, groups.allies, totals.allies, 'allies')}
          ${groupBlock('Axis', teamColor('Axis'), groups.axis, totals.axis, 'axis')}
          ${groupBlock('Spectators', teamColor('Spectators'), groups.spec, totals.spec, 'spec')}
          ${groupBlock('Unassigned', teamColor('Unassigned'), groups.unassigned, totals.unassigned, 'unassigned')}
        </tbody>
      </table>
    </div>`;

  container.querySelectorAll('tr.scoreboard-player-row').forEach((tr) => {
    tr.addEventListener('click', () => {
      const id = tr.dataset.playerId;
      highlightedPlayerId = highlightedPlayerId === id ? null : id;
      renderScoreboardTab(container);
    });
  });
}

function renderScoreboardRow(p, color) {
  const isSelected = highlightedPlayerId === p.id;
  const isPov = report.state.pov_player_index !== null && report.state.pov_player_index !== undefined && playerClientId(p) === report.state.pov_player_index;
  const povBadge = isPov ? ' 🎥' : '';
  const reconnBadge = p.has_reconnected ? ' <span title="Player reconnected mid-demo" style="color:#ffb74d;">🔄</span>' : '';
  const preDemoBadge = p.has_pre_demo_activity ? ' <span title="Player had pre-existing stats when recording started" style="color:#ffb74d;">*</span>' : '';
  const rowColor = isSelected ? '#ffffff' : color;
  return `
    <tr class="scoreboard-player-row" data-player-id="${esc(p.id)}" style="color:${rowColor};cursor:pointer;${isSelected ? 'background:rgba(255,255,255,0.08);' : ''}">
      <td>${esc(p.name)}${povBadge}${reconnBadge}${preDemoBadge}</td>
      <td>${esc(p.class || 'Unknown')}</td>
      <td style="text-align:right;">${p.stats[0]}</td>
      <td style="text-align:right;">${p.stats[1]}</td>
      <td style="text-align:right;">${p.stats[2]}</td>
    </tr>`;
}

// ── 3. Player Details ────────────────────────────────────────────────────────

function renderPlayerDetailsTab(container) {
  const players = (report.state.players || []).slice().sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
  if (players.length === 0) {
    container.innerHTML = '<p class="analyzer-empty">No players found in this demo.</p>';
    return;
  }
  let selectedId = selectedPlayerId;
  if (!selectedId || !players.find((p) => p.id === selectedId)) {
    selectedId = highlightedPlayerId && players.find((p) => p.id === highlightedPlayerId) ? highlightedPlayerId : players[0].id;
  }
  selectedPlayerId = selectedId;

  const options = players.map((p) => `<option value="${esc(p.id)}" ${p.id === selectedId ? 'selected' : ''}>${esc(p.name)}</option>`).join('');

  container.innerHTML = `
    <div class="analyzer-toolbar">
      <label>Player:</label>
      <select id="player-details-select">${options}</select>
    </div>
    <div id="player-details-body"></div>`;

  container.querySelector('#player-details-select').addEventListener('change', (e) => {
    selectedPlayerId = e.target.value;
    highlightedPlayerId = selectedPlayerId;
    renderPlayerDetailsBody(container, players.find((p) => p.id === selectedPlayerId));
  });

  renderPlayerDetailsBody(container, players.find((p) => p.id === selectedId));
}

function renderPlayerDetailsBody(tabContainer, p) {
  const body = tabContainer.querySelector('#player-details-body');
  if (!body) return;
  if (!p) { body.innerHTML = ''; return; }

  const color = teamColor(p.team);
  const kd = p.stats[2] > 0 ? p.stats[1] / p.stats[2] : p.stats[1];
  const lifespans = mortalityLifespans(p.mortality);
  const avgLife = lifespans.length ? lifespans.reduce((a, b) => a + b, 0) / lifespans.length : 0;
  const minLife = lifespans.length ? Math.min(...lifespans) : 0;
  const maxLife = lifespans.length ? Math.max(...lifespans) : 0;

  const connected = isConnected(p);
  const clientId = playerClientId(p);
  const steamId = steamIdDisplay(p.id);

  const weaponRows = Object.entries(p.weapon_breakdown || {}).sort((a, b) => b[1][0] - a[1][0] || a[0].localeCompare(b[0]));
  const totalKills = weaponRows.reduce((s, [, v]) => s + v[0], 0) || 1;

  body.innerHTML = `
    <div class="analyzer-hero-card" style="border-color:${color}66;">
      <div class="flex-between">
        <div>
          <div class="analyzer-hero-name">${esc(p.name)}</div>
          <div class="analyzer-hero-sub" style="color:${color};">${esc((teamLabel(p.team, report.state.allies_are_british) || 'UNASSIGNED').toUpperCase())}${p.class ? ` &nbsp;|&nbsp; ${esc(p.class.toUpperCase())}` : ''}</div>
        </div>
        <div class="analyzer-hero-links">
          ${/^\d{15,20}$/.test(p.id) ? `<a href="https://steamcommunity.com/profiles/${esc(p.id)}" target="_blank" rel="noopener">Steam Profile</a>` : '<span class="text-muted">No Steam ID</span>'}
        </div>
      </div>
      <div class="analyzer-hero-status">
        <span>Steam ID: <code>${esc(steamId)}</code></span>
        <span style="color:${connected ? '#4caf50' : '#888'};">${connected ? `Connected (Slot ${clientId})` : 'Disconnected'}</span>
        ${p.has_reconnected ? '<span style="color:#ffb74d;">🔄 Reconnected mid-demo</span>' : ''}
        ${p.has_pre_demo_activity ? '<span style="color:#ffb74d;">* Pre-existing stats</span>' : ''}
      </div>
    </div>

    <div class="analyzer-stat-cards">
      <div class="analyzer-stat-card"><div class="stat-title">Match Score</div><div class="stat-value">${p.stats[0]}</div></div>
      <div class="analyzer-stat-card"><div class="stat-title">Kills</div><div class="stat-value">${p.stats[1]}</div></div>
      <div class="analyzer-stat-card"><div class="stat-title">Deaths</div><div class="stat-value">${p.stats[2]}</div><div class="stat-badge" style="color:${kd >= 1 ? '#22c55e' : '#ef4444'};">${kd.toFixed(2)} K/D</div></div>
      <div class="analyzer-stat-card"><div class="stat-title">Avg. Lifespan</div><div class="stat-value">${avgLife.toFixed(1)}s</div><div class="stat-badge text-muted">Min: ${minLife.toFixed(0)}s / Max: ${maxLife.toFixed(0)}s</div></div>
    </div>

    <div class="analyzer-two-col">
      <div>
        <h4 class="analyzer-section-title">Weapon Breakdown</h4>
        <div class="table-wrapper" style="max-height:320px;">
          <table class="analyzer-table">
            <thead><tr><th>Weapon</th><th style="text-align:right;">Kills</th><th>% of Total</th><th style="text-align:right;">Team Kills</th></tr></thead>
            <tbody>
              ${weaponRows.map(([w, [k, tk]]) => `
                <tr>
                  <td>${esc(weaponName(w))}</td>
                  <td style="text-align:right;">${k}</td>
                  <td><div class="analyzer-progress"><div class="analyzer-progress-fill" style="width:${(k / totalKills * 100).toFixed(1)}%;"></div></div><span class="analyzer-progress-label">${(k / totalKills * 100).toFixed(1)}%</span></td>
                  <td style="text-align:right;">${tk}</td>
                </tr>`).join('') || '<tr><td colspan="4" class="table-empty">No weapon data.</td></tr>'}
            </tbody>
          </table>
        </div>
      </div>
      <div>
        <h4 class="analyzer-section-title">Kill Streaks</h4>
        ${renderKillStreaksTable(p)}
      </div>
    </div>`;

  body.querySelectorAll('.killstreak-victim').forEach((el) => {
    el.addEventListener('click', () => {
      const vid = el.dataset.victimId;
      if (!(report.state.players || []).find((pl) => pl.id === vid)) return;
      selectedPlayerId = vid;
      highlightedPlayerId = vid;
      renderPlayerDetailsTab(document.querySelector('#analyzer-tab-content'));
    });
  });
}

function renderKillStreaksTable(p) {
  const streaks = (p.kill_streaks || []).filter((s) => (s.kills || []).length > 0);
  if (streaks.length === 0) return '<p class="analyzer-empty">No kill streaks recorded.</p>';

  const rows = streaks.map((s, idx) => {
    const kills = s.kills;
    const startSecs = durSecs(kills[0][0].viewdemo_offset);
    const endSecs = durSecs(kills[kills.length - 1][0].viewdemo_offset);
    const durationSecs = Math.max(0, endSecs - startSecs);
    const weaponSummary = groupConsecutiveWeapons(kills.map((k) => weaponName(k[1])));

    const sub = kills.map((k, i) => {
      const victim = (report.state.players || []).find((pl) => pl.id === k[2]);
      const victimName = victim ? victim.name : k[2];
      const victimColor = victim ? teamColor(victim.team) : '#7ec8e3';
      const delta = i === 0 ? '—' : `+${formatMMSS(durSecs(k[0].viewdemo_offset) - durSecs(kills[i - 1][0].viewdemo_offset))}`;
      return `<tr class="killstreak-subrow"><td></td><td class="text-muted">${esc(delta)}</td><td colspan="2" style="font-size:0.85em;">${esc(weaponName(k[1]))} &#9876; <span class="killstreak-victim" data-victim-id="${esc(k[2])}" style="color:${victimColor};cursor:pointer;">${esc(victimName)}</span></td></tr>`;
    }).join('');

    return `
      <tr class="killstreak-row">
        <td>${idx + 1}</td>
        <td>${kills.length}</td>
        <td>${formatGameTime(startSecs)}</td>
        <td>${durationSecs.toFixed(1)}s</td>
      </tr>
      <tr><td></td><td colspan="3" style="font-size:0.85em;color:#aaa;">${esc(weaponSummary)}</td></tr>
      ${sub}`;
  }).join('');

  return `
    <div class="table-wrapper" style="max-height:320px;">
      <table class="analyzer-table">
        <thead><tr><th>Wave</th><th>Kills</th><th>Time</th><th>Duration</th></tr></thead>
        <tbody>${rows}</tbody>
      </table>
    </div>`;
}

// ── 4. Team Details ──────────────────────────────────────────────────────────

function renderTeamDetailsTab(container) {
  const st = report.state;
  const players = st.players || [];
  const alliesLabel = teamLabel('Allies', st.allies_are_british);
  const alliesColor = teamColor(st.allies_are_british ? 'British' : 'Allies');
  const axisColor = teamColor('Axis');

  const alliesPlayers = players.filter((p) => p.team === 'Allies' || p.team === 'British');
  const axisPlayers = players.filter((p) => p.team === 'Axis');

  const sumStat = (arr, idx) => arr.reduce((s, p) => s + p.stats[idx], 0);
  const alliesScore = getTeamScore((t) => t === 'Allies' || t === 'British');
  const axisScore = getTeamScore((t) => t === 'Axis');
  const alliesKills = sumStat(alliesPlayers, 1), axisKills = sumStat(axisPlayers, 1);
  const alliesDeaths = sumStat(alliesPlayers, 2), axisDeaths = sumStat(axisPlayers, 2);
  const kdOrKills = (k, d) => (d > 0 ? (k / d).toFixed(2) : String(k));

  const overviewRow = (label, allies, axis) => `<tr><td class="kv-label">${esc(label)}</td><td style="text-align:right;">${allies}</td><td style="text-align:right;">${axis}</td></tr>`;

  const weaponTable = (arr) => {
    const agg = {};
    arr.forEach((p) => Object.entries(p.weapon_breakdown || {}).forEach(([w, [k, tk]]) => {
      if (!agg[w]) agg[w] = [0, 0];
      agg[w][0] += k; agg[w][1] += tk;
    }));
    const rows = Object.entries(agg).sort((a, b) => b[1][0] - a[1][0] || a[0].localeCompare(b[0]));
    if (rows.length === 0) return '<p class="analyzer-empty">No weapon data.</p>';
    const totalKills = rows.reduce((s, [, v]) => s + v[0], 0) || 1;
    const totalTk = rows.reduce((s, [, v]) => s + v[1], 0) || 1;
    return `
      <div class="table-wrapper">
        <table class="analyzer-table">
          <thead><tr><th>Weapon</th><th style="text-align:right;">Kills</th><th>% of Total</th><th style="text-align:right;">Team Kills</th><th>% of Total</th></tr></thead>
          <tbody>
            ${rows.map(([w, [k, tk]]) => `
              <tr>
                <td>${esc(weaponName(w))}</td>
                <td style="text-align:right;">${k}</td>
                <td><div class="analyzer-progress"><div class="analyzer-progress-fill" style="width:${(k / totalKills * 100).toFixed(1)}%;"></div></div><span class="analyzer-progress-label">${(k / totalKills * 100).toFixed(1)}%</span></td>
                <td style="text-align:right;">${tk}</td>
                <td><div class="analyzer-progress"><div class="analyzer-progress-fill" style="width:${(tk / totalTk * 100).toFixed(1)}%;background:#e57373;"></div></div><span class="analyzer-progress-label">${(tk / totalTk * 100).toFixed(1)}%</span></td>
              </tr>`).join('')}
          </tbody>
        </table>
      </div>`;
  };

  container.innerHTML = `
    <h3 class="analyzer-heading">Team Details</h3>
    <h4 class="analyzer-section-title">Match Overview</h4>
    <table class="analyzer-kv-table" style="max-width:520px;">
      <thead><tr><th></th><th style="text-align:right;color:${alliesColor};">${esc(alliesLabel)}</th><th style="text-align:right;color:${axisColor};">Axis</th></tr></thead>
      <tbody>
        ${overviewRow('Round Score', alliesScore, axisScore)}
        ${overviewRow('Total Kills', alliesKills, axisKills)}
        ${overviewRow('Total Deaths', alliesDeaths, axisDeaths)}
        ${overviewRow('Team K/D', kdOrKills(alliesKills, alliesDeaths), kdOrKills(axisKills, axisDeaths))}
        ${overviewRow('Active Players', alliesPlayers.length, axisPlayers.length)}
      </tbody>
    </table>

    <h4 class="analyzer-section-title" style="margin-top:16px;">Team Weapon Performance</h4>
    <details open class="analyzer-collapsible"><summary style="color:${alliesColor};">${esc(alliesLabel)}</summary>${weaponTable(alliesPlayers)}</details>
    <details open class="analyzer-collapsible"><summary style="color:${axisColor};">Axis</summary>${weaponTable(axisPlayers)}</details>`;
}

// ── 5. Timeline ───────────────────────────────────────────────────────────────

function renderTimelineTab(container) {
  const st = report.state;
  const alliesLabel = teamLabel('Allies', st.allies_are_british);
  const alliesColor = teamColor(st.allies_are_british ? 'British' : 'Allies');
  const axisColor = teamColor('Axis');

  container.innerHTML = `
    <h3 class="analyzer-heading">Team Score Timeline</h3>
    <div class="analyzer-timeline-legend">
      <span><span class="legend-swatch" style="background:${alliesColor};"></span>${esc(alliesLabel)}</span>
      <span><span class="legend-swatch" style="background:${axisColor};"></span>Axis</span>
    </div>
    <canvas id="analyzer-timeline-canvas" style="width:100%;height:340px;background:#121212;border:1px solid #333;border-radius:2px;"></canvas>`;

  const timeline = (st.team_scores && st.team_scores.timeline) || [];
  const alliesSeries = timeline.filter(([, t]) => t === 'Allies' || t === 'British').map(([time, , score]) => [durSecs(time.viewdemo_offset), score]);
  const axisSeries = timeline.filter(([, t]) => t === 'Axis').map(([time, , score]) => [durSecs(time.viewdemo_offset), score]);

  drawTimelineChart(container.querySelector('#analyzer-timeline-canvas'), alliesSeries, axisSeries, alliesColor, axisColor);
}

function drawTimelineChart(canvas, seriesA, seriesB, colorA, colorB) {
  if (!canvas) return;
  const width = canvas.clientWidth || 600;
  const height = canvas.clientHeight || 340;
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext('2d');
  ctx.fillStyle = '#121212';
  ctx.fillRect(0, 0, width, height);

  const allPoints = [...seriesA, ...seriesB];
  if (allPoints.length === 0) {
    ctx.fillStyle = '#666';
    ctx.font = '12px sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText('No team score events recorded', width / 2, height / 2);
    return;
  }
  const padding = 36;
  const maxX = Math.max(...allPoints.map((p) => p[0]), 1);
  const maxY = Math.max(...allPoints.map((p) => p[1]), 1);
  const minY = Math.min(0, ...allPoints.map((p) => p[1]));

  const xAt = (x) => padding + (x / maxX) * (width - padding * 2);
  const yAt = (y) => height - padding - ((y - minY) / (maxY - minY || 1)) * (height - padding * 2);

  ctx.strokeStyle = '#333';
  ctx.beginPath();
  ctx.moveTo(padding, height - padding);
  ctx.lineTo(width - padding, height - padding);
  ctx.moveTo(padding, padding);
  ctx.lineTo(padding, height - padding);
  ctx.stroke();

  ctx.fillStyle = '#888';
  ctx.font = '10px monospace';
  ctx.textAlign = 'left';
  ctx.fillText('0:00', padding - 4, height - padding + 14);
  ctx.textAlign = 'right';
  ctx.fillText(formatMMSS(maxX), width - padding, height - padding + 14);
  ctx.fillText(String(maxY), padding - 6, padding + 4);

  const drawSeries = (series, color) => {
    if (series.length === 0) return;
    ctx.strokeStyle = color;
    ctx.lineWidth = 2;
    ctx.beginPath();
    series.forEach(([x, y], i) => {
      const px = xAt(x), py = yAt(y);
      if (i === 0) ctx.moveTo(px, py); else ctx.lineTo(px, py);
    });
    ctx.stroke();
    ctx.fillStyle = color;
    series.forEach(([x, y]) => {
      ctx.beginPath();
      ctx.arc(xAt(x), yAt(y), 2.5, 0, Math.PI * 2);
      ctx.fill();
    });
  };
  drawSeries(seriesA, colorA);
  drawSeries(seriesB, colorB);
}

// ── 6. Rounds ─────────────────────────────────────────────────────────────────

function renderRoundsTab(container) {
  const rounds = report.state.rounds || [];
  const completed = rounds.filter((r) => r.Completed).map((r) => r.Completed);
  let matchDurationSecs = 0;

  const rows = completed.map((r, i) => {
    const startSecs = durSecs(r.start_time.viewdemo_offset);
    const endSecs = durSecs(r.end_time.viewdemo_offset);
    const roundDurSecs = Math.max(0, endSecs - startSecs);
    matchDurationSecs += roundDurSecs;

    let winnerCell = '', killsCell = '', color = '#ffffff';
    if (r.winner_stats) {
      const [team, kills] = r.winner_stats;
      color = teamColor(team === 'Allies' && report.state.allies_are_british ? 'British' : team);
      winnerCell = esc(teamLabel(team, report.state.allies_are_british));
      killsCell = String(kills);
    }
    return `
      <tr>
        <td><div style="width:10px;height:10px;border-radius:2px;background:${color};"></div></td>
        <td>${i + 1}</td>
        <td>${formatDuration(startSecs)}</td>
        <td>${formatDuration(roundDurSecs)}</td>
        <td>${winnerCell}</td>
        <td style="text-align:right;">${killsCell}</td>
      </tr>`;
  }).join('');

  container.innerHTML = `
    <h3 class="analyzer-heading">Rounds</h3>
    <div class="table-wrapper">
      <table class="analyzer-table">
        <thead><tr><th></th><th>#</th><th>Start Time</th><th>Duration</th><th>Winner</th><th style="text-align:right;">Kills by Winner</th></tr></thead>
        <tbody>
          ${rows || '<tr><td colspan="6" class="table-empty">No completed rounds recorded.</td></tr>'}
          <tr class="analyzer-total-row"><td></td><td></td><td></td><td>${formatDuration(matchDurationSecs)}</td><td></td><td></td></tr>
        </tbody>
      </table>
    </div>`;
}

// ── 7. Chat Log ───────────────────────────────────────────────────────────────

function renderChatTab(container) {
  container.innerHTML = `
    <h3 class="analyzer-heading">Chat &amp; System Log</h3>
    <div class="analyzer-toolbar" style="flex-wrap:wrap;gap:10px;">
      <label><input type="checkbox" id="chat-filter-mm1" ${chatFilters.showMm1 ? 'checked' : ''}/> All Chat</label>
      <label><input type="checkbox" id="chat-filter-mm2" ${chatFilters.showMm2 ? 'checked' : ''}/> Team Chat</label>
      <label><input type="checkbox" id="chat-filter-sys" ${chatFilters.showSystem ? 'checked' : ''}/> System</label>
      <label>Team:
        <select id="chat-filter-team">
          ${['All', 'Allies', 'British', 'Axis', 'Spectators'].map((t) => `<option value="${t}" ${chatFilters.team === t ? 'selected' : ''}>${t}</option>`).join('')}
        </select>
      </label>
      <input type="text" id="chat-filter-search" placeholder="Search sender or text..." value="${esc(chatFilters.search)}" style="flex:1;min-width:150px;" />
    </div>
    <div id="chat-log-list" class="analyzer-chat-log"></div>`;

  container.querySelector('#chat-filter-mm1').addEventListener('change', (e) => { chatFilters.showMm1 = e.target.checked; renderChatLogList(); });
  container.querySelector('#chat-filter-mm2').addEventListener('change', (e) => { chatFilters.showMm2 = e.target.checked; renderChatLogList(); });
  container.querySelector('#chat-filter-sys').addEventListener('change', (e) => { chatFilters.showSystem = e.target.checked; renderChatLogList(); });
  container.querySelector('#chat-filter-team').addEventListener('change', (e) => { chatFilters.team = e.target.value; renderChatLogList(); });
  container.querySelector('#chat-filter-search').addEventListener('input', (e) => { chatFilters.search = e.target.value; renderChatLogList(); });

  renderChatLogList();
}

function renderChatLogList() {
  const listEl = document.querySelector('#chat-log-list');
  if (!listEl) return;
  const messages = report.state.chat_messages || [];
  const search = chatFilters.search.toLowerCase();

  const filtered = messages.filter((m) => {
    if (m.chat_type === 'Mm1' && !chatFilters.showMm1) return false;
    if (m.chat_type === 'Mm2' && !chatFilters.showMm2) return false;
    if (m.chat_type === 'System' && !chatFilters.showSystem) return false;
    if (chatFilters.team !== 'All' && m.chat_type !== 'System') {
      const t = m.sender_team;
      const matchesTeam = t === chatFilters.team
        || (chatFilters.team === 'Allies' && t === 'British')
        || (chatFilters.team === 'British' && t === 'Allies');
      if (!matchesTeam) return false;
    }
    if (search) {
      const hay = `${m.sender_name || ''} ${m.text || ''}`.toLowerCase();
      if (!hay.includes(search)) return false;
    }
    return true;
  });

  if (filtered.length === 0) {
    listEl.innerHTML = '<p class="analyzer-empty">No messages match the current filters.</p>';
    return;
  }

  listEl.innerHTML = filtered.map((m) => {
    const time = formatGameTime(durSecs(m.time.viewdemo_offset));
    const deadBadge = m.sender_dead ? '<span style="color:#dc3232;">*DEAD*</span> ' : '';
    let body;
    if (m.chat_type === 'System') {
      body = `<span style="color:#c89650;">[system]</span> <span style="color:#b4dcdc;font-style:italic;">${esc(m.text)}</span>`;
    } else {
      const teamBadge = m.chat_type === 'Mm2' ? `<span style="color:${teamColor(m.sender_team)};">(Team)</span> ` : '';
      body = `${teamBadge}<span style="color:${teamColor(m.sender_team)};font-weight:600;">${esc(m.sender_name || 'Unknown')}:</span> ${esc(m.text)}`;
    }
    return `<div class="analyzer-chat-row"><span class="chat-timestamp">[${time}]</span> ${deadBadge}${body}</div>`;
  }).join('');
}
