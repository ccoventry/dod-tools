# Engineering Backlog

## 📋 Immediate Tasks & Critical Bugs (Current Sprint)

### Capture Studio Feature Parity & UI Restorations
- [ ] **Task: Verify Branch Merge Readiness:** Verify `feature/tauri-migration` functionality against `dev` for clean merge.

---

## 🔍 Capture Studio Parity Backlog (Audited Gaps)

### High Priority
- [ ] **`PatcherConfig` Full Persistence:** Expand Tauri `AppSettings` (`settings_manager.rs`) to persist resolution, `separate_hud`, `condebug`, auto-clear flags, lead/trail times, initial delay, fast-forward speed, allocation strategy, and drive pools across application restarts.
- [ ] **Pre-Flight Disk Allocation & Pre-Scan Estimator:** Restore overlap-merged duration math (`calculate_merged_duration`), `separate_hud` 3x multiplier, summed drive pool checks, per-drive breakdown UI, and hard Launch-button lock when total available space is insufficient.
- [ ] **Running Process Guard & Detector Modal:** Implement `sysinfo` process detection prior to preview launch to prompt the user with Force Relaunch or Copy View Command when `hl.exe` or `hlae.exe` is already running.

### Medium Priority
- [ ] **"Clear Previews" Audit Modal:** Add scanner and IPC command to sweep `<hl>/dod` for orphaned `*_preview.dem` files with valid `.dodtools_preview` sidecars and offer one-click batch deletion.
- [ ] **Standalone Game Launch:** Restore "Launch Game (HLAE)" button to boot the game environment without loading a demo stream.
- [ ] **Drive Pool Interactive Management:** Add ⬆/⬇ reorder and 🗑 delete actions with live persistence for `#target-drive-list` and `#render-folder-list`.
- [ ] **`movie_config` Input & Payload Binding:** Expose sanitised text field in Export Config to populate `+exec <name>.cfg` in HLAE launch args.
- [ ] **Session Bookkeeping & Manifest Logging:** Restore writing `Capture_Sessions/<session_id>/manifest.txt`, routing chained demo copies, and cleaning up empty session directories on cancel/completion.
- [ ] **Autosave Lockfile Recovery:** Re-enable writing `.autosave.json` immediately before batch dispatch to allow state restoration after unclean exits or crashes.
- [ ] **Demo Path Resolution & Missing Marker:** Implement fallback path resolution (`resolve_demo_path`) against project root and highlight missing files with red UI indicators in the master list.
- [ ] **Session Schema & Hash Key Verification:** Update project import/export to include FNV demo hashes, re-scanning un-indexed files on load, and restoring default `Documents/dod-tools/projects` path routing.
- [ ] **Ingestion Failure Diagnostics:** Surface HLTV proxy detection and corrupted demo skip events via toasts and log lines instead of silent drops.
- [ ] **Queue Bulk Action ("Clear Discovered"):** Add a single-click button to clear all loaded demos from the Master List.
- [ ] **Global Select/Deselect Scope:** Ensure global "Select All" / "Deselect All" operates across every queued demo in the master queue while respecting local-player POV filters.

### Low Priority
- [ ] **Execution Timeline (Mock Table):** Restore visual time-offset preview table showing relative sequence marks for Record Start/Stop, pre/post-roll, and custom commands.
- [ ] **Keyboard Shortcuts & Navigation Polish:** Restore Escape key handler to clear demo selection ring.
- [ ] **Default Fast-Forward Speed Parity:** Synchronize `default_fast_forward_speed` serde default (10.0 vs 0.05) across Rust backend and frontend inputs.

---

## 🛠 General Backlog & Future Upgrades

### R&D & Architectural Enhancements
- **External Demo Playback:** Investigate if DoD `.dem` files can be parsed and rendered outside the game engine (e.g., web browser / lightweight desktop app) to preview killstreaks quickly.
- **Mode Toggle:** Add functionality to Capture Studio to switch between "Timing Mode" and "Capture Mode".
- **Session Compression & Modular Export:** Add options for Gzip/Zstd compression for JSON and "Selective Export" feature flags (e.g., export demos only vs. export full project metadata).
- **Project Classification Types:** Add project categorization options to Capture Studio (e.g., Solo Movie, Frags of the Week, Team Movie).
- **Advanced Queue Filters:** Add quick-filter toggles to Master List (e.g., hide demos with 0 kills, hide demos with no streaks, hide non-POV demos).

---

## ✅ Completed Tasks

- [x] **Feature: Tauri & Vite Integration:** Migrated frontend stack from native `egui` to Tauri + Vite, establishing async IPC boundaries for demo ingestion, settings persistence, demo auditor, and AOT simulation.
- [x] **Headless Preview CLI Generator:** Built secondary `preview_cli` binary target ([main.rs](file:///c:/Users/Chris%20Coventry/Repos/dod-tools/native/src/bin/cli/main.rs)). Supports drag-and-drop folder/file execution, localized terminal output, and interactive/headless modes.
- [x] **Top Navigation Bar Migration & Non-Destructive Scan Cancellation:** Extracted Export Manager view, migrated sidebar to Top Navigation Bar, implemented non-destructive scan cancellation (`INGESTION_CANCEL`), and scaffolded localized footers across views.
- [x] **Localization System & Complete String Audit:** Localized all GUI, scanner weapon names, and CLI string literals. Updated `analysis::localization` to resolve both hashed (`#key`) and un-hashed (`key`) localization entries seamlessly.
- [x] Task: Magic Number Audit — Scanned codebase for hardcoded magic numbers and extracted shared constants.
- [x] Refactor: Isolate native engine stream slicing mechanisms behind explicit `target_arch` macro controls.
- [x] **HLTV Active Frame Injection:** Implemented and verified active standalone `DRC_CMD_INEYE` frame injection in `native/src/patch/engine.rs` with dynamic `target_player_id` extraction.
- [x] **Dynamic Drive Failover:** Implemented AOT capture routing, duration math parity, JIT render routing, and UI/UX export pool indicators with dynamic vector list reordering.
- [x] **Long Demo Validation & Packet Audit:** Verified parsing and slicing stability on extended demo files (>30 mins).
- [x] **Global Deprecation & Cruft Purge:** Deleted legacy fallback paths, unused variables, and historical code workarounds across UI and patcher crates.
- [x] **Bug Fix: `capture_directories` Drive Junction Abort:** `capture_pane.js` now builds `capture_directories` from the Target Output Drives pool (`state.targetDrives`), falling back to Primary/Backup Media Dir when that pool is empty, and toasts an error instead of dispatching when no output directory is configured at all — no longer sourced from `scanPaths`.
- [x] **Bug Fix: Un-stamped `session_id` Timestamp:** `generateSessionId()` in `capture_pane.js` stamps `session_YYYYMMDD_HHMMSS` into every outbound payload; `config_from_payload()` in `capture_manager.rs` extracts `payload.session_id` into `PatcherConfig.session_id`.
- [x] **Feature: Global & Per-Demo Bookmark Previews (`viewdemo`):** Reverted the per-streak preview ▶ buttons. Added "Launch Preview" (per-demo) and "Generate All Previews" (global) to `detail_pane.js`/`index.html`. Backend commands `launch_demo_preview`/`generate_all_previews` (replacing `launch_live_preview`) patch bookmarked `<stem>_preview.dem` files via `build_preview_patch_jobs`' existing per-highlight `svc_director` STUFFTEXT event injection, and the single-demo path launches HLAE directly with `+viewdemo <stem>_preview` (the old `primer_preview.dem` relay is gone).
- [x] **Bug Fix / UI Polish: Detail Pane Selection Checkbox & String Formatting (verified 2026-08-13, no code change needed):** Audited `detail_pane.js` against a request to restore the "Sel" checkbox and fix a leading-comma/missing-weapon formatting bug — both were already fixed in `82bf4a0` ("restore backend-supported highlight-table controls lost in migration"). The "Sel" column already renders `<input type="checkbox" class="streak-select-cb">` bound to `streak.selected`; `updateStreakVisuals()`'s `.map().join(', ')` never prepends a leading comma and always includes the weapon name, matching `CaptureStreak::update_visuals` in `native/src/patch/types.rs`.
- [x] **Bug Fix: Detail Pane — 3rd re-verification, defensive hardening added (2026-08-13):** Same claim re-filed with a specific repro string (`". (+0:29) , (+0:06)"`). Traced every candidate source (join loop, Rust mirror, `analyzer_pane.js`'s unrelated weapon grouping, the full `weapon.*` localization table) and found no path that produces it. Added `data-index="${streakIdx}"` to the checkbox per spec, and hardened `weaponClean` in `updateStreakVisuals()` with `.trim() || 'Unknown'` so an unresolved/empty weapon name can never leave a blank `Array.join()` element — the one theoretical (but unreproduced) root cause for an orphaned separator.
- [x] **Bug Fix: Detail Pane — actual root cause found from screenshot, two real bugs fixed (2026-08-13):** The prior two passes checked JS join logic and localization *data* (both correct) but not whether the app could *find* the localization file at runtime. (1) `styles.css`'s global `appearance: none` reset stripped native checkbox/radio rendering app-wide with no replacement — every checkbox including "Sel" was invisible but still clickable; exempted `input[type="checkbox"], input[type="radio"]` from the reset with themed `accent-color`. (2) `analysis::localization::load_pass()` (`analysis/src/localization.rs`) only checked CWD-relative paths and one `.parent()` hop from the exe, which never resolves to the workspace-root `localizations/` folder from the Tauri app's actual runtime working directory (`desktop-studio/src-tauri/target/...`) — every `translate_key("weapon.*")` call silently returned `None` and cached that failure process-wide, so every weapon name rendered as `""`, producing "N kills" fallbacks for single-kill streaks and orphaned `(+m:ss)` fragments (no weapon name) for multi-kill streaks. Fixed by walking up to 6 parent directories from the exe path looking for `localizations/` instead of assuming a fixed depth.
- [x] **UI Polish: Tab Branding & Styling (2026-08-13):** Renamed the nav tab "Workspace & Master Queue" → "Capture Studio" in `index.html`. Added a themed SVG chevron background to the global `select {}` rule in `styles.css` (the pre-existing `appearance: none` reset had removed the native arrow with no replacement, so every dropdown looked like a plain text field); stripped the two inline `style="..."` selects (`#render-codec-select` in `index.html`, `.streak-status-select` in `detail_pane.js`) that would otherwise override the chevron via inline-style specificity, preserving `.streak-status-select`'s dynamic per-status `color`.
- [x] **UI Cleanup: Dead POV Input Field (2026-08-13):** Deleted `#config-hide-non-pov`, its `<label>`, and the wrapper `.control-group` from `index.html`; deleted the corresponding unused `hideNonPovCheckbox` query from `detail_pane.js`. The POV filter (`isHLTV`/`recPlayer` check in `renderDetailView`) never read this checkbox — it was already unconditional — so no downstream logic changed.
- [x] **Custom Engine Commands Integration (2026-08-13):** Added a 4th "Custom Commands" tab to the Export Configuration panel (`index.html`) with an Init Commands list (text input + Remove) and a Custom Commands list (command text, Before/After `<select>`, offset-seconds number input, Remove) per row. `capture_pane.js` holds `initCommands`/`customCommands` as local module state (nothing else in the app needs them), with render functions wiring Add/Remove/edit handlers; extracted the existing inline payload-assembly code into a named `buildCapturePayload(state)` function (as referenced by this and the capture-directories task) and appended `init_commands`/`custom_commands` to its output, dropping blank rows. Backend: added `CustomCommandPayload { command, relation: String, offset_seconds }` DTO to `capture_manager.rs` and two new `#[serde(default)]` `CapturePayload` fields; `config_from_payload()` maps `init_commands` straight through and converts each `CustomCommandPayload` into `native::patch::CustomCommand`, parsing `relation` with a safe `_ => Before` fallback for any unrecognised value (mirrors the existing `allocation_strategy` string-match pattern) rather than rejecting the batch. Both fields were already fully consumed downstream by `build_batch_queue`/`builder.rs` (`config.custom_commands`/`config.init_commands`) — this closes the last gap between that existing engine support and the UI.