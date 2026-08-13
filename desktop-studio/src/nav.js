// nav.js — shared top-nav tab switching. Extracted from main.js so panes that
// need to jump to a different tab programmatically (e.g. detail_pane.js's
// "View Match Telemetry" button jumping to the Demo Analyzer tab) can call it
// without creating a circular import with main.js.

export function switchNavTab(navKey) {
  const workspacePane = document.querySelector('#pane-workspace');
  const detailsPane = document.querySelector('#pane-details-config');
  const detailPane = document.querySelector('#detail-pane');
  const advancedPanel = document.querySelector('#advanced-diagnostics-details');
  const exportPanel = document.querySelector('#export-config-panel');
  const renderPanel = document.querySelector('#render-studio-panel');
  const auditorPane = document.querySelector('#pane-demo-auditor');
  const analyzerPane = document.querySelector('#pane-demo-analyzer');

  [workspacePane, detailsPane, detailPane, advancedPanel, exportPanel, renderPanel, auditorPane, analyzerPane]
    .forEach((el) => { if (el) el.style.display = 'none'; });

  if (navKey === 'workspace') {
    if (workspacePane) workspacePane.style.display = 'flex';
    if (detailsPane) detailsPane.style.display = 'flex';
    if (detailPane) detailPane.style.display = 'block';
    if (advancedPanel) advancedPanel.style.display = 'block';
  } else if (navKey === 'export-config') {
    if (detailsPane) detailsPane.style.display = 'flex';
    if (exportPanel) exportPanel.style.display = 'block';
  } else if (navKey === 'render-studio') {
    if (detailsPane) detailsPane.style.display = 'flex';
    if (renderPanel) renderPanel.style.display = 'block';
  } else if (navKey === 'demo-auditor') {
    if (auditorPane) auditorPane.style.display = 'flex';
  } else if (navKey === 'demo-analyzer') {
    if (analyzerPane) analyzerPane.style.display = 'flex';
  }

  document.querySelectorAll('.nav-tab-btn').forEach((b) => {
    b.classList.toggle('active', b.dataset.nav === navKey);
  });
}
