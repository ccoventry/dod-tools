# Engineering Backlog

## 📋 Immediate Tasks & Critical Bugs (Current Sprint)

### Pipeline Blockers
- [ ] **Bug Fix: `capture_directories` Drive Junction Abort:** `capture_pane.js` passes `scanPaths` (demo file paths) as target output drives instead of drive directories. Fix `buildCapturePayload()` to bind strictly to configured output drive pools (`primary_media_dir`/`backup_media_dir`), preventing `mklink /J` junction errors.
- [ ] **Bug Fix: Un-stamped `session_id` Timestamp:** `capture_pane.js` and `config_from_payload()` leave `session_id` as `""`. Implement ISO timestamp generation (`session_YYYYMMDD_HHMMSS`) in payload construction so batch outputs land in dedicated session subfolders.

### Capture Studio Feature Parity & UI Restorations
- [ ] **Feature: Global & Per-Demo Bookmark Previews (`viewdemo`):** Revert per-streak preview ▶ buttons. Implement single-demo "Preview Demo" and global "Preview All" actions that patch `svc_director` `BOOKMARK` events for highlights and launch HLAE using `+viewdemo <stem>_preview` to populate the GoldSrc VCR event list.
- [ ] **Bug Fix: Detail Pane String Formatting:** Fix `detail_pane.js` rendering logic where weapon strings are omitted and streak detail lists display a leading comma on the first event.
- [ ] **UI Polish: Selection Checkbox Restoration:** Revert the highlight table "Sel" column back to an explicit selection checkbox.
- [ ] **UI Polish: Tab Branding & Styling:** Rename "Workspace & Master Queue" back to "Capture Studio". Add explicit CSS dropdown chevrons in `styles.css` so select controls do not look like plain text fields.
- [ ] **UI Cleanup: Dead POV Input Field:** Remove or properly wire the text field `#config-hide-non-pov` to a clean boolean toggle.
- [ ] **Task: Verify Branch Merge Readiness:** Verify `feature/tauri-migration` functionality against `dev` for clean merge.

---

## 🔍 Capture Studio Parity Backlog (Audited Gaps)

### High Priority
- [ ] **Custom Engine Commands Integration:** Add UI inputs and payload wiring for `init_commands` and `custom_commands` (Before/After execution with time offsets) to reach `builder.rs`.
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