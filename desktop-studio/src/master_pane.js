let currentDemos = [];
let currentOnSelectDemo = null;
let currentSearchTerm = "";

export function initMasterPane() {
  const searchInput = document.querySelector('#demo-search-input') || document.querySelector('#demo-search-filter');
  if (searchInput) {
    searchInput.addEventListener('input', (e) => {
      currentSearchTerm = e.target.value.toLowerCase();
      renderMasterList(currentDemos, null, currentOnSelectDemo);
    });
  }
}

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
    tableBody.innerHTML = '<tr><td colspan="6" style="padding: 12px; text-align: center; color: #888;">No demos found in specified directories.</td></tr>';
    if (currentOnSelectDemo) currentOnSelectDemo(null, null);
    return;
  }

  const filteredDemos = currentDemos.filter(demo => {
    if (!currentSearchTerm) return true;
    return (demo.name && demo.name.toLowerCase().includes(currentSearchTerm)) ||
           (demo.map_name && demo.map_name.toLowerCase().includes(currentSearchTerm));
  });

  if (filteredDemos.length === 0) {
    tableBody.innerHTML = '<tr><td colspan="6" style="padding: 12px; text-align: center; color: #888;">No demos match your search.</td></tr>';
    return;
  }

  filteredDemos.forEach((demo, filteredIdx) => {
    // Find original index for selection state
    const originalIdx = currentDemos.indexOf(demo);
    const tr = document.createElement('tr');
    tr.style.borderBottom = '1px solid #333';
    tr.style.cursor = 'pointer';
    if (selectedDemoIdx === originalIdx) {
      tr.classList.add('table-row-selected');
      tr.style.background = 'rgba(255, 255, 255, 0.1)';
    }

    tr.innerHTML = `
      <td style="padding: 8px; font-weight: bold;">${demo.name}</td>
      <td style="padding: 8px;">${demo.streaks ? demo.streaks.length : 0} streaks</td>
      <td style="padding: 8px; cursor: pointer;" title="Scanned / Remove">Scanned 🗑️</td>
      <td style="padding: 8px;">0</td>
      <td style="padding: 8px;">0</td>
      <td style="padding: 8px;">0</td>
    `;

    tr.addEventListener('click', () => {
      const allRows = tableBody.querySelectorAll('tr');
      allRows.forEach(r => {
        r.classList.remove('table-row-selected');
        r.style.background = '';
      });
      tr.classList.add('table-row-selected');
      tr.style.background = 'rgba(255, 255, 255, 0.1)';
      if (currentOnSelectDemo) currentOnSelectDemo(demo, originalIdx);
    });

    tableBody.appendChild(tr);
  });
}
