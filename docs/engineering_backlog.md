# Engineering Backlog

## 🚀 Active Sprint Priorities
- [ ] Task: Audit and strip lingering template text lines out of `docs/app_architecture.md`.
- [ ] Task: Ensure all local terminal pipelines cleanly integrate the `.cursorrules` `cmd 2>&1 | tuf` parameter sequence.
- [ ] Task: Verify that the `build_bootstrap.ps1` dynamic file utility executes targeted laser payloads under 50 lines.

## 📋 General Backlog (Future Roadmap Items)
- [ ] Feature: Set up an automated directory watch mechanism looking out for local `DOD_BATCH_DONE` signals.
- [ ] Refactor: Isolate native engine stream slicing mechanisms behind explicit `target_arch` macro controls.
- [ ] Chore: Build absolute workspace path anchors derived natively from `std::env::current_exe()`.

## 🛑 Non-Goals (Out of Scope)
- No support for native engine multi-threading layers operating inside immediate-mode UI thread blocks.
- No direct architectural support for alternative Source/GoldSrc modifications outside of Day of Defeat 1.3.

## Immediate Tasks (Next Session)
- Investigate audio glitches during `host_framerate` changes (fast-forwarding).
- Implement initial commands from the Settings page into `dodtools_helper` aliases, ensuring they are injected at the correct times in the demo.
- Test and verify the custom BEFORE/AFTER command injection logic.
- Audit `sys_` aliases (e.g., `sys_record_start`) to determine if they are actually being utilized by multi-command injections, or if they can be pruned (e.g., `sys_capture_done_path` currently calls the raw commands directly).
- Bundle `ffmpeg` directly with the program to eliminate the need for manual downloads when testing the rendering page.
- Evaluate the necessity of the Settings page once the Demo Analyzer and Capture Studio are split into separate projects.
- Implement a Rust `Drop` trait "Garbage Collector" to safely delete the `dodtools_session` junction, `DOD_TOOLS_EXIT_TRIGGER`, and helper configs if the program exits or panics.

## Future Ideas / R&D
- **External Demo Playback:** Investigate if DoD `.dem` files can be parsed and rendered outside the game engine (e.g., a web browser or lightweight desktop app) to preview killstreaks quickly.
- **Mode Toggle:** Add functionality to Capture Studio to switch between "Timing Mode" and "Capture Mode".
- **Session State Management:** Implement Import/Export/Save/Load functionality for demo settings so users can save their timing progress and resume batch capturing later.
