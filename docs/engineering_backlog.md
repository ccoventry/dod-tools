# Engineering Backlog

## 📋 General Backlog (Future Roadmap Items)
- [ ] Feature: Add match start and demo end director events in `native/src/patch/builder.rs` for each demo (requires piping `match_start_tick` from `AnalyzerState` to `DemoData`).
- [ ] Refactor: Isolate native engine stream slicing mechanisms behind explicit `target_arch` macro controls.

## Immediate Tasks (Next Session)
- [ ] **Long Demo Validation & Packet Audit:** Verify parsing and slicing stability on extended demo files (>30 mins) to ensure packet index alignments do not drift.
- [ ] **Global Deprecation & Cruft Purge:** Aggressively identify and delete all legacy fallback paths, unused variables, and historical code workarounds across the UI and patcher crates. *Constraint: Backwards compatibility is not required for the current application lifecycle phase.*

## Completed
- [x] **Tauri & Vite Integration:** Built native IPC bridge in `desktop-studio/src-tauri/src/lib.rs`, Master-Detail Vite workspace UI layout in `desktop-studio/src/main.js` & `index.html`, native system directory dialogs via `@tauri-apps/plugin-dialog`, and project session JSON persistence using `@tauri-apps/plugin-fs` (`readTextFile`/`writeTextFile`).
- [x] **Vite Master-Detail Workspace UI:** Implemented top-pane Master List Table and bottom-pane Detail View in `desktop-studio/src/main.js` and `index.html`.
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

