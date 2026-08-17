// nav.js — shared top-nav tab switching. Extracted from main.js so panes that
// need to jump to a different tab programmatically (e.g. detail_pane.js's
// "View Match Telemetry" button jumping to the Demo Analyzer tab) can call it
// without creating a circular import with main.js.

// In-workflow Capture Studio phase — dev's SelectTab::Highlights/Configuration
// live inside the single continuous Capture Studio destination, not as a
// separate top-level nav tab ("Batch Capture Config" was DRIFT, see
// docs/tauri_parity_audit.md Area 1). 'highlights' shows the Master Queue's
// detail view + advanced diagnostics; 'configuration' shows Path
// Routing/Timing/Capture Output/Custom Commands.
let activeCaptureDetailSubtab = 'highlights';

function applyCaptureDetailSubtabDisplay() {
  const detailPane = document.querySelector('#detail-pane');
  const advancedPanel = document.querySelector('#advanced-diagnostics-details');
  const exportPanel = document.querySelector('#export-config-panel');
  const showHighlights = activeCaptureDetailSubtab === 'highlights';

  if (detailPane) detailPane.style.display = showHighlights ? 'block' : 'none';
  if (advancedPanel) advancedPanel.style.display = showHighlights ? 'block' : 'none';
  if (exportPanel) exportPanel.style.display = showHighlights ? 'none' : 'block';

  document.querySelectorAll('.capture-detail-subtab-btn').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.captureSubtab === activeCaptureDetailSubtab);
  });
}

export function setCaptureDetailSubtab(subtab) {
  activeCaptureDetailSubtab = subtab;
  applyCaptureDetailSubtabDisplay();
}

export function getCaptureDetailSubtab() {
  return activeCaptureDetailSubtab;
}

export function switchNavTab(navKey) {
  const workspacePane = document.querySelector('#pane-workspace');
  const detailsPane = document.querySelector('#pane-details-config');
  const captureSubtabsBar = document.querySelector('#capture-detail-subtabs');
  const detailPane = document.querySelector('#detail-pane');
  const advancedPanel = document.querySelector('#advanced-diagnostics-details');
  const exportPanel = document.querySelector('#export-config-panel');
  const renderPanel = document.querySelector('#render-studio-panel');
  const auditorPane = document.querySelector('#pane-demo-auditor');
  const analyzerPane = document.querySelector('#pane-demo-analyzer');

  [workspacePane, detailsPane, captureSubtabsBar, detailPane, advancedPanel, exportPanel, renderPanel, auditorPane, analyzerPane]
    .forEach((el) => { if (el) el.style.display = 'none'; });

  if (navKey === 'workspace') {
    if (workspacePane) workspacePane.style.display = 'flex';
    if (detailsPane) detailsPane.style.display = 'flex';
    if (captureSubtabsBar) captureSubtabsBar.style.display = 'flex';
    applyCaptureDetailSubtabDisplay();
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
