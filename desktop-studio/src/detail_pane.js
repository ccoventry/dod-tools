export function renderDetailView(demo, selectedDemoIdx) {
  const titleEl = document.querySelector('#detail-demo-title');
  const container = document.querySelector('#detail-streaks-container');
  if (!titleEl || !container) return;

  if (!demo) {
    titleEl.textContent = 'Highlight Details (Select a Demo)';
    container.innerHTML = '<p style="color: #888;">Select a demo in the Master List to view its killstreak details.</p>';
    return;
  }

  const hideNonPov = document.querySelector('#config-hide-non-pov')?.checked || false;
  titleEl.textContent = `Highlight Details: ${demo.name}`;
  container.innerHTML = '';

  if (!demo.streaks || demo.streaks.length === 0) {
    container.innerHTML = '<p style="color: #888;">No killstreak highlights detected in this demo.</p>';
    return;
  }

  demo.streaks.forEach((streak, streakIdx) => {
    if (hideNonPov && !demo.is_pov && streak.player_index !== demo.local_player_index) {
      return;
    }

    const card = document.createElement('div');
    card.className = 'egui-card';
    card.style.display = 'flex';
    card.style.alignItems = 'center';
    card.style.gap = '12px';

    const checkbox = document.createElement('input');
    checkbox.type = 'checkbox';
    checkbox.checked = true;
    checkbox.dataset.demoIdx = selectedDemoIdx !== null ? selectedDemoIdx : 0;
    checkbox.dataset.streakIdx = streakIdx;

    const label = document.createElement('label');
    label.style.flex = '1';
    label.innerHTML = `<strong>${streak.kill_count} Kills</strong> (${streak.target_player || 'Player ' + streak.player_index}) &nbsp;|&nbsp; <em>${streak.timeline_string}</em> &nbsp;|&nbsp; <span style="font-family: monospace; color: #888;">Ticks: ${streak.start_tick} - ${streak.end_tick}</span>`;

    card.appendChild(checkbox);
    card.appendChild(label);
    container.appendChild(card);
  });
}
