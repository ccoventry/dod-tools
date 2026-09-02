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

const EMPTY = { unseen: [], overrides: [], shadowed: [], custom: [], launchConfigMissing: null };

let report = EMPTY;

/**
 * Re-scan and redraw. Safe to call repeatedly — on start-up, when the hl.exe
 * path changes, and whenever the init command list is edited.
 *
 * `context` carries the settings that decide what the app appends for itself,
 * so the overrides it reports are the ones a capture would really apply.
 *
 * `customCommands` are `{command, relation, offset_seconds}` rather than bare
 * strings, because the order the engine reaches them decides which one is
 * actually displacing a config value and which is just changing it again.
 */
export async function refreshCfgWarnings(
  gamePath,
  initCommands = [],
  customCommands = [],
  context = {}
) {
  report = gamePath
    ? await scanGameConfigs(gamePath, initCommands, customCommands, context)
    : EMPTY;
  render();
}

/** The value half of `cvar value`, for rows that name the cvar separately. */
function valueOf(command) {
  return String(command).trim().split(/\s+/)[1] ?? '';
}

function section(title, advice, rows, accent) {
  const style = accent ? ` style="color:${accent}"` : '';
  return `
      <strong${style}>${title}</strong>
      <ul style="margin:6px 0 6px 18px; padding:0;">${rows}</ul>
      <div style="opacity:.8">${advice}</div>`;
}

/**
 * Wraps each section from `section()` and separates them with a divider —
 * except the last, which gets none. Done here rather than via a CSS
 * `:last-child` rule because each wrapper needs its own inline
 * `border-bottom`, and an inline style always beats a stylesheet selector
 * regardless of specificity, so a CSS-only "remove it on the last one"
 * rule can never actually take effect.
 */
function joinSections(parts) {
  return parts
    .map((html, i) => {
      const style =
        i === parts.length - 1
          ? ''
          : ' style="margin-bottom:16px; padding-bottom:16px; border-bottom:1px solid rgba(255,255,255,.12);"';
      return `<div${style}>${html}</div>`;
    })
    .join('');
}

/**
 * Fills one of the three per-field banners and shows/hides it independently
 * of the other two — each field's warnings sit directly under that field
 * (same reasoning as `.path-warning` in Path Routing) rather than all piling
 * into one banner under Initial Commands regardless of which field they're
 * actually about.
 */
function renderInto(elId, sectionsHtml) {
  const el = document.querySelector(elId);
  if (!el) return;

  if (!sectionsHtml) {
    el.hidden = true;
    el.innerHTML = '';
    return;
  }

  el.hidden = false;
  el.style.cssText =
    'margin: 0 0 8px; padding: 10px 12px; border: 1px solid #b58900; ' +
    'border-radius: 4px; background: #2a2410; color: #e8dcb0; font-size: 12px;';
  el.innerHTML = sectionsHtml;
}

function render() {
  const unseen = report?.unseen ?? [];
  const overrides = report?.overrides ?? [];
  const shadowed = report?.shadowed ?? [];
  const custom = report?.custom ?? [];
  const launchConfigMissing = report?.launchConfigMissing ?? null;
  const hazards = custom.filter((c) => c.kind === 'hazard');
  const customOverrides = custom.filter((c) => c.kind !== 'hazard');

  // ── Launch Config ────────────────────────────────────────────────────────
  const launchParts = [];
  if (launchConfigMissing) {
    const rows = `<li><code>${STRINGS.CFG.launchConfigMissingRow(launchConfigMissing)}</code></li>`;
    launchParts.push(section(STRINGS.CFG.LAUNCH_CONFIG_MISSING_TITLE, STRINGS.CFG.LAUNCH_CONFIG_MISSING_ADVICE, rows));
  }
  renderInto('#launch-config-warning-banner', joinSections(launchParts));

  // ── Initial Commands ─────────────────────────────────────────────────────
  // Game Config (unseen) belongs here, not its own block: the fix it advises
  // is "state this in Initial Commands", so that is where seeing it is useful.
  const initParts = [];
  // First, because something the user typed is being thrown away rather than
  // winning.
  if (shadowed.length > 0) {
    const rows = shadowed
      .map((s) => {
        const text = s.winnerFromApp
          ? STRINGS.CFG.shadowedByApp(
              s.cvar,
              s.shadowedValue,
              s.winnerValue,
              STRINGS.CFG.SETTING_FOR_CVAR[s.cvar.toLowerCase()] || STRINGS.CFG.UNKNOWN_SETTING
            )
          : STRINGS.CFG.shadowedByYou(s.cvar, s.shadowedValue, s.winnerValue);
        return `<li><code>${text}</code></li>`;
      })
      .join('');
    initParts.push(section(STRINGS.CFG.SHADOWED_TITLE, STRINGS.CFG.SHADOWED_ADVICE, rows, '#ff8a5c'));
  }
  if (unseen.length > 0) {
    const rows = unseen
      .map(
        (f) =>
          `<li><code>${f.cvar} ${f.value}</code> — ${STRINGS.CFG.location(f.file, f.line)}</li>`
      )
      .join('');
    initParts.push(section(STRINGS.CFG.BANNER_TITLE, STRINGS.CFG.ADVICE, rows));
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
    initParts.push(section(STRINGS.CFG.OVERRIDE_TITLE, STRINGS.CFG.OVERRIDE_ADVICE, rows));
  }
  renderInto('#init-commands-warning-banner', joinSections(initParts));

  // ── Scheduled Commands ───────────────────────────────────────────────────
  const schedParts = [];
  // First of all, because this one does not merely surprise: it breaks the
  // flush and leaves a capture that completes and looks plausible.
  if (hazards.length > 0) {
    const rows = hazards
      .map((h) => `<li><code>${STRINGS.CFG.hazardRow(h.command)}</code></li>`)
      .join('');
    schedParts.push(section(STRINGS.CFG.HAZARD_TITLE, STRINGS.CFG.HAZARD_ADVICE, rows, '#ff6b6b'));
  }
  if (customOverrides.length > 0) {
    const rows = customOverrides
      .map((c) => {
        const text =
          c.kind === 'overridesInit'
            ? STRINGS.CFG.customOverridesInit(c.cvar, valueOf(c.command), c.replacedValue)
            : STRINGS.CFG.customOverridesConfig(
                c.cvar,
                valueOf(c.command),
                c.replacedValue,
                c.source
              );
        return `<li><code>${text}</code></li>`;
      })
      .join('');
    schedParts.push(section(STRINGS.CFG.CUSTOM_TITLE, STRINGS.CFG.CUSTOM_ADVICE, rows, '#ff8a5c'));
  }
  renderInto('#scheduled-commands-warning-banner', joinSections(schedParts));
}
