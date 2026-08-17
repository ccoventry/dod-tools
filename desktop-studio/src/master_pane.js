// master_pane.js
// Renders the Master Demo Queue table with live per-demo status columns,
// selected-streak yield counts, and a functional delete action (M1-M4 parity).

let currentDemos = [];
let currentOnSelectDemo = null;
let currentOnDeleteDemo = null;
let currentSearchTerm = '';

export function initMasterPane(onDeleteDemo) {
  if (onDeleteDemo) {
    currentOnDeleteDemo = onDeleteDemo;
  }

  const searchInput =
    document.querySelector('#demo-search-input') ||
    document.querySelector('#demo-search-filter');

  if (searchInput) {
    searchInput.addEventListener('input', (e) => {
      currentSearchTerm = (e.target.value || '').toLowerCase().trim();
      renderMasterList(currentDemos, null, currentOnSelectDemo);
    });
  }

  window.addEventListener('keydown', (e) => {
    const activeTab = document.querySelector('.nav-tab-btn.active')?.dataset.nav;
    if (activeTab !== 'workspace') return;

    const rows = Array.from(document.querySelectorAll('#master-demo-table-body tr'));
    if (!rows.length || rows[0].querySelector('.table-empty')) return;

    let currentIndex = rows.findIndex(r => r.classList.contains('keyboard-selected'));
    
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (currentIndex < rows.length - 1) {
        if (currentIndex >= 0) rows[currentIndex].classList.remove('keyboard-selected');
        currentIndex++;
        rows[currentIndex].classList.add('keyboard-selected');
        rows[currentIndex].scrollIntoView({ block: 'nearest' });
      }
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (currentIndex > 0) {
        rows[currentIndex].classList.remove('keyboard-selected');
        currentIndex--;
        rows[currentIndex].classList.add('keyboard-selected');
        rows[currentIndex].scrollIntoView({ block: 'nearest' });
      }
    } else if (e.key === 'Enter') {
      if (currentIndex >= 0 && currentIndex < rows.length) {
        e.preventDefault();
        rows[currentIndex].click();
      }
    }
  });
}

// ── Status helpers ────────────────────────────────────────────────────────────

/**
 * Streaks belong to whichever player got the kills, not just the demo's
 * recording player — `demo.streaks` covers every player in the match. The
 * Highlight Details table (detail_pane.js) filters down to the recording
 * player's own streaks before displaying rows; mirror that same filter here
 * so the queue's counts agree with what the table actually shows instead of
 * summing every player in the match.
 *
 * Gate on whether local_player_index actually resolved, not on demo.is_pov
 * — is_pov reflects any SvcHltv/SvcDirector message anywhere in the file,
 * which also fires on an ordinary player-recorded demo whenever an HLTV
 * caster was merely spectating the live match (server-broadcast messages
 * every connected client picks up), so it's not a reliable "no single
 * owner" signal. True HLTV proxy files are already rejected earlier in the
 * pipeline (scan_demo_for_highlights), so None here means "no resolvable
 * owner", not "is_pov".
 */
function recordingPlayerStreaks(demo) {
  const streaks = demo.streaks || [];
  const recPlayer = demo.local_player_index;
  if (recPlayer === null || recPlayer === undefined) return streaks;

  return streaks.filter((s) => s.player_index === recPlayer);
}

/** Count streaks matching a given status string. */
function countByStatus(streaks, status) {
  return (streaks || []).filter((s) => (s.status || 'Pending') === status).length;
}

// ── Render ───────────────────────────────────────────────────────────────────

export function renderMasterList(demos, selectedDemoIdx, onSelectDemo) {
  if (demos !== currentDemos) {
    currentDemos = demos || [];
  }
  if (onSelectDemo) {
    currentOnSelectDemo = onSelectDemo;
  }

  const tableBody = document.querySelector('#master-demo-table-body');
  if (!tableBody) return;
  tableBody.innerHTML = '';

  if (!currentDemos || currentDemos.length === 0) {
    tableBody.innerHTML =
      '<tr><td colspan="6" class="table-empty">No demos found in specified directories.</td></tr>';
    if (currentOnSelectDemo) currentOnSelectDemo(null, null);
    return;
  }

  const searchInput = document.querySelector('#demo-search-input');
  if (searchInput && searchInput.value !== undefined) {
    currentSearchTerm = searchInput.value.toLowerCase().trim();
  }

  const filteredDemos = currentDemos.filter((demo) => {
    if (!currentSearchTerm) return true;
    const matchName = demo.name && demo.name.toLowerCase().includes(currentSearchTerm);
    const matchPath = demo.path && demo.path.toLowerCase().includes(currentSearchTerm);
    const matchMap  = demo.map_name && demo.map_name.toLowerCase().includes(currentSearchTerm);
    return matchName || matchPath || matchMap;
  });

  if (filteredDemos.length === 0) {
    tableBody.innerHTML =
      '<tr><td colspan="6" class="table-empty">No demos match your search.</td></tr>';
    return;
  }

  filteredDemos.forEach((demo) => {
    const originalIdx = currentDemos.indexOf(demo);
    // Highlights/Pending/Captured/Rendered all count only the recording
    // player's own streaks, matching the Highlight Details table's rows.
    const ownStreaks = recordingPlayerStreaks(demo);

    // ── Derive live column values ─────────────────────────────────────────
    const pending     = countByStatus(ownStreaks, 'Pending');    // M4
    const captured    = countByStatus(ownStreaks, 'Captured');   // M4
    const rendered    = countByStatus(ownStreaks, 'Rendered');   // M4

    const tr = document.createElement('tr');
    tr.style.borderBottom = '1px solid #333';
    tr.style.cursor = 'pointer';

    if (selectedDemoIdx === originalIdx) {
      tr.classList.add('table-row-selected');
    }

    // ── Build cells ───────────────────────────────────────────────────────
    // Col 1: Demo name
    const tdName = document.createElement('td');
    tdName.style.padding = '6px 8px';
    tdName.style.fontWeight = 'bold';
    tdName.style.maxWidth = '200px';
    tdName.style.overflow = 'hidden';
    tdName.style.textOverflow = 'ellipsis';
    tdName.style.whiteSpace = 'nowrap';
    tdName.title = demo.name || '';
    tdName.textContent = demo.name || '—';

    // Col 2: Highlights (total streak count, recording player only)  [M2]
    const tdYield = document.createElement('td');
    tdYield.style.padding = '6px 8px';
    tdYield.style.textAlign = 'center';
    tdYield.textContent = `${ownStreaks.length}`;

    // Col 3: Pending count   [M4]
    const tdPending = document.createElement('td');
    tdPending.style.padding = '6px 8px';
    tdPending.style.textAlign = 'center';
    tdPending.style.color = pending > 0 ? '#ffa726' : '#555';
    tdPending.textContent = pending;

    // Col 4: Captured count  [M4]
    const tdCaptured = document.createElement('td');
    tdCaptured.style.padding = '6px 8px';
    tdCaptured.style.textAlign = 'center';
    tdCaptured.style.color = captured > 0 ? '#4caf50' : '#555';
    tdCaptured.textContent = captured;

    // Col 5: Rendered count  [M4]
    const tdRendered = document.createElement('td');
    tdRendered.style.padding = '6px 8px';
    tdRendered.style.textAlign = 'center';
    tdRendered.style.color = rendered > 0 ? '#2196f3' : '#555';
    tdRendered.textContent = rendered;

    // Col 6: Actions — remove-from-queue only, no status badge  [M3]
    const tdActions = document.createElement('td');
    tdActions.style.padding = '6px 8px';
    tdActions.style.textAlign = 'center';
    tdActions.style.whiteSpace = 'nowrap';

    const deleteBtn = document.createElement('button');
    deleteBtn.textContent = '🗑';
    deleteBtn.title = 'Remove demo from queue';
    deleteBtn.style.padding = '1px 5px';
    deleteBtn.style.fontSize = '11px';
    deleteBtn.style.background = 'transparent';
    deleteBtn.style.border = '1px solid #555';
    deleteBtn.style.borderRadius = '2px';
    deleteBtn.style.cursor = 'pointer';
    deleteBtn.style.color = '#aaa';
    deleteBtn.addEventListener('click', (e) => {
      e.stopPropagation(); // do not select the row when deleting
      currentDemos.splice(originalIdx, 1);
      if (currentOnDeleteDemo) currentOnDeleteDemo(originalIdx, currentDemos);
      // Re-render: no demo is now "selected" at the old index
      renderMasterList(currentDemos, null, currentOnSelectDemo);
    });
    tdActions.appendChild(deleteBtn);

    tr.appendChild(tdName);
    tr.appendChild(tdYield);
    tr.appendChild(tdPending);
    tr.appendChild(tdCaptured);
    tr.appendChild(tdRendered);
    tr.appendChild(tdActions);

    // Row click selects the demo (but not when clicking the delete btn)
    tr.addEventListener('click', () => {
      const allRows = tableBody.querySelectorAll('tr');
      allRows.forEach((r) => r.classList.remove('table-row-selected'));
      tr.classList.add('table-row-selected');
      if (currentOnSelectDemo) currentOnSelectDemo(demo, originalIdx);
    });

    tableBody.appendChild(tr);
  });
}
