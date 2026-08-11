# Engineering Backlog

## 📋 General Backlog (Future Roadmap Items)

## Immediate Tasks (Next Session)
- [ ] Feature: Tauri & Vite Integration — Audit existing scaffolding in `desktop-studio/` and `src-tauri/` on branch `feature/tauri-migration`.

## Completed
- [x] **Headless Preview CLI Generator:** Built secondary `preview_cli` binary target ([main.rs](file:///c:/Users/Chris%20Coventry/Repos/dod-tools/native/src/bin/cli/main.rs)). Supports drag-and-drop folder/file execution, localized terminal output, and interactive/headless modes.
- [x] **Top Navigation Bar Migration & Non-Destructive Scan Cancellation:** Extracted Export Manager view, migrated sidebar to Top Navigation Bar, implemented non-destructive scan cancellation (`INGESTION_CANCEL`), and scaffolded localized footers across views.
- [x] **Localization System & Complete String Audit:** Localized all GUI, scanner weapon names, and CLI string literals. Updated `analysis::localization` to resolve both hashed (`#key`) and un-hashed (`key`) localization entries seamlessly.
- [x] Task: Magic Number Audit - Scan the codebase for hardcoded magic numbers (e.g., buffer sizes, static offsets, byte limits) and extract them into shared module-level constants.
- [x] Refactor: Isolate native engine stream slicing mechanisms behind explicit `target_arch` macro controls.
- [x] **HLTV Active Frame Injection:** Implemented and verified active standalone `DRC_CMD_INEYE` frame injection in `native/src/patch/engine.rs` (StreamPatcher `NetworkMessage` match arm) with dynamic `target_player_id` extraction from capture streaks.
- [x] **Dynamic Drive Failover:** Implemented AOT capture routing, duration math parity, JIT render routing, and UI/UX export pool indicators with dynamic vector list reordering.
- [x] **Long Demo Validation & Packet Audit:** Verify parsing and slicing stability on extended demo files (>30 mins) to ensure packet index alignments do not drift.
- [x] **Global Deprecation & Cruft Purge:** Aggressively identify and delete all legacy fallback paths, unused variables, and historical code workarounds across the UI and patcher crates. *Constraint: Backwards compatibility is not required for the current application lifecycle phase.*
- [x] Feature: Add match start and demo end director events in `native/src/patch/builder.rs` for each demo (requires piping `match_start_tick` from `AnalyzerState` to `DemoData`).
- [x] Refactor: Purged vestigial directory picker from the JSON project load sequence.
- [x] Feature: Transposed Workspace UI elements (Paths/Disk Space) and added Pending/Captured/Rendered tracking columns to the Master List.
- [x] Epic: Capture Studio Master-Detail Refactor — Pivot the GUI from a linear wizard to a unified Master-Detail layout (mirroring the Demo Analyzer). 
    - Phase 1: Deprecate `target_player` and `min_kills` legacy pre-scan filters. Merge `CaptureStudioState::Scan` and `Select` into a single `Workspace` enum.
    - Phase 2: Build the top-pane Master List (`egui_extras::TableBuilder`) for directory routing and demo file tracking.
    - Phase 3: Build the bottom-pane Detail View to conditionally render highlight checkboxes based on the active Master List selection.
- [x] Feature: Implement "Session Restore" on startup (Detects unclean exits and prompts user to restore state).
- [x] Review and finalize the Rust `Drop` trait "Garbage Collector" to ensure `DOD_TOOLS_EXIT_TRIGGER` and junction links are safely cleaned up on program exit/panic.
- [x] Feature: Set up an automated directory watch mechanism looking out for local `DOD_BATCH_DONE` signals.
- [x] Chore: Build absolute workspace path anchors derived natively from `std::env::current_exe()`.
- [x] Implement initial commands (Injected as raw text, limited to 50-60 characters per line).
- [x] Test and verify the custom BEFORE/AFTER command injection logic.
- [x] Audit `sys_` aliases for utilization or pruning.
- [x] Bundle `ffmpeg` directly with the program.
- [x] [UI/State] Demo Queue Session Import/Export: Implemented async JSON export/import with FNV-1a hash-based fallback validation and metadata persistence (highlights/ranges).

## Future Upgrades / R&D
- **External Demo Playback:** Investigate if DoD `.dem` files can be parsed and rendered outside the game engine (e.g., a web browser or lightweight desktop app) to preview killstreaks quickly.
- **Mode Toggle:** Add functionality to Capture Studio to switch between "Timing Mode" and "Capture Mode".
- **[Optimization] Session Compression & Modular Export:** Add options for Gzip/Zstd compression for JSON and "Selective Export" feature flags (e.g., export demos only vs. export full project metadata).




chris random manually added ideas for tasks (NOT TO BE DONE YET):

- Add a project type to Capture Studio (e.g. for solo movie, frags of the week, team movie, etc)
- Add additional filters for Capture Studio (e.g. hide demos with no kills, hide demos with no streaks, hide demos with no highlights, etc)



