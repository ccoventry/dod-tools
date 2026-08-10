## Web AI State
- **Overarching Goal:** Architectural UI decoupling complete (Export Manager extracted, Top Navigation bar unified, contextual footers scaffolded, non-destructive loading state implemented). Next phase: Wire functional cancellation channels and finish remaining `TODO` dispatches.
- **Last Edited:** `native/src/bin/gui/views/capture/mod.rs`, `native/src/bin/gui/views/capture/workspace.rs`, `native/src/bin/gui/main.rs`.
- **Unresolved Errors/Bugs:** The "Cancel Scan" button in `CaptureState::Scanning` requires proper background channel signaling or atomic flag abort handling to stop the underlying ingestion thread.


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
- **Current Goal:** Wire functional scan cancellation (INGESTION_CANCEL AtomicBool implemented) and finish remaining `TODO` dispatches across Export Manager, Demo Auditor, and Demo Analyzer footers.
- **Last Evaluated:** Appended 3 new session lessons to `docs/staging_lessons.md`; updated both state sections in `docs/active_sprint_state.md`.
- **Status:** Cancellation infrastructure landed in `mod.rs` (`INGESTION_CANCEL` atomic, `request_ingestion_cancel()`, per-file break check). Redundant workspace banner removed. All panels compile and run.
- **Next File to Open:** `native/src/bin/gui/views/capture/mod.rs` (to verify cancellation resets `CaptureState` to `Idle` after break, ensuring the Scanning spinner dismisses correctly).
- **Next Command:** `cargo run --bin dod-tools-gui` (to verify UI renders correctly after ingestion cancellation).
