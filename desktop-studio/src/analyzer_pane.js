// analyzer_pane.js
// Standalone "Demo Analyzer" tab — a JS port of the egui report_ui views
// (Summary / Scoreboard / Player Details / Team Details / Timeline / Rounds /
// Chat Log) from the `dev` branch. Reads the full analysis::{DemoInfo,
// AnalyzerState} payload from `analyze_demo_full` rather than the flattened
// generic JSON used by the compact inline telemetry summary.

import { open } from '@tauri-apps/plugin-dialog';
import { listen } from '@tauri-apps/api/event';
import { analyzeDemoFull, browseDirectory, defaultBrowseDir, countDemoFiles, scanDemoFolders } from './ipc_bridge.js';

let report = null;
let analyzerLoadInProgress = false;
let activeSubTab = 'summary';
let highlightedPlayerId = null; // shared selection: Scoreboard row <-> Player Details dropdown
let selectedPlayerId = null;
let chatFilters = {
  showMm1: true, showMm2: true, team: 'All', search: '',
  status: 'All', // 'All' | 'Alive' | 'Dead'
  showJoins: true, showTeams: true, showGameplay: true, showOtherSys: true,
};
// Kill Streaks weapon-category filter — reset whenever the selected player changes.
let disabledWeapons = new Set();

const WEAPON_CATEGORIES = [
  ['Grenades', ['Mk2Grenade', 'StickGrenade', 'MillsBomb']],
  ['Melee', ['Kabar', 'GermanKnife', 'BritishKnife', 'Spade', 'K98Bayonet', 'EnfieldBayonet', 'ButtStock']],
  ['Allied', ['M1911', 'Garand', 'Springfield', 'Thompson', 'Bar', 'M1Carbine', 'Browning30Cal', 'GreaseGun', 'Bazooka', 'LeeEnfield', 'ScopedLeeEnfield', 'Sten', 'Bren', 'Webley', 'Piat', 'M1A1Carbine', 'Mortar']],
  ['Axis', ['Luger', 'ScopedK98', 'Stg44', 'K98', 'Mp40', 'Mg42', 'Mg34', 'Fg42', 'ScopedFg42', 'K43', 'Panzerschreck']],
];

// ── Explorer sidebar state — a real native folder tree (drives -> subfolders,
// lazily loaded, expand/collapse) plus a 3-tier Quick Links box (Pinned /
// Recent / Local), both driving one shared `currentDir`. The demos table
// below is scoped to ONLY that single folder's contents (non-recursive) —
// mirrors dev's SidePanel::left explorer + `desktop_files`, see
// docs/tauri_parity_audit.md Area 3 for the corrected design this replaced
// an earlier (wrong-shape) recursive multi-folder aggregate with. Dev's
// "Group by Match"/"Group by Player-Recorder" view modes are NOT ported:
// their grouping keys (server_ip/player_roster_hash/recorder_id) were only
// ever assigned `None` in dev's own source, so those two view modes never
// actually grouped anything there either — restoring the one real, working
// view (Flat List) is the faithful port here, not a scope cut.
let getPinnedFolders = () => [];
let pinFolder = async () => {};
let unpinFolder = async () => {};
let getDemoFolderHistory = () => [];
// Gates the Explorer tree's per-subfolder "(N)" demo-count badge — dev's
// `settings.scan_folders_for_demos`, defaults false. Quick Links counts are
// NOT gated by this (dev never gated those either), only the tree is.
let getScanFoldersForDemos = () => false;
let setScanFoldersForDemos = async () => {};
let recordDemoFolderVisit = async () => {};
let forgetDemoFolderVisit = async () => {};

let currentDir = null;
const dirCache = new Map(); // path -> DirListing from browse_directory
const openTreeNodes = new Set();
let thisPcOpen = true;
let driveRoots = []; // DirEntryLite[]
let localFolders = []; // DemoFolderHit[] from scan_demo_folders
let localFoldersScanning = false;
let localFoldersScanTriggered = false;
const quickLinkCountCache = new Map(); // path -> demo_count

let currentFolderDemos = []; // DemoFileEntry[] for currentDir only
let browserSelectedDemo = null;
let browserError = null;
let demoFilterQuery = '';
let demoFilterType = 'All';
let demoFilterMap = '';
let demoFilterDateStart = '';
let demoFilterDateEnd = '';
let demoSortColumn = null; // 'name' | 'type' | 'map' | 'date'
let demoSortAscending = true;

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

// ── Explorer sidebar + single-folder demo list ───────────────────────────────

function demoTypeOf(entry) {
  return entry.demo_type || 'POV';
}

// Guarantees the drive letter and final folder name stay visible, eliding
// the middle when the full path is too long to fit the sidebar — e.g.
// "C:\...\DoD Demos". Falls back to a forward-slash root-relative path when
// short enough (dev's own `display_path`, see tree quick-links in
// docs/tauri_parity_audit.md Area 3). Full path is always available via the
// row's `title` attribute regardless of how the visible label is shortened.
function shortenPath(fullPath, maxChars = 34) {
  if (!fullPath) return '';
  const forward = fullPath.replace(/\\/g, '/');
  if (forward.length <= maxChars) return forward;

  const driveMatch = fullPath.match(/^([A-Za-z]:\\|\\\\|\/)/);
  const drive = driveMatch ? driveMatch[0].replace(/[\\/]+$/, '') : '';
  const segments = fullPath.split(/[\\/]/).filter(Boolean);
  let finalComponent = segments[segments.length - 1] || fullPath;

  let collapsed = `${drive}\\...\\${finalComponent}`;
  if (collapsed.length > maxChars) {
    const keep = Math.max(6, maxChars - drive.length - 8);
    finalComponent = `…${finalComponent.slice(-keep)}`;
    collapsed = `${drive}\\...\\${finalComponent}`;
  }
  return collapsed;
}

// `pinned_folders`/`demo_folder_history` are shared with Capture Studio's
// own "+ Add Demo Files" flow, which legitimately pushes individual `.dem`
// FILE paths into the same list (main.js's scanPaths) for its own rescan
// purposes. Quick Links/the tree only ever navigate to folders, so filter
// those file entries out here rather than showing them as bogus "pinned
// folders" that error out with "Not a directory" when clicked.
function isFolderPath(p) { return !/\.dem$/i.test(p); }

function pathSeparator(p) { return p.includes('\\') ? '\\' : '/'; }

function parentDirOf(path) {
  const sep = pathSeparator(path);
  const idx = path.lastIndexOf(sep);
  return idx > 0 ? path.slice(0, idx) : null;
}

// Full ancestor chain from the top of the tree down to (and including)
// `path` itself — used to auto-expand the Explorer Tree down to a selection
// and to detect ancestor/descendant relationships for the collapse-jumps-up
// mechanic below.
function ancestorChain(path) {
  const sep = pathSeparator(path);
  const trimmed = path.endsWith(sep) && path.length > sep.length ? path.slice(0, -1) : path;
  const segments = trimmed.split(sep).filter((s) => s.length > 0);
  const chain = [];
  if (sep === '\\' && /^[A-Za-z]:$/.test(segments[0] || '')) {
    let acc = `${segments[0]}\\`;
    chain.push(acc);
    for (let i = 1; i < segments.length; i++) {
      acc += segments[i];
      chain.push(acc);
      acc += '\\';
    }
  } else {
    let acc = '/';
    for (const seg of segments) {
      acc = acc === '/' ? `/${seg}` : `${acc}/${seg}`;
      chain.push(acc);
    }
  }
  return chain;
}

function isAncestorOf(candidate, descendant) {
  if (!descendant || candidate === descendant) return false;
  return ancestorChain(descendant).includes(candidate);
}

async function countDemoFilesCached(path) {
  if (quickLinkCountCache.has(path)) return quickLinkCountCache.get(path);
  const n = await countDemoFiles(path);
  quickLinkCountCache.set(path, n);
  return n;
}

function quickLinkRowHtml(folder, count, isPinned) {
  const label = shortenPath(folder);
  return `<div class="quicklink-row ${folder === currentDir ? 'selected' : ''}" data-path="${esc(folder)}" title="${esc(folder)}">
    <button class="quicklink-pin-btn ${isPinned ? 'pinned' : ''}" data-path="${esc(folder)}" title="${isPinned ? 'Unpin folder' : 'Pin folder'}">📌</button>
    <span class="quicklink-label">${esc(label)} (${count})</span>
  </div>`;
}

// Dev's Windows-11-style Quick Access pattern: Pinned (explicit bookmarks),
// Recent (auto-tracked history), Local (bounded background scan) — each
// tier hidden entirely when empty, excludes anything already promoted to a
// higher tier. See docs/tauri_parity_audit.md Area 3.
async function renderQuickLinksSection() {
  const container = document.querySelector('#analyzer-quick-links');
  if (!container) return;

  const pinned = (getPinnedFolders() || []).filter(isFolderPath);
  const recent = (getDemoFolderHistory() || []).filter(isFolderPath).filter((f) => !pinned.includes(f));
  const local = localFolders.map((f) => f.path).filter((f) => !pinned.includes(f));

  const allPaths = [...new Set([...pinned, ...recent, ...local])];
  await Promise.all(allPaths.map((p) => countDemoFilesCached(p)));

  const tier = (label, paths) => paths.length === 0 ? '' : `
    <div class="quicklink-tier-label">${esc(label)}</div>
    ${paths.map((p) => quickLinkRowHtml(p, quickLinkCountCache.get(p) || 0, pinned.includes(p))).join('')}`;

  if (pinned.length === 0 && recent.length === 0 && local.length === 0) {
    container.innerHTML = localFoldersScanning
      ? '<div class="quicklink-empty"><span class="spinner"></span> Scanning workspace…</div>'
      : '<div class="quicklink-empty">No demo folders found.</div>';
  } else {
    container.innerHTML = tier('📌 Pinned', pinned) + tier('🕒 Recent', recent) + tier('📂 Local', local);
  }

  container.querySelectorAll('.quicklink-row').forEach((row) => {
    row.addEventListener('click', (e) => {
      if (e.target.closest('.quicklink-pin-btn')) return;
      setCurrentDir(row.dataset.path);
    });
  });
  container.querySelectorAll('.quicklink-pin-btn').forEach((btn) => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const path = btn.dataset.path;
      const isPinned = (getPinnedFolders() || []).includes(path);
      if (isPinned) await unpinFolder(path); else await pinFolder(path);
      renderQuickLinksSection();
    });
  });
}

function treeRowHtml(entry) {
  const { path, name, demo_count } = entry;
  const isOpen = openTreeNodes.has(path);
  const isSelected = path === currentDir;
  const showCount = getScanFoldersForDemos() && demo_count > 0;
  const icon = showCount ? '📂' : '📁';
  const label = showCount ? `${name} (${demo_count})` : name;
  const arrow = isOpen ? '⏷' : '⏵';

  let childrenHtml = '';
  if (isOpen) {
    const listing = dirCache.get(path);
    if (listing) {
      childrenHtml = listing.subdirs.length > 0
        ? `<div class="tree-children">${listing.subdirs.map(treeRowHtml).join('')}</div>`
        : '';
    } else {
      childrenHtml = '<div class="tree-children"><div class="tree-loading">Loading…</div></div>';
    }
  }

  return `<div class="tree-node">
    <div class="tree-row ${isSelected ? 'selected' : ''}">
      <button class="tree-toggle" data-path="${esc(path)}">${arrow}</button>
      <span class="tree-label" data-path="${esc(path)}" title="${esc(path)}">${icon} ${esc(label)}</span>
    </div>
    ${childrenHtml}
  </div>`;
}

// Native Explorer Tree: drives -> subfolders, lazily loaded and cached per
// node, genuinely expand/collapse (default closed). Mirrors dev's
// `tree.rs::render_native_dir_node` — see docs/tauri_parity_audit.md Area 3.
async function renderExplorerTree() {
  const container = document.querySelector('#analyzer-tree');
  if (!container) return;
  if (driveRoots.length === 0) {
    try {
      const listing = await browseDirectory(null);
      driveRoots = listing.subdirs;
    } catch {
      driveRoots = [];
    }
  }

  const thisPcArrow = thisPcOpen ? '⏷' : '⏵';
  container.innerHTML = `<div class="tree-node">
    <div class="tree-row">
      <button class="tree-toggle" id="tree-this-pc-toggle">${thisPcArrow}</button>
      <span class="tree-label">💻 This PC</span>
    </div>
    ${thisPcOpen ? `<div class="tree-children">${driveRoots.map(treeRowHtml).join('')}</div>` : ''}
  </div>`;

  const thisPcToggle = container.querySelector('#tree-this-pc-toggle');
  thisPcToggle?.addEventListener('click', () => {
    thisPcOpen = !thisPcOpen;
    renderExplorerTree();
  });

  container.querySelectorAll('.tree-toggle:not(#tree-this-pc-toggle)').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const path = btn.dataset.path;
      if (openTreeNodes.has(path)) {
        openTreeNodes.delete(path);
        // Key mechanic: collapsing a node you're currently inside navigates
        // you up to it, rather than leaving you on a hidden selection —
        // matches dev/Windows Explorer both (tree.rs:373-383).
        if (isAncestorOf(path, currentDir)) {
          await setCurrentDir(path);
          return;
        }
        renderExplorerTree();
      } else {
        openTreeNodes.add(path);
        renderExplorerTree();
        if (!dirCache.has(path)) {
          try {
            dirCache.set(path, await browseDirectory(path));
          } catch {
            dirCache.set(path, { subdirs: [], demos: [] });
          }
          renderExplorerTree();
        }
      }
    });
  });
  container.querySelectorAll('.tree-label').forEach((el) => {
    el.addEventListener('click', () => setCurrentDir(el.dataset.path));
  });
}

async function forgetInvalidFolder(path) {
  if ((getPinnedFolders() || []).includes(path)) await unpinFolder(path);
  await forgetDemoFolderVisit(path);
}

// Sets the single folder the Demos panel and Explorer Tree are scoped to.
// Key mechanic: whenever a node's ancestor chain contains `currentDir`, that
// node force-opens on the next render — so simply changing `currentDir`
// (from a tree click *or* a Quick Links click) is what makes the tree
// auto-expand down to the newly selected folder (tree.rs:358-368).
async function setCurrentDir(path) {
  let listing;
  try {
    listing = await browseDirectory(path);
  } catch (err) {
    browserError = String(err);
    await forgetInvalidFolder(path);
    renderQuickLinksSection();
    renderDemoTable();
    return;
  }

  dirCache.set(path, listing);
  currentDir = path;
  browserError = null;

  const ancestors = ancestorChain(path);
  ancestors.slice(0, -1).forEach((a) => openTreeNodes.add(a));
  await Promise.all(ancestors.slice(0, -1).map(async (a) => {
    if (!dirCache.has(a)) {
      try { dirCache.set(a, await browseDirectory(a)); } catch { /* leaf render shows a loading placeholder */ }
    }
  }));

  currentFolderDemos = listing.demos;
  browserSelectedDemo = null;

  renderQuickLinksSection();
  await renderExplorerTree();
  renderDemoTable();

  if (listing.demos.length > 0) {
    await recordDemoFolderVisit(path);
    renderQuickLinksSection();
  }
}

async function triggerLocalFoldersScan(force = false) {
  if (localFoldersScanTriggered && !force) return;
  localFoldersScanTriggered = true;
  localFoldersScanning = true;
  renderQuickLinksSection();
  try {
    const root = currentDir || (await defaultBrowseDir());
    localFolders = await scanDemoFolders(root);
  } catch {
    localFolders = [];
  }
  localFoldersScanning = false;
  renderQuickLinksSection();
}

function demoDateISO(entry) {
  if (!entry.modified_unix_secs) return '';
  return new Date(entry.modified_unix_secs * 1000).toISOString().slice(0, 10);
}

function demoDateDisplay(entry) {
  if (!entry.modified_unix_secs) return '—';
  const d = new Date(entry.modified_unix_secs * 1000);
  return `${d.toLocaleDateString()} ${d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`;
}

function passesDemoFilter(entry) {
  if (demoFilterQuery) {
    const q = demoFilterQuery.toLowerCase();
    const hay = `${entry.name} ${entry.map_name || ''} ${entry.path}`.toLowerCase();
    if (!hay.includes(q)) return false;
  }
  if (demoFilterType !== 'All' && demoTypeOf(entry) !== demoFilterType) return false;
  if (demoFilterMap && !(entry.map_name || '').toLowerCase().includes(demoFilterMap.toLowerCase())) return false;
  const iso = demoDateISO(entry);
  if (demoFilterDateStart.length === 10 && (iso.length < 10 || iso < demoFilterDateStart)) return false;
  if (demoFilterDateEnd.length === 10 && (iso.length < 10 || iso > demoFilterDateEnd)) return false;
  return true;
}

function sortedFilteredDemos() {
  let list = currentFolderDemos.filter(passesDemoFilter);
  if (demoSortColumn) {
    list = list.slice().sort((a, b) => {
      let cmp;
      switch (demoSortColumn) {
        case 'name': cmp = a.name.toLowerCase().localeCompare(b.name.toLowerCase()); break;
        case 'type': cmp = demoTypeOf(a).localeCompare(demoTypeOf(b)); break;
        case 'map': cmp = (a.map_name || '').toLowerCase().localeCompare((b.map_name || '').toLowerCase()); break;
        case 'date': cmp = a.modified_unix_secs - b.modified_unix_secs; break;
        default: cmp = 0;
      }
      return demoSortAscending ? cmp : -cmp;
    });
  }
  return list;
}

function updateSortHeaderIndicators() {
  document.querySelectorAll('#analyzer-demo-table th[data-sort]').forEach((th) => {
    const base = th.dataset.label;
    th.textContent = demoSortColumn === th.dataset.sort ? `${base} ${demoSortAscending ? '▲' : '▼'}` : base;
  });
}

function renderDemoTable() {
  const tbody = document.querySelector('#analyzer-demo-tbody');
  if (!tbody) return;
  updateSortHeaderIndicators();

  if (browserError) {
    tbody.innerHTML = `<tr><td colspan="4" class="table-empty" style="color:#f44336;">${esc(browserError)}</td></tr>`;
    return;
  }
  if (!currentDir) {
    tbody.innerHTML = '<tr><td colspan="4" class="table-empty">Pick a folder from the Explorer sidebar.</td></tr>';
    return;
  }
  if (currentFolderDemos.length === 0) {
    tbody.innerHTML = '<tr><td colspan="4" class="table-empty">No demos found in this folder.</td></tr>';
    return;
  }
  const list = sortedFilteredDemos();
  if (list.length === 0) {
    tbody.innerHTML = '<tr><td colspan="4" class="table-empty">No demos match the current filters.</td></tr>';
    return;
  }

  tbody.innerHTML = list.map((entry) => {
    const isSelected = entry.path === browserSelectedDemo;
    return `<tr class="analyzer-demo-row ${isSelected ? 'selected' : ''}" data-path="${esc(entry.path)}" title="${esc(entry.path)}">
      <td>${esc(entry.name)}</td>
      <td>${esc(demoTypeOf(entry))}</td>
      <td>${esc(entry.map_name || '—')}</td>
      <td>${esc(demoDateDisplay(entry))}</td>
    </tr>`;
  }).join('');

  tbody.querySelectorAll('tr[data-path]').forEach((tr) => {
    tr.addEventListener('click', () => selectDemo(tr.dataset.path));
  });
}

function selectDemo(path) {
  browserSelectedDemo = path;
  renderDemoTable();
  loadAnalyzerDemo(path);
}

function initAnalyzerBrowserKeyboardNav() {
  document.addEventListener('keydown', (e) => {
    const pane = document.querySelector('#pane-demo-analyzer');
    if (!pane || pane.style.display === 'none') return;
    const tag = document.activeElement && document.activeElement.tagName;
    if (tag === 'INPUT' || tag === 'SELECT' || tag === 'TEXTAREA') return;

    const list = sortedFilteredDemos();
    if (list.length === 0) return;

    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const dir = e.key === 'ArrowDown' ? 1 : -1;
      const idx = list.findIndex((d) => d.path === browserSelectedDemo);
      const newIdx = idx === -1 ? (dir > 0 ? 0 : list.length - 1) : Math.min(list.length - 1, Math.max(0, idx + dir));
      browserSelectedDemo = list[newIdx].path;
      renderDemoTable();
      const row = document.querySelector(`#analyzer-demo-tbody tr[data-path="${CSS.escape(browserSelectedDemo)}"]`);
      row?.scrollIntoView({ block: 'nearest' });
    } else if (e.key === 'Enter' && browserSelectedDemo) {
      loadAnalyzerDemo(browserSelectedDemo);
    }
  });
}

function initAnalyzerBrowser() {
  const refreshBtn = document.querySelector('#analyzer-tree-refresh-btn');
  if (refreshBtn) {
    refreshBtn.addEventListener('click', () => {
      dirCache.clear();
      driveRoots = [];
      quickLinkCountCache.clear();
      triggerLocalFoldersScan(true);
      renderExplorerTree();
      if (currentDir) setCurrentDir(currentDir);
    });
  }

  const scanFoldersCb = document.querySelector('#analyzer-scan-folders-for-demos');
  if (scanFoldersCb) {
    scanFoldersCb.checked = getScanFoldersForDemos();
    scanFoldersCb.addEventListener('change', async (e) => {
      await setScanFoldersForDemos(e.target.checked);
      renderExplorerTree();
    });
  }

  const addPinBtn = document.querySelector('#analyzer-add-pin-btn');
  if (addPinBtn) {
    addPinBtn.addEventListener('click', async () => {
      try {
        const selected = await open({ directory: true, multiple: false, title: 'Add Pinned Folder' });
        if (selected) {
          const folder = Array.isArray(selected) ? selected[0] : selected;
          await pinFolder(folder);
          renderQuickLinksSection();
        }
      } catch (err) {
        console.error('Error picking folder to pin:', err);
      }
    });
  }

  document.querySelectorAll('#analyzer-demo-table th[data-sort]').forEach((th) => {
    th.addEventListener('click', () => {
      const col = th.dataset.sort;
      if (demoSortColumn === col) demoSortAscending = !demoSortAscending;
      else { demoSortColumn = col; demoSortAscending = true; }
      renderDemoTable();
    });
  });

  const searchEl = document.querySelector('#analyzer-filter-search');
  const typeEl = document.querySelector('#analyzer-filter-type');
  const mapEl = document.querySelector('#analyzer-filter-map');
  const dateStartEl = document.querySelector('#analyzer-filter-date-start');
  const dateEndEl = document.querySelector('#analyzer-filter-date-end');
  const resetBtn = document.querySelector('#analyzer-filter-reset');

  searchEl?.addEventListener('input', (e) => { demoFilterQuery = e.target.value; renderDemoTable(); });
  typeEl?.addEventListener('change', (e) => { demoFilterType = e.target.value; renderDemoTable(); });
  mapEl?.addEventListener('input', (e) => { demoFilterMap = e.target.value; renderDemoTable(); });
  dateStartEl?.addEventListener('input', (e) => { demoFilterDateStart = e.target.value; renderDemoTable(); });
  dateEndEl?.addEventListener('input', (e) => { demoFilterDateEnd = e.target.value; renderDemoTable(); });
  resetBtn?.addEventListener('click', () => {
    demoFilterQuery = ''; demoFilterType = 'All'; demoFilterMap = ''; demoFilterDateStart = ''; demoFilterDateEnd = '';
    if (searchEl) searchEl.value = '';
    if (typeEl) typeEl.value = 'All';
    if (mapEl) mapEl.value = '';
    if (dateStartEl) dateStartEl.value = '';
    if (dateEndEl) dateEndEl.value = '';
    renderDemoTable();
  });

  initAnalyzerBrowserKeyboardNav();

  renderQuickLinksSection();
  renderExplorerTree();
  triggerLocalFoldersScan();

  (async () => {
    const startDir = (getPinnedFolders() || []).filter(isFolderPath)[0]
      || (getDemoFolderHistory() || []).filter(isFolderPath)[0]
      || (await defaultBrowseDir());
    if (startDir) setCurrentDir(startDir);
  })();
}

// ── Init / entry points ──────────────────────────────────────────────────────

// Registered once here (not per-load) to avoid the double-registration bug
// noted for render_status in ipc_bridge.js — analyzer_progress is throttled
// to ~30fps backend-side (Rust `analyze_demo_full`), so no further
// throttling is needed on the receiving end.
listen('analyzer_progress', (event) => {
  if (!analyzerLoadInProgress) return;
  const { processed, total } = event.payload || {};
  if (!total) return;
  const pct = Math.min(100, Math.round((processed / total) * 100));
  const titleEl = document.querySelector('#analyzer-current-file');
  const container = document.querySelector('#analyzer-tab-content');
  if (titleEl) titleEl.textContent = `Analyzing… ${pct}%`;
  if (container) container.innerHTML = `<p class="analyzer-empty">Analyzing demo… ${pct}%</p>`;
});

export function initAnalyzerPane({
  getPinnedFolders: getPinnedFoldersCb,
  pinFolder: pinFolderCb,
  unpinFolder: unpinFolderCb,
  getDemoFolderHistory: getDemoFolderHistoryCb,
  recordDemoFolderVisit: recordDemoFolderVisitCb,
  forgetDemoFolderVisit: forgetDemoFolderVisitCb,
  getScanFoldersForDemos: getScanFoldersForDemosCb,
  setScanFoldersForDemos: setScanFoldersForDemosCb,
} = {}) {
  if (getPinnedFoldersCb) getPinnedFolders = getPinnedFoldersCb;
  if (pinFolderCb) pinFolder = pinFolderCb;
  if (unpinFolderCb) unpinFolder = unpinFolderCb;
  if (getDemoFolderHistoryCb) getDemoFolderHistory = getDemoFolderHistoryCb;
  if (recordDemoFolderVisitCb) recordDemoFolderVisit = recordDemoFolderVisitCb;
  if (forgetDemoFolderVisitCb) forgetDemoFolderVisit = forgetDemoFolderVisitCb;
  if (getScanFoldersForDemosCb) getScanFoldersForDemos = getScanFoldersForDemosCb;
  if (setScanFoldersForDemosCb) setScanFoldersForDemos = setScanFoldersForDemosCb;

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

  initAnalyzerBrowser();
}

// Entry point for cross-pane jumps (e.g. Workspace's "View Match Telemetry"
// button) — points the Explorer sidebar at the demo's own folder first, so
// the Demos table reliably highlights it instead of only coincidentally
// matching whatever folder the sidebar was already browsing.
export async function openAnalyzerDemo(path) {
  const parentDir = parentDirOf(path);
  if (parentDir && parentDir !== currentDir) {
    await setCurrentDir(parentDir);
  }
  await loadAnalyzerDemo(path);
}

export async function loadAnalyzerDemo(path) {
  const container = document.querySelector('#analyzer-tab-content');
  const titleEl = document.querySelector('#analyzer-current-file');
  if (container) container.innerHTML = '<p class="analyzer-empty">Analyzing demo…</p>';
  if (titleEl) titleEl.textContent = 'Analyzing…';
  analyzerLoadInProgress = true;
  try {
    report = await analyzeDemoFull(path);
    highlightedPlayerId = null;
    selectedPlayerId = null;
    if (titleEl) titleEl.textContent = report.file_name;
    browserSelectedDemo = path;
    renderDemoTable();
    renderActiveTab();
  } catch (err) {
    if (container) {
      container.innerHTML = `<p class="analyzer-empty" style="color:#f44336;">Failed to analyze demo: ${esc(String(err))}</p>`;
    }
    if (titleEl) titleEl.textContent = '';
  } finally {
    analyzerLoadInProgress = false;
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
  const previousSelectedId = selectedPlayerId;
  let selectedId = selectedPlayerId;
  if (!selectedId || !players.find((p) => p.id === selectedId)) {
    selectedId = highlightedPlayerId && players.find((p) => p.id === highlightedPlayerId) ? highlightedPlayerId : players[0].id;
  }
  selectedPlayerId = selectedId;
  // Any change in the effective selected player (dropdown, scoreboard click,
  // or a kill-streak victim jump) clears the per-player weapon filter state.
  if (selectedId !== previousSelectedId) disabledWeapons = new Set();

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
    disabledWeapons = new Set();
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
          ${/^\d{15,20}$/.test(p.id) ? `<a href="https://www.legit-proof.com/search?q=${esc(steamId)}" target="_blank" rel="noopener" title="Search this player on Legit-Proof">Legit-Proof</a> / <a href="https://steamcommunity.com/profiles/${esc(p.id)}" target="_blank" rel="noopener">Steam Profile</a>` : '<span class="text-muted">No Steam ID</span>'}
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
        <div id="killstreak-weapon-filters" class="analyzer-weapon-filters"></div>
        <div id="killstreak-table-wrap"></div>
      </div>
    </div>`;

  renderKillStreaksSection(p);
}

function renderKillStreaksSection(p) {
  const filtersEl = document.querySelector('#killstreak-weapon-filters');
  const tableEl = document.querySelector('#killstreak-table-wrap');
  if (!filtersEl || !tableEl) return;

  const allWeapons = new Set();
  (p.kill_streaks || []).forEach((s) => (s.kills || []).forEach((k) => allWeapons.add(k[1])));

  renderWeaponFilterCategories(filtersEl, allWeapons, p);
  renderKillStreaksTable(tableEl, p);
}

function renderWeaponFilterCategories(el, allWeapons, p) {
  const categorized = new Set(WEAPON_CATEGORIES.flatMap(([, ws]) => ws));
  const other = [...allWeapons].filter((w) => !categorized.has(w)).sort((a, b) => weaponName(a).localeCompare(weaponName(b)));
  const groups = [...WEAPON_CATEGORIES, ...(other.length ? [['Other', other]] : [])]
    .map(([label, ws]) => [label, ws.filter((w) => allWeapons.has(w))])
    .filter(([, ws]) => ws.length > 0);

  if (groups.length === 0) { el.innerHTML = ''; return; }

  el.innerHTML = groups.map(([label, ws]) => {
    const allEnabled = ws.every((w) => !disabledWeapons.has(w));
    return `
      <div class="weapon-filter-group">
        <button type="button" class="weapon-filter-toggle-all" title="${allEnabled ? 'Hide all' : 'Show all'}" data-weapons="${esc(ws.join(','))}">[${esc(label)}]</button>
        ${ws.map((w) => `<label><input type="checkbox" class="weapon-filter-cb" data-weapon="${esc(w)}" ${!disabledWeapons.has(w) ? 'checked' : ''} /> ${esc(weaponName(w))}</label>`).join('')}
      </div>`;
  }).join('');

  el.querySelectorAll('.weapon-filter-toggle-all').forEach((btn) => {
    btn.addEventListener('click', () => {
      const ws = btn.dataset.weapons.split(',');
      const allEnabled = ws.every((w) => !disabledWeapons.has(w));
      ws.forEach((w) => (allEnabled ? disabledWeapons.add(w) : disabledWeapons.delete(w)));
      renderKillStreaksSection(p);
    });
  });
  el.querySelectorAll('.weapon-filter-cb').forEach((cb) => {
    cb.addEventListener('change', () => {
      const w = cb.dataset.weapon;
      if (cb.checked) disabledWeapons.delete(w); else disabledWeapons.add(w);
      renderKillStreaksSection(p);
    });
  });
}

function renderKillStreaksTable(container, p) {
  const streaks = (p.kill_streaks || [])
    .map((s) => ({ ...s, kills: (s.kills || []).filter((k) => !disabledWeapons.has(k[1])) }))
    .filter((s) => s.kills.length > 0);
  if (streaks.length === 0) {
    container.innerHTML = '<p class="analyzer-empty">No kill streaks recorded.</p>';
    return;
  }

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

  container.innerHTML = `
    <div class="table-wrapper" style="max-height:320px;">
      <table class="analyzer-table">
        <thead><tr><th>Wave</th><th>Kills</th><th>Time</th><th>Duration</th></tr></thead>
        <tbody>${rows}</tbody>
      </table>
    </div>`;

  container.querySelectorAll('.killstreak-victim').forEach((el) => {
    el.addEventListener('click', () => {
      const vid = el.dataset.victimId;
      if (!(report.state.players || []).find((pl) => pl.id === vid)) return;
      selectedPlayerId = vid;
      highlightedPlayerId = vid;
      renderPlayerDetailsTab(document.querySelector('#analyzer-tab-content'));
    });
  });
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
    ${renderTeamWeaponSections(players, st.allies_are_british)}`;
}

// Dev groups weapon breakdowns by the player's raw `team` value (Allies vs.
// British kept separate, not merged) so a mid-match side-label switch shows
// as two sections instead of silently combining into one.
function renderTeamWeaponSections(players, alliesAreBritish) {
  const alliesPlayers = players.filter((p) => p.team === 'Allies');
  const britishPlayers = players.filter((p) => p.team === 'British');
  const axisPlayers = players.filter((p) => p.team === 'Axis');

  const alliesColor = teamColor('Allies');
  const britishColor = teamColor('British');
  const axisColor = teamColor('Axis');

  const sections = [];
  // Matches dev exactly: show the Allies/US section unless there's British
  // data present with no pure-Allies data at all.
  if (alliesPlayers.length > 0 || britishPlayers.length === 0) {
    const label = alliesAreBritish ? 'Allies (US)' : 'Allies';
    sections.push(`<details open class="analyzer-collapsible"><summary style="color:${alliesColor};">${esc(label)}</summary>${weaponBreakdownTable(alliesPlayers)}</details>`);
  }
  if (britishPlayers.length > 0) {
    sections.push(`<details open class="analyzer-collapsible"><summary style="color:${britishColor};">British</summary>${weaponBreakdownTable(britishPlayers)}</details>`);
  }
  sections.push(`<details open class="analyzer-collapsible"><summary style="color:${axisColor};">Axis</summary>${weaponBreakdownTable(axisPlayers)}</details>`);
  return sections.join('');
}

function weaponBreakdownTable(arr) {
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
    <div style="position:relative;">
      <canvas id="analyzer-timeline-canvas" style="width:100%;height:340px;background:#121212;border:1px solid #333;border-radius:2px;"></canvas>
      <div id="analyzer-timeline-tooltip" class="analyzer-timeline-tooltip" style="display:none;"></div>
    </div>`;

  const timeline = (st.team_scores && st.team_scores.timeline) || [];
  const alliesSeries = timeline.filter(([, t]) => t === 'Allies' || t === 'British').map(([time, , score]) => [durSecs(time.viewdemo_offset), score]);
  const axisSeries = timeline.filter(([, t]) => t === 'Axis').map(([time, , score]) => [durSecs(time.viewdemo_offset), score]);

  const canvas = container.querySelector('#analyzer-timeline-canvas');
  const points = drawTimelineChart(canvas, alliesSeries, axisSeries, alliesColor, axisColor, alliesLabel, 'Axis');
  initTimelineTooltip(canvas, container.querySelector('#analyzer-timeline-tooltip'), points);
}

function drawTimelineChart(canvas, seriesA, seriesB, colorA, colorB, labelA, labelB) {
  if (!canvas) return [];
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
    return [];
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

  const hitPoints = [];
  const drawSeries = (series, color, label) => {
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
      const px = xAt(x), py = yAt(y);
      ctx.beginPath();
      ctx.arc(px, py, 2.5, 0, Math.PI * 2);
      ctx.fill();
      hitPoints.push({ px, py, x, y, color, label });
    });
  };
  drawSeries(seriesA, colorA, labelA);
  drawSeries(seriesB, colorB, labelB);
  return hitPoints;
}

// Dev's egui_plot had a built-in hover tooltip (label_formatter); the canvas
// reimplementation needs its own nearest-point hit test.
function initTimelineTooltip(canvas, tooltip, points) {
  if (!canvas || !tooltip || points.length === 0) return;
  const HIT_RADIUS_PX = 14;
  canvas.addEventListener('mousemove', (e) => {
    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    let nearest = null;
    let nearestDist = HIT_RADIUS_PX;
    for (const p of points) {
      const d = Math.hypot(p.px - mx, p.py - my);
      if (d < nearestDist) { nearest = p; nearestDist = d; }
    }
    if (nearest) {
      tooltip.style.display = 'block';
      tooltip.style.left = `${Math.min(nearest.px + 12, canvas.clientWidth - 90)}px`;
      tooltip.style.top = `${Math.max(nearest.py - 30, 0)}px`;
      tooltip.innerHTML = `${esc(formatMMSS(nearest.x))}<br><span style="color:${nearest.color};">${esc(nearest.label)}: ${nearest.y}</span>`;
    } else {
      tooltip.style.display = 'none';
    }
  });
  canvas.addEventListener('mouseleave', () => { tooltip.style.display = 'none'; });
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

// Categorizes a System chat message the same way dev's chat.rs does, from
// its raw system_token string — used to drive the 4 system-log checkboxes.
function systemMessageCategory(m) {
  if (!m.system_token) return 'other';
  const t = m.system_token.toLowerCase();
  if (t.includes('connect') || t.includes('join_game') || t.includes('joined_game') || t.includes('kick') || t.includes('disconnect')) return 'join_leave';
  if (t.includes('joined_team') || t.includes('team')) return 'team_change';
  if (t.includes('score') || t.includes('capture') || t.includes('cap') || t.includes('reinforce')) return 'gameplay';
  return 'other';
}

const SYSTEM_KEYWORD_STYLES = [
  { patterns: ['allies', 'allied'], color: '#228b22' },
  { patterns: ['axis'], color: '#b22222' },
  { patterns: ['spectators', 'spectator', 'spec'], color: '#dddd00' },
];

// Colors team-name keywords inline within an already-translated system
// message, matching dev's earliest-match scan in render_system_message.
function colorSystemMessage(text) {
  const baseColor = '#b4dcdc';
  let remainder = text;
  let out = '';
  while (remainder.length > 0) {
    const lower = remainder.toLowerCase();
    let earliest = null;
    for (const style of SYSTEM_KEYWORD_STYLES) {
      for (const pattern of style.patterns) {
        const idx = lower.indexOf(pattern);
        if (idx !== -1 && (!earliest || idx < earliest.idx)) {
          earliest = { idx, len: pattern.length, color: style.color };
        }
      }
    }
    if (!earliest) {
      out += `<span style="color:${baseColor};font-style:italic;">${esc(remainder)}</span>`;
      break;
    }
    if (earliest.idx > 0) {
      out += `<span style="color:${baseColor};font-style:italic;">${esc(remainder.slice(0, earliest.idx))}</span>`;
    }
    out += `<span style="color:${earliest.color};font-style:italic;">${esc(remainder.slice(earliest.idx, earliest.idx + earliest.len))}</span>`;
    remainder = remainder.slice(earliest.idx + earliest.len);
  }
  return out;
}

function renderChatTab(container) {
  container.innerHTML = `
    <h3 class="analyzer-heading">Chat &amp; System Log</h3>
    <div class="analyzer-toolbar" style="flex-wrap:wrap;gap:10px;">
      <button type="button" id="chat-select-all">Select All</button>
      <button type="button" id="chat-clear-all">Clear All</button>
    </div>
    <div class="analyzer-toolbar" style="flex-wrap:wrap;gap:10px;">
      <label><input type="checkbox" id="chat-filter-mm1" ${chatFilters.showMm1 ? 'checked' : ''}/> All Chat</label>
      <label><input type="checkbox" id="chat-filter-mm2" ${chatFilters.showMm2 ? 'checked' : ''}/> Team Chat</label>
      <span class="text-muted">|</span>
      <label><input type="radio" name="chat-status" id="chat-status-all" ${chatFilters.status === 'All' ? 'checked' : ''}/> All</label>
      <label><input type="radio" name="chat-status" id="chat-status-alive" ${chatFilters.status === 'Alive' ? 'checked' : ''}/> Alive</label>
      <label><input type="radio" name="chat-status" id="chat-status-dead" ${chatFilters.status === 'Dead' ? 'checked' : ''}/> Dead</label>
    </div>
    <div class="analyzer-toolbar" style="flex-wrap:wrap;gap:10px;">
      <label>Team:
        <select id="chat-filter-team">
          ${['All', 'Allies', 'British', 'Axis', 'Spectators'].map((t) => `<option value="${t}" ${chatFilters.team === t ? 'selected' : ''}>${t}</option>`).join('')}
        </select>
      </label>
    </div>
    <div class="analyzer-toolbar" style="flex-wrap:wrap;gap:10px;">
      <span class="text-muted">System Logs:</span>
      <label><input type="checkbox" id="chat-filter-joins" ${chatFilters.showJoins ? 'checked' : ''}/> Joins/Leaves</label>
      <label><input type="checkbox" id="chat-filter-teams" ${chatFilters.showTeams ? 'checked' : ''}/> Team Changes</label>
      <label><input type="checkbox" id="chat-filter-gameplay" ${chatFilters.showGameplay ? 'checked' : ''}/> Gameplay</label>
      <label><input type="checkbox" id="chat-filter-othersys" ${chatFilters.showOtherSys ? 'checked' : ''}/> Other System</label>
      <input type="text" id="chat-filter-search" placeholder="Search sender or text..." value="${esc(chatFilters.search)}" style="flex:1;min-width:150px;" />
    </div>
    <div id="chat-log-list" class="analyzer-chat-log"></div>`;

  container.querySelector('#chat-select-all').addEventListener('click', () => {
    Object.assign(chatFilters, { showMm1: true, showMm2: true, status: 'All', team: 'All', showJoins: true, showTeams: true, showGameplay: true, showOtherSys: true });
    renderChatTab(container);
  });
  container.querySelector('#chat-clear-all').addEventListener('click', () => {
    Object.assign(chatFilters, { showMm1: false, showMm2: false, showJoins: false, showTeams: false, showGameplay: false, showOtherSys: false });
    renderChatTab(container);
  });

  container.querySelector('#chat-filter-mm1').addEventListener('change', (e) => { chatFilters.showMm1 = e.target.checked; renderChatLogList(); });
  container.querySelector('#chat-filter-mm2').addEventListener('change', (e) => { chatFilters.showMm2 = e.target.checked; renderChatLogList(); });
  container.querySelector('#chat-status-all').addEventListener('change', () => { chatFilters.status = 'All'; renderChatLogList(); });
  container.querySelector('#chat-status-alive').addEventListener('change', () => { chatFilters.status = 'Alive'; renderChatLogList(); });
  container.querySelector('#chat-status-dead').addEventListener('change', () => { chatFilters.status = 'Dead'; renderChatLogList(); });
  container.querySelector('#chat-filter-team').addEventListener('change', (e) => { chatFilters.team = e.target.value; renderChatLogList(); });
  container.querySelector('#chat-filter-joins').addEventListener('change', (e) => { chatFilters.showJoins = e.target.checked; renderChatLogList(); });
  container.querySelector('#chat-filter-teams').addEventListener('change', (e) => { chatFilters.showTeams = e.target.checked; renderChatLogList(); });
  container.querySelector('#chat-filter-gameplay').addEventListener('change', (e) => { chatFilters.showGameplay = e.target.checked; renderChatLogList(); });
  container.querySelector('#chat-filter-othersys').addEventListener('change', (e) => { chatFilters.showOtherSys = e.target.checked; renderChatLogList(); });
  container.querySelector('#chat-filter-search').addEventListener('input', (e) => { chatFilters.search = e.target.value; renderChatLogList(); });

  renderChatLogList();
}

function renderChatLogList() {
  const listEl = document.querySelector('#chat-log-list');
  if (!listEl) return;
  const messages = report.state.chat_messages || [];
  const search = chatFilters.search.toLowerCase();

  const filtered = messages.filter((m) => {
    if (m.chat_type === 'System') {
      const cat = systemMessageCategory(m);
      if (cat === 'join_leave' && !chatFilters.showJoins) return false;
      if (cat === 'team_change' && !chatFilters.showTeams) return false;
      if (cat === 'gameplay' && !chatFilters.showGameplay) return false;
      if (cat === 'other' && !chatFilters.showOtherSys) return false;
    } else {
      if (m.chat_type === 'Mm1' && !chatFilters.showMm1) return false;
      if (m.chat_type === 'Mm2' && !chatFilters.showMm2) return false;
      if (chatFilters.status === 'Alive' && m.sender_dead) return false;
      if (chatFilters.status === 'Dead' && !m.sender_dead) return false;
      if (chatFilters.team !== 'All') {
        const t = m.sender_team;
        const matchesTeam = t === chatFilters.team
          || (chatFilters.team === 'Allies' && t === 'British')
          || (chatFilters.team === 'British' && t === 'Allies');
        if (!matchesTeam) return false;
      }
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
      body = `<span style="color:#c89650;">[system]</span> ${colorSystemMessage(m.text)}`;
    } else {
      const teamBadge = m.chat_type === 'Mm2' ? `<span style="color:${teamColor(m.sender_team)};">(Team)</span> ` : '';
      body = `${teamBadge}<span style="color:${teamColor(m.sender_team)};font-weight:600;">${esc(m.sender_name || 'Unknown')}:</span> ${esc(m.text)}`;
    }
    return `<div class="analyzer-chat-row"><span class="chat-timestamp">[${time}]</span> ${deadBadge}${body}</div>`;
  }).join('');
}
