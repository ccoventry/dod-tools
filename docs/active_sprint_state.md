## Web AI State
- **Overarching Goal:** "Localization & Zero-Allocation Refactor" epic is officially closed. Next phase is architectural/UI decoupling (moving baked-in features to a global toolkit/main menu).
- **Last Edited:** `localizations/dod_tools_english.txt` (added missing `#label.*` keys) and `native/src/patch/builder.rs` (fixed mock grouping assertions).
- **Unresolved Errors/Bugs:** None. The native workspace is fully localized, all GUI fallbacks are resolved, and the test suite passes 100% green. Tauri/Vite frontend integration remains paused.

## Active Epics
- **HLTV Active Frame Injection:** COMPLETED
  - **Standalone Frame Injection:** Implemented active standalone `DRC_CMD_INEYE` frame injection in `native/src/patch/engine.rs` within the StreamPatcher `NetworkMessage` match arm with dynamic `target_player_id` extraction from capture streaks.
- **Frontend Migration:** IN PROGRESS (Branch: `feature/tauri-migration`)
  - **Tauri & Vite Integration:** Transitioning frontend stack to Tauri + Vite architecture in the `desktop-studio/` workspace (`src-tauri/`).
- **Dynamic Drive Failover:** COMPLETED
  - **AOT Capture Routing:** Automated Ahead-Of-Time capacity simulation loop that calculates disk footprint before execution and deploys NTFS directory junctions to swap output drives when a disk drops below 15 GB.
  - **Duration Math Parity:** Abstracted a unified `calculate_total_capture_duration` method on `PatcherConfig` to ensure UI disk estimates and backend AOT math accurately isolate recording boundaries and exclude non-capturing engine phases.
  - **JIT Render Routing:** Just-In-Time threshold polling loop for the FFmpeg pipeline that guarantees a target export drive has >20 GB of free space prior to spawning a high-framerate mezzanine transcode.
  - **UI/UX Polish:** Integrated dynamic vector list reordering (⬆/⬇ swap controls), removed deprecated individual directory pickers, and mounted a global "Total Export Pool Free" indicator on the Render view.

## IDE AI State
- **Current Goal:** Architectural/UI decoupling (extracting baked-in tools into a global main menu).
- **Last Evaluated:** Appended session lessons to `docs/staging_lessons.md` and updated sprint state.
- **Status:** Context handoff complete. All legacy tests passing and UI fully localized. Ready to refactor architecture.
- **Next File to Open:** `native/src/bin/gui/main.rs` (to begin scaffolding the global toolkit/main menu UI).
- **Next Command:** `cargo run --bin dod-tools-gui` (to verify menu rendering).
