import { analyzeDemo } from './ipc_bridge.js';

export function setupTelemetryUI() {
  const closeBtn = document.querySelector('#telemetry-modal-close-btn');
  const modal = document.querySelector('#telemetry-modal');
  if (closeBtn && modal) {
    closeBtn.addEventListener('click', () => {
      modal.style.display = 'none';
    });
  }
}

export function initTelemetryPane() {
  setupTelemetryUI();
}

export function renderTelemetry(data) {
  const container = document.getElementById("telemetry-container");
  if (!container) return;

  if (!data) {
    container.innerHTML = '<p style="color: #888;">Select a demo to view telemetry data.</p>';
    return;
  }

  const formattedJson = JSON.stringify(data, null, 2);
  container.innerHTML = `
    <pre style="color: #dcdcaa; font-family: monospace; font-size: 12px; margin: 0;">${formattedJson}</pre>
  `;
}

function renderTableFromData(data) {
  if (!data || (Array.isArray(data) && data.length === 0)) {
    return '<p style="color: #888; font-style: italic;">No data available.</p>';
  }
  if (typeof data === 'object' && !Array.isArray(data)) {
    return `<pre style="color: #dcdcaa; font-family: monospace; font-size: 12px; margin: 0;">${JSON.stringify(data, null, 2)}</pre>`;
  }
  if (Array.isArray(data)) {
    if (typeof data[0] === 'string') {
      return `<ul style="margin: 0; padding-left: 20px;">${data.map(item => `<li>${item}</li>`).join('')}</ul>`;
    }
    const keys = Object.keys(data[0]);
    let html = '<table style="width: 100%; border-collapse: collapse; text-align: left; font-size: 13px;"><thead><tr style="border-bottom: 1px solid #444;">';
    keys.forEach(k => { html += `<th style="padding: 6px;">${k}</th>`; });
    html += '</tr></thead><tbody>';
    data.forEach(row => {
      html += '<tr style="border-bottom: 1px solid #333;">';
      keys.forEach(k => {
        const val = typeof row[k] === 'object' ? JSON.stringify(row[k]) : row[k];
        html += `<td style="padding: 6px;">${val ?? ''}</td>`;
      });
      html += '</tr>';
    });
    html += '</tbody></table>';
    return html;
  }
  return String(data);
}

export async function loadAndShowTelemetry(demoPath) {
  const modal = document.querySelector('#telemetry-modal');
  const loadingEl = document.querySelector('#telemetry-loading-state');
  const contentContainer = document.querySelector('#telemetry-content-container');
  const scoreboardBox = document.querySelector('#telemetry-scoreboard .telemetry-data-box');
  const chatBox = document.querySelector('#telemetry-chat-logs .telemetry-data-box');
  const mortalityBox = document.querySelector('#telemetry-mortality-metrics .telemetry-data-box');

  if (modal) modal.style.display = 'flex';
  if (loadingEl) loadingEl.style.display = 'block';
  if (contentContainer) contentContainer.style.display = 'none';

  try {
    const analysis = await analyzeDemo(demoPath);
    if (loadingEl) loadingEl.style.display = 'none';
    if (contentContainer) contentContainer.style.display = 'block';

    if (scoreboardBox) scoreboardBox.innerHTML = renderTableFromData(analysis?.scoreboard);
    if (chatBox) chatBox.innerHTML = renderTableFromData(analysis?.chat_logs);
    if (mortalityBox) mortalityBox.innerHTML = renderTableFromData(analysis?.mortality_metrics);
  } catch (err) {
    console.error("Telemetry analysis error:", err);
    if (loadingEl) loadingEl.style.display = 'none';
    if (contentContainer) {
      contentContainer.style.display = 'block';
      contentContainer.innerHTML = `<div style="color: #f44336; padding: 15px; background: #2a1515; border-radius: 4px;">Failed to load telemetry: ${err}</div>`;
    }
  }
}
