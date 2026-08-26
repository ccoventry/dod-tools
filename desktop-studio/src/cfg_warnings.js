// cfg_warnings.js
// Tells the user what their own config files set, and what the app will
// override in them.
//
// Two different problems, both invisible without this:
//
//   1. A config sets something the pipeline reads and the app never hears about
//      it. That is how a capture ran at `mirv_fov 105` from movie.cfg while the
//      flush sized its on-screen test for the default 90.
//   2. An init command sets the same cvar a config does. The init command runs
//      later, so it wins — which is the point of typing it, but it also means a
//      line the user set deliberately, possibly years ago, quietly stops
//      applying. Including the commands the app adds for itself: the capture
//      fps overrides a `mirv_movie_fps` in movie.cfg, and the decal pin
//      overrides an `r_decals` there.
//
// ADVISORY ONLY. Nothing here, or anywhere in this app, writes to a config file.

import { scanGameConfigs } from './ipc_bridge.js';
import { STRINGS } from './strings.js';

let report = { unseen: [], overrides: [] };

function bannerEl() {
  return document.querySelector('#cfg-warning-banner');
}

/**
 * Re-scan and redraw. Safe to call repeatedly — on start-up, when the hl.exe
 * path changes, and whenever the init command list is edited.
 *
 * `context` carries the settings that decide what the app appends for itself,
 * so the overrides it reports are the ones a capture would really apply.
 */
export async function refreshCfgWarnings(gamePath, initCommands = [], context = {}) {
  report = gamePath
    ? await scanGameConfigs(gamePath, initCommands, context)
    : { unseen: [], overrides: [] };
  render();
}

function section(title, advice, rows) {
  return `
    <div style="margin-bottom:8px;">
      <strong>${title}</strong>
      <ul style="margin:6px 0 6px 18px; padding:0;">${rows}</ul>
      <div style="opacity:.8">${advice}</div>
    </div>`;
}

function render() {
  const el = bannerEl();
  if (!el) return;

  const unseen = report?.unseen ?? [];
  const overrides = report?.overrides ?? [];

  if (unseen.length === 0 && overrides.length === 0) {
    el.hidden = true;
    el.innerHTML = '';
    return;
  }

  el.hidden = false;
  el.style.cssText =
    'margin: 0 0 8px; padding: 10px 12px; border: 1px solid #b58900; ' +
    'border-radius: 4px; background: #2a2410; color: #e8dcb0; font-size: 12px;';

  let html = '';

  if (unseen.length > 0) {
    const rows = unseen
      .map(
        (f) =>
          `<li><code>${f.cvar} ${f.value}</code> — ${STRINGS.CFG.location(f.file, f.line)}</li>`
      )
      .join('');
    html += section(STRINGS.CFG.BANNER_TITLE, STRINGS.CFG.ADVICE, rows);
  }

  if (overrides.length > 0) {
    const rows = overrides
      .map((o) => {
        const note = o.fromApp
          ? ` <span style="opacity:.7">(${STRINGS.CFG.FROM_APP_NOTE})</span>`
          : '';
        return `<li><code>${STRINGS.CFG.override(o.cvar, o.initValue, o.cfgValue, o.file, o.line)}</code>${note}</li>`;
      })
      .join('');
    html += section(STRINGS.CFG.OVERRIDE_TITLE, STRINGS.CFG.OVERRIDE_ADVICE, rows);
  }

  el.innerHTML = html;
}
