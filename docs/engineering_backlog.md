# Engineering Backlog

## 📋 General Backlog (Future Roadmap Items)
- [ ] Refactor: Isolate native engine stream slicing mechanisms behind explicit `target_arch` macro controls.

## Immediate Tasks (Next Session)
- [ ] Review and finalize the Rust `Drop` trait "Garbage Collector" to ensure `DOD_TOOLS_EXIT_TRIGGER` and junction links are safely cleaned up on program exit/panic.

## Completed
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
