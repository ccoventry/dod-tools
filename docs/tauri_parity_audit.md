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
- [ ] **DRIFT** — "Batch Capture Config" as its own top-level tab
  (`data-nav="export-config"`, `desktop-studio/index.html:22`). Dev never had
  this as a separate destination — export/timing/drive/custom-command config
  was a phase *inside* the single continuous Capture Studio workflow, not a
  place you navigate away to.
- [ ] **GAP** — Settings tab has no equivalent anywhere in Tauri. Dev's
  Settings view (`git show 80feaaf:native/src/bin/gui/views/settings.rs`) had
  a language selector, a "scan folders for demos" toggle, and a pinned-folder
  bookmark list (add/remove) with an explicit draft → Save/Revert pattern.
  In Tauri: `main.js:64-65` hardcodes `language: "en"`, and `pinned_folders`
  (`main.js:163-164`) auto-populates from whatever's been scanned instead of
  being a user-managed list. Settings now auto-save per field with no revert.

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

- [x] **GAP — fixed (2026-08-16).** The demo browser lost search, filters,
  sort, and grouping entirely — rebuilt as a flat, filterable/sortable list
  across every recursively-watched folder (search name/map/path, Type/Map/
  Date filters + Reset, sortable Name/Type/Map/Date columns with ▲/▼
  indicators, arrow-key navigation + Enter to open), replacing the
  single-directory drill-down.
  - **Correction from the original finding**: dev's "Group by Match" and
    "Group by Player-Recorder" view modes were *not* ported — traced their
    grouping keys (`server_ip`, `player_roster_hash`, `recorder_id`) and
    confirmed via `grep` that dev's `main.rs` only ever assigns them `None`,
    at the one call site that constructs them, with no other assignment
    anywhere in the file. Every demo would land in the same empty group in
    dev too — same category of dead shell as the POV tab / Auditor's delete
    button / CSV export button already excluded elsewhere in this audit.
    Only "Flat List" (the one real, working view) was restored.
  - **New backend**: `list_demos_recursive` (`dir_browser.rs`) walks watched
    folders for `.dem` files — filesystem-only, no parsing, near-instant
    regardless of library size — and opportunistically fills `map_name`/
    `demo_type` from a new cache-only-read helper, `native::peek_analyzer_cache`
    (never falls back to parsing, unlike `run_analyzer_cached`). Cache misses
    ship as `null` and get lazily resolved one at a time in the background via
    a new `resolve_demo_summary` command (does the real, possibly-~1.3s parse,
    through the existing on-disk cache, so the cost is paid once per demo
    rather than once per browse) — table re-renders incrementally as results
    land instead of blocking on the whole library up front.
  - **Watched folders**: reuses the app's existing shared `pinned_folders`/
    `scanPaths` state (same list Capture Studio scans) rather than a
    separate Analyzer-only list — this is the faithful restoration of dev's
    actual design (its `desktop_files` list was app-wide too), unlike the
    Auditor fix earlier in this doc, which deliberately moved *away* from
    shared state because dev's Auditor really was scoped to one ad-hoc
    folder. Added explicit add/remove UI so which folders are being searched
    is no longer silent (the problem that made sharing state wrong for the
    Auditor) even though the state itself is intentionally shared here.
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
- [ ] **DRIFT** — Fast-Forward Speed: dev rendered this slider explicitly
  disabled (`add_enabled_ui(false, ...)`) — present but intentionally
  non-editable. Tauri's version is fully live. Distinct from the
  already-tracked default-value mismatch (10.0 vs 0.05, see
  `engineering_backlog.md` Low Priority) — this is about the control being
  locked vs. unlocked.
- [x] ~~Timing fields split across two dev tabs, merged into one in Tauri~~ —
  low-severity DRIFT, functionally harmless, not worth tracking further.
- [ ] **GAP** — "Auto-Quit Game on Completion" checkbox
  (`PatcherConfig.exit_on_finish`) is gone — no control, no payload field, no
  read anywhere in Tauri. Caveat: this field looks like it was *already*
  dead-wired in dev's real (non-CLI) capture path too — only a separate,
  CLI-only code path (`native/src/bin/cli.rs`) ever honored it — so practical
  impact may be low. Still a real, untracked UI-parity fact.

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
