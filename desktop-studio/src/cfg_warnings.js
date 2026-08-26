// cfg_warnings.js
// Tells the user when the game's own config files set something this app reads.
//
// The case that prompted it: a `config.cfg` ending in `exec movie.cfg`, and a
// `movie.cfg` carrying `mirv_fov 105`. The engine renders at 105; the app, which
// only ever looked at its own Init Commands, sized the decal flush's on-screen
// test for the default 90 — a cone some seven degrees too narrow. Nothing on
// screen says so.
//
// ADVISORY ONLY. Nothing here, or anywhere in this app, writes to a config file.
// Those are the user's. The banner says what was found and leaves the decision
// where it belongs.

import { scanGameConfigs } from './ipc_bridge.js';
import { STRINGS } from './strings.js';

let findings = [];

function bannerEl() {
  return document.querySelector('#cfg-warning-banner');
}

/** Re-scan the configured game folder and redraw. Safe to call repeatedly. */
export async function refreshCfgWarnings(gamePath) {
  findings = gamePath ? await scanGameConfigs(gamePath) : [];
  render();
}

function render() {
  const el = bannerEl();
  if (!el) return;

  if (!Array.isArray(findings) || findings.length === 0) {
    el.hidden = true;
    el.innerHTML = '';
    return;
  }

  el.hidden = false;
  el.style.cssText =
    'margin: 0 0 8px; padding: 10px 12px; border: 1px solid #b58900; ' +
    'border-radius: 4px; background: #2a2410; color: #e8dcb0; font-size: 12px;';

  const rows = findings
    .map(
      (f) =>
        `<li><code>${f.cvar} ${f.value}</code> — ${STRINGS.CFG.location(f.file, f.line)}</li>`
    )
    .join('');

  el.innerHTML = `
    <strong>${STRINGS.CFG.BANNER_TITLE}</strong>
    <ul style="margin:6px 0 6px 18px; padding:0;">${rows}</ul>
    <div style="opacity:.8">${STRINGS.CFG.ADVICE}</div>`;
}
