# Tauri Parity Audit — `feature/tauri-migration` vs. `dev` egui source

Status (2026-08-16): **No merge blocker.** `cargo build --workspace` and
`cargo test --workspace --no-fail-fast` are clean except 4 pre-existing
`analysis`-crate failures, confirmed byte-identical at the `dev` merge-base
(`80feaaf`) — not caused by this branch (see `active_sprint_state.md`).
`dev`'s tip *is* the merge-base, so there is no divergence to reconcile
before merging.

This document is the result of a systematic field-by-field comparison of the
Tauri/Vite rebuild (`desktop-studio/`) against the actual dev-branch `egui`
source as it existed at the merge-base commit (`git show 80feaaf:native/src/bin/gui/...`
— that directory is deleted at HEAD, use `git show` to read it). Purpose:
separate real hallucinations (AI-invented fields/tabs with no backing logic)
from honest UI reshaping (structural drift) and simple feature loss that
never got tracked (parity gap).

Classification used throughout:
- **INVENTED** — exists in Tauri, no dev equivalent, traced to be functionally
  dead (not backed by real logic downstream).
- **DRIFT** — exists in materially different shape than dev, but is real,
  working functionality. Judgment call, not a bug — unless flagged
  **OUTPUT BUG**, meaning it silently changes what actually gets produced
  (rendered video, etc.), not just how it looks/feels.
- **GAP** — real, functional dev feature, simply missing from Tauri today,
  and not already tracked in `docs/engineering_backlog.md`.
- Confirmed matches are noted as a rollup per area, not itemized — the point
  of this doc is the problems, not re-proving what already works.

Suggested order of attack: fix the 2 INVENTED items first (cheap, no
downside) — **done, 2026-08-16**. Then decide on the 2 Render Studio OUTPUT
BUG items (silent correctness issues, not cosmetic) — **done, 2026-08-16**.
Then triage GAP items into `engineering_backlog.md` at whatever priority you
want. DRIFT items without an OUTPUT BUG flag are reshaping, not breakage —
decide case by case whether they're worth reverting.

---

## 1. Top-level navigation

Dev's `SidebarTab` enum (`native/src/bin/gui/types.rs` @ `80feaaf`) had exactly
5 destinations: `Analyzer, CaptureStudio, Auditor, ExportManager, Settings`.

- [x] ~~"Render Studio" tab~~ — **not an issue.** Faithful rename: dev's
  `SidebarTab::ExportManager` literally rendered the HLCR render UI
  (`native/src/hlcr/ui.rs`). Corrected from an earlier assumption made
  mid-audit.
- [x] **DRIFT — fixed (2026-08-16).** "Batch Capture Config" as its own
  top-level tab. Verified dev's real structure (`views/capture/workspace.rs`):
  a `SelectTab` enum (`Highlights` / `Configuration` / `Advanced`) persisted
  in `egui`'s temp memory, switched via in-page buttons *inside* the single
  `SidebarTab::CaptureStudio` destination — the Master List panel renders
  unconditionally above the tab switch, only the detail area below it changes
  content. Dev's `Configuration` (engine paths, drive failover, export/codec
  config) + `Advanced` (highlight buffers, custom commands, debug settings)
  map onto Tauri's existing 4-tab Path Routing/Timing/Drive Overrides/Custom
  Commands split with no fields lost, so rebuilt as 2 in-page sub-tabs
  (Highlights/Configuration) rather than 3 — a non-lossy consolidation, same
  precedent as the already-accepted "timing fields merged into one tab" DRIFT
  below. Removed the `data-nav="export-config"` top-level button; added a
  `#capture-detail-subtabs` bar as the first child of `#pane-details-config`
  (`setCaptureDetailSubtab`/`getCaptureDetailSubtab` in `nav.js`) toggling
  `#detail-pane`+`#advanced-diagnostics-details` vs `#export-config-panel`.
  "Proceed to Capture" now switches to the Configuration sub-tab instead of
  navigating to a separate tab. Also fixed a dead branch this exposed: the
  Ctrl+O shortcut dispatcher (`main.js`) checked for the now-removed
  `'export-config'` top-level tab; repointed it at the new sub-tab state so
  Ctrl+O still maps to Load Project vs Add Demo Files correctly.
- [x] **GAP — fixed (2026-08-16), scope narrowed with user.** Dev's
  Settings view (`git show 80feaaf:native/src/bin/gui/views/settings.rs`) had
  a language selector, a "scan folders for demos" toggle, and a pinned-folder
  bookmark list (add/remove) with an explicit draft → Save/Revert pattern,
  as its own top-level `SidebarTab::Settings` destination (confirmed —
  `hlae_path`/`hl_path`/capture config are NOT in dev's Settings tab, they
  stay inline in Capture Studio, exactly where Tauri already has them; the
  split itself was already correct, only the Settings tab was missing).
  Rebuilt with two scope cuts, both decided with the user this session:
  (1) **no separate Settings page** — the remaining two settings are
  Analyzer-specific, so they now live in a collapsible "⚙ Explorer Settings"
  section inside the Analyzer's Explorer sidebar instead of a 6th nav tab;
  (2) **language selector dropped entirely** — dev's 8-language dropdown
  retranslates real UI text via a `t()` lookup table that has no Tauri
  equivalent (every string is hardcoded English), so shipping the dropdown
  would itself be a new INVENTED field. `scan_folders_for_demos` was added to
  `AppSettings` (`#[serde(default)]` → `false`, matching dev's own default —
  this is a real behavior change from Tauri's prior always-on tree demo
  counts) and gates only the Explorer tree's "(N)" badge, auto-saving
  immediately on toggle rather than dev's draft/Save/Revert (simplification:
  one inline checkbox doesn't warrant a bespoke commit pattern when every
  other Tauri setting already auto-saves). The pinned-folder bookmark list
  itself was **not** duplicated — the Analyzer's Quick Links "Pinned" tier
  already lists every pinned folder with its own pin/unpin control, so a
  second add/remove list in the same pane would be redundant; only dev's
  "add a pin without navigating to it first" convenience was kept, as an
  "➕ Add Pin…" button next to the toggle.

---

## 2. Demo Auditor

Rollup: core duplicate-detection algorithm is a faithful port — both sides
call the real `hl_demo_auditor::scan_dir`/`find_duplicates`. No invented
logic anywhere in this pane.

- [x] **DRIFT — fixed (2026-08-16).** Folder targeting: dev's dedicated
  per-audit "Target Folder" field + native picker was gone, replaced by
  silently auditing the app-wide pinned-folders list. Restored a dedicated
  `#audit-target-folder-input` + Browse button in `auditor_pane.js`; "Start
  Audit" is disabled until a folder is chosen, matching dev's
  `!target_folder.is_empty()` gate.
- [x] **DRIFT — fixed (2026-08-16).** Result table: rebuilt as a collapsible
  group tree (▶/▼ toggle per group, hash shown once per group header) instead
  of a flat list with the hash repeated on every row. Kept Tauri's own
  checkbox/delete mechanism (see below) layered on top rather than reverting
  to dev's checkbox-less table.
- [x] **DRIFT — fixed (2026-08-16).** Scan progress: status area now shows a
  CSS spinner + bold "Found N demo file(s) so far…" + the raw progress detail
  underneath, instead of one flat combined line. New `.spinner` class in
  `styles.css`.
- [x] ~~Delete mechanism~~ — **not a regression.** Dev's "Delete Selected
  Demos" was a literal `// TODO: Wire deletion dispatch` stub (pane subtitled
  "Read-Only tool"). Tauri's version is real, working deletion via per-row
  checkboxes → `delete_audit_files`. Improvement, not drift to worry about —
  kept and layered under the restored group-tree view above.
- [x] **GAP — fixed (2026-08-16).** Cancel Scan is now wired: added a Cancel
  button in `index.html`, a `cancelAudit()` wrapper in `ipc_bridge.js`, and a
  click handler in `auditor_pane.js` that calls the pre-existing (but
  previously unused) `cancel_audit` command. `hl_demo_auditor::scan_dir`/
  `find_duplicates` already checked the cancel token internally and return
  partial results early, so no backend change was needed — only the missing
  frontend wiring.
- [x] **GAP — fixed (2026-08-16).** Per-file "Copy Path" restored via
  `navigator.clipboard.writeText`.
- [x] **GAP — fixed (2026-08-16).** Per-file "Open Folder" restored. Added a
  new `reveal_in_explorer` Tauri command (`audit_manager.rs`, registered in
  `lib.rs`) — `explorer /select,` on Windows, `open -R` on macOS, `xdg-open`
  on the parent directory as the Linux fallback (dev used the `open` crate
  there; used `xdg-open` directly instead to avoid an unverifiable new
  dependency on a platform this pass can't build/test).
- [x] **GAP — fixed (2026-08-16).** Groups now default to expanded (all
  files visible, matching Tauri's prior always-visible behavior) with a
  per-group collapse toggle for decluttering large result sets — see the
  result-table fix above.

---

## 3. Demo Analyzer

Rollup: **zero invented fields.** Summary (~17 fields), Scoreboard,
Player Details, Team Details, Rounds, Timeline, and Chat Log all trace
field-for-field to real `analysis::AnalyzerState` data on both sides. The
missing POV tab is *correct*, not a gap — dev's `pov.rs` was already
`#[allow(dead_code)]` and never called before the migration.

- [x] **GAP — fixed (2026-08-16), rebuilt to the corrected design.** The
  wrong-shape recursive multi-folder aggregate (`list_demos_recursive`,
  `resolve_demo_summary`, `native::peek_analyzer_cache`, the "Watched
  Folders" flat-list sidebar from `75da64f`) was deleted and replaced with
  dev's real architecture: a native Explorer Tree (drives → subfolders,
  lazily loaded per node, genuine expand/collapse, auto-expands down to the
  current selection, collapsing an ancestor jumps you up to it) plus a
  3-tier Quick Links box (📌 Pinned / 🕒 Recent / 📂 Local, each hidden when
  empty) — both driving one shared selected folder. The Demos filter/sort
  table (already correct from the first attempt) is now scoped to only that
  single folder's contents, non-recursive, with map name read straight from
  each file's 276-byte `HLDEMO` header instead of the analyzer cache.
  `AppSettings` gained `demo_folder_history` (Recent, capped at 10,
  `#[serde(default)]`); `pinned_folders` (Pinned) was already there. Long
  Quick Links paths are truncated to `<drive>\...\<final folder>` with the
  full path on hover, per the user's explicit spec. One deliberate
  simplification vs. dev: no `scan_folders_for_demos` settings toggle exists
  yet (that's Area 1's Settings-tab GAP, still open), so per-folder demo
  counts are always computed rather than gated behind it — harmless, just
  means the toggle has nothing to disable yet. Small follow-up fix same
  session: Workspace's "View Match Telemetry" button now points the sidebar
  at the demo's own folder before loading it (`openAnalyzerDemo` in
  `analyzer_pane.js`), so the Demos table reliably highlights the jumped-to
  demo instead of only coincidentally matching whatever folder the sidebar
  already happened to be on.
- [x] **GAP — fixed (2026-08-16).** Kill-streak weapon-category filters
  restored (Grenades/Melee/Allied/Axis/Other groups, per-category
  toggle-all + per-weapon checkboxes) in `analyzer_pane.js`, filtering which
  kills appear in the streak table exactly like dev's
  `render_streak_weapon_filters`/`rebuild_filtered_streaks`. Filter state
  resets whenever the effective selected player changes (dropdown, scoreboard
  click, or a kill-streak victim jump).
- [x] **GAP — fixed (2026-08-16).** "Legit-Proof" profile search link
  restored next to the Steam link on the Player Details hero card, built
  from the same `STEAM_0:X:YYYY`-formatted ID already computed for display.
- [x] **DRIFT — fixed (2026-08-16).** Team Details now aggregates weapon
  breakdowns into up to 3 buckets keyed off each player's raw `team` value
  (Allies / British / Axis, not merged), matching dev exactly — including
  its exact show/hide condition for the Allies section. The Overview grid's
  Allies+British-merged stats were correct already and untouched.
- [x] **GAP — fixed (2026-08-16).** Timeline chart hover tooltips restored.
  `drawTimelineChart` now returns per-point pixel coordinates + team/score,
  and a new `initTimelineTooltip` does a nearest-point hit test on
  `mousemove` to show a floating tooltip — canvas has no native
  `label_formatter` like `egui_plot`, so this reimplements the same
  end-user behavior rather than porting an API that doesn't exist here.
- [x] **GAP — fixed (2026-08-16).** Chat: Alive/Dead sender-status filter
  restored as a 3-way radio (All/Alive/Dead) using the already-real
  `sender_dead` field.
- [x] **DRIFT — fixed (2026-08-16).** Chat: system messages now filter
  through the same 4 independently-toggleable categories dev used
  (Joins/Leaves, Team Changes, Gameplay, Other System), categorized from
  `m.system_token` with dev's exact keyword rules
  (`systemMessageCategory` in `analyzer_pane.js`).
- [x] **GAP — fixed (2026-08-16).** Chat: Select-All/Clear-All buttons
  restored, resetting all chat filters and re-rendering the toolbar.
- [x] **GAP — fixed (2026-08-16).** Chat: team-name keyword coloring inside
  system message text restored (`colorSystemMessage`) — colors
  "allies"/"axis"/"spectators" substrings within the already-translated
  message text using dev's earliest-match scan, applied over `m.text` (which
  arrives from the backend already fully translated — confirmed
  `ChatMessage.text` is set from `translate_system_message` server-side in
  `analysis/src/chat.rs`, so no client-side translation call was needed).

---

## 4. Capture Studio workflow

Rollup: Master Queue, Highlights table, Path Routing, Timing math, Drive
Overrides, Custom Commands, launch buttons, and the Preview-Detector modal
all trace to real backend reads — most of the drift here is panels being
reshuffled, not logic being faked.

> Framing note: dev's 6-state `CapturePhase` enum (`ReviewQueue → Patching →
> HlaeCapture → HlcrRendering → Complete/Failed`) was already vestigial by
> the merge-base — the real live state was a per-streak `HighlightStatus`
> plus one global "is patching" gate. Doesn't change any finding below, just
> corrects the mental model of what "the phases" actually were.

- [x] **INVENTED — removed (2026-08-16).** "Expected FPS" field
  (`#config-expected-fps`) fed `PatcherConfig.tickrate`/`pre_roll_ticks`/
  `post_roll_ticks`, none of which anything in `native/src/patch/builder.rs`
  or `engine.rs` read (confirmed: `grep -rn "\.tickrate\b" native/` hit only
  struct defaults and test mocks; `builder.rs:144` comment: `// tickrate is
  extracted dynamically from streaks per-demo`). The real per-demo tickrate
  comes from the scanner, not this field. Dev's egui UI never exposed a
  control for `tickrate` at all. Deleted the input from `index.html`, the
  `expectedFpsVal`/`expected_fps` plumbing from `capture_pane.js`, and the
  `expected_fps` field + its three dead `cfg.*` assignments from
  `capture_manager.rs`.
- [x] **INVENTED (partial) — removed entirely (2026-08-16).** "Backup Media
  Dir" input partially resurrected a field dev had already killed off before
  the merge-base. Decided to remove the whole feature rather than keep it
  under a repurposed name — deleted the input row from `index.html`; its
  read/payload/fallback wiring from `capture_pane.js` (both
  `refreshLaunchGuard` and `buildCapturePayload`); its persistence, hydration,
  and browse-dialog handler from `main.js`; the `backup_media_dir` field from
  `CapturePayload` and its `cfg.backup_media_dir` assignment in
  `capture_manager.rs`; the field from `AppSettings` in
  `settings_manager.rs`; and the dead `PatcherConfig.backup_media_dir` field
  itself (declaration + `Default` entry) from `native/src/patch/types.rs`.
  Output drive routing now falls back to Primary Media Dir alone when no
  Target Drive is configured — users need at least a Primary Media Dir or a
  Target Drive to start a capture.
- [x] **DRIFT — fixed (2026-08-16).** Fast-Forward Speed: dev rendered this
  slider explicitly disabled (`add_enabled_ui(false, ...)`) — present but
  intentionally non-editable. Added `disabled` + an explanatory `title` to
  `#config-fast-forward-speed`; unaffected — disabled inputs still report
  `.value` to `persistAppSettings()`. Distinct from the already-tracked
  default-value mismatch (10.0 vs 0.05, see `engineering_backlog.md` Low
  Priority), which remains untouched — this fix was about locked vs. unlocked
  only.
- [x] ~~Timing fields split across two dev tabs, merged into one in Tauri~~ —
  low-severity DRIFT, functionally harmless, not worth tracking further.
- [x] **GAP — confirmed non-actionable (2026-08-16), no UI added.**
  "Auto-Quit Game on Completion" checkbox (`PatcherConfig.exit_on_finish`).
  Traced the real GUI/HLAE capture path this session: `capture_engine.rs:466`
  unconditionally `taskkill /F /IM hl.exe`s after every capture job
  regardless of any config, by design — a batch pipeline processing many
  demos sequentially must always tear down between jobs. `exit_on_finish` is
  only ever read by `highlevel.rs:184`'s `PatchOptions` (a *different*
  struct), only ever populated by the standalone CLI tool
  (`cli.rs:187: exit_on_finish: quit`) — confirmed genuinely dead in both
  dev's and Tauri's real (non-CLI) engine, not just "low practical impact."
  Building a checkbox for it would recreate exactly the kind of
  UI-with-no-backing-logic this audit exists to catch, so left unbuilt rather
  than adding a new INVENTED field for field-parity's own sake.

---

## 5. Render Studio

Rollup: no invented controls — every field maps to a real command. This area
had the audit's only two **output-affecting** regressions — both fixed
2026-08-16, see below.

> `hlcr/config.rs`, `renderer.rs`, and `autosave.rs` are byte-identical to
> dev and still compile — but they're orphaned. `render_manager.rs`
> reimplements the entire encode pipeline from scratch instead of calling
> them (`grep -rn "run_render_job|RenderSessionData|keepawake"
> desktop-studio/src-tauri/` → zero hits).

- [x] **OUTPUT BUG — fixed (2026-08-16)** — HUD-alpha clips silently lost
  transparency. Dev special-cased `clip_type == "hud_only"` to merge color +
  alpha BMP sequences via an `alphamerge` filter into transparent ProRes4444.
  The Tauri render loop never read `clip.clip_type` at all — it fed the
  color layer alone through whatever main codec was picked, producing an
  *opaque* video for a clip the scanner still correctly flags as HUD-only.
  Fixed in `render_manager.rs::execute_render_batch`: branches on
  `clip_type == "hud_only"`, feeds both the `hudcolor` folder and its
  sibling `hudalpha` folder (always present together per
  `native/src/hlcr/scanner.rs`) through the same `extractplanes`/
  `alphamerge` filter dev used, forced through ProRes4444
  (`alpha_codec_args_and_ext`) regardless of the user's codec selection.
- [x] **OUTPUT BUG — resolved (2026-08-16)** — "H.264" had silently become
  software-only encoding. Dev's H.264 option was `h264_nvenc` (GPU/NVENC,
  `-preset p6 -tune hq -cq 15`); Tauri's was `libx264` (CPU/software,
  `-preset fast -crf 16`) under the same menu label. Resolved by adding both
  as separate, explicit options rather than reverting to one: "H.264
  (Software, MP4)" (`libx264`, unchanged) and "H.264 (NVENC GPU, MP4)"
  (`h264_nvenc`, dev's original args) in `render-codec-select`. Decision
  context: the user's own later, independent rewrite of this tool (a
  separate Python/PySide6 project, `../HLCR`) also defaulted to software
  `libx264` — so keeping it wasn't the accidental-hallucination artifact it
  looked like, it matched the user's own considered choice. NVENC is
  generally faster and comparable-or-better quality than `libx264` at fast
  presets on an NVIDIA GPU (Turing/RTX 20-series+), but isn't universally
  available, hence offering both explicitly instead of picking one.
- [ ] **DRIFT** — concurrent render queue → strictly sequential. Dev's
  `max_concurrent_renders` (1-8) drove parallel jobs with per-job CPU thread
  scaling; Tauri processes one FFmpeg job at a time, no concurrency control
  exposed anywhere.
- [ ] **DRIFT** — JIT multi-drive export pool collapsed to one static path.
  Dev auto-selected the first export drive with 20 GB+ free from a
  configurable pool, with a live "Total Export Pool Free" readout. Tauri has
  a single text-field export directory — no picker, no free-space fallback,
  no readout in the Render Studio panel. (Tauri's capture-side drive pool is
  a separate thing and is never wired into rendering.)
- [ ] **DRIFT** — per-job queue table collapsed into one global status line +
  one global progress bar. Dev showed clip name/stream/frames/date/colored
  status/speed/per-row progress bar/per-row Cancel+Reset+View-Log actions.
  Tauri: no per-job rows, no Speed field, no per-job cancel/reset, errors are
  a transient toast only (nothing persisted).
- [ ] **GAP** — render-batch crash-recovery autosave (`.render_autosave.json`)
  is gone. Distinct from the capture-side autosave already tracked in
  `engineering_backlog.md` (dev kept these as two separate files with
  separate recovery prompts) — the render-side one is untracked.
- [ ] **GAP** — no wake-lock during a render batch. Dev held a
  `keepawake::KeepAwake` guard for the render's duration, released on
  cancel/completion, to stop the OS sleeping mid-batch. No trace of this
  anywhere in Tauri.
- [ ] **GAP** — auto-chaining a render after capture is gone. Dev
  automatically pointed the render source at the just-finished capture
  session and kicked off a scan/render once a capture batch completed — this
  was automatic behavior, not a manual toggle, so there's no checkbox to
  restore, the behavior itself just doesn't happen anymore.

---

## Methodology notes (for whoever picks this up next)

- Ground truth for "what dev actually did" is always `git show
  80feaaf:<path>` — that commit is the `dev`/`feature/tauri-migration`
  merge-base, and `native/src/bin/gui/` is fully deleted at HEAD, so working-tree
  reads of that directory will 404.
- The "field exists in UI but nothing downstream reads it" pattern (see
  INVENTED items above) is checked with `grep -rn "<field_name>"
  native/src/patch/` — a real field shows up read in `builder.rs`/`engine.rs`,
  not just declared in `types.rs` and defaulted.
- Not everything different from dev is a problem — dev shipped real dead
  code and non-functional stubs too (POV tab, Auditor's delete button,
  Analyzer's CSV export button, footer counters). Confirming a dev-side
  feature was itself already non-functional before calling its absence in
  Tauri a "gap" avoided several false positives in this pass.
