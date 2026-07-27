export function renderMasterList(demos, selectedDemoIdx, onSelectDemo) {
  const tableBody = document.querySelector('#master-demo-table-body');
  if (!tableBody) return;
  tableBody.innerHTML = '';

  if (!demos || demos.length === 0) {
    tableBody.innerHTML = '<tr><td colspan="6" style="padding: 12px; text-align: center; color: #888;">No demos found in specified directories.</td></tr>';
    if (onSelectDemo) onSelectDemo(null, null);
    return;
  }

  demos.forEach((demo, idx) => {
    const tr = document.createElement('tr');
    tr.style.borderBottom = '1px solid #333';
    tr.style.cursor = 'pointer';
    if (selectedDemoIdx === idx) {
      tr.classList.add('table-row-selected');
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
      const allRows = tableBody.querySelectorAll('tr');
      allRows.forEach(r => {
        r.classList.remove('table-row-selected');
        r.style.background = '';
      });
      tr.classList.add('table-row-selected');
      tr.style.background = 'rgba(255, 255, 255, 0.1)';
      if (onSelectDemo) onSelectDemo(demo, idx);
    });

    tableBody.appendChild(tr);
  });
}
