## Web AI State
* **Current Goal:** Parity and functional reconciliation sprint complete across Phase 1 (Render Studio), Phase 2 (Analysis & Telemetry IPC), and Phase 3 (UX Polish, Filtering & Timeline Canvas).
* **Last Edited:** `desktop-studio/src/toast.js`, `desktop-studio/src/detail_pane.js`, `desktop-studio/src/master_pane.js`, `desktop-studio/src-tauri/src/capture_manager.rs`, `desktop-studio/src-tauri/src/lib.rs`, `desktop-studio/index.html`, `.cargo/sign_and_run.ps1`.
* **Unresolved Bugs:** Local binary execution blocked by Windows 11 Smart App Control cloud reputation check (requires disabling SAC in Windows Security for local dev execution).

## Active Epics
- **Rendering & Finalization:** COMPLETED (Branch: `feature/tauri-migration`)
  - **FFmpeg Transcoding & Render Studio:** Native FFmpeg encoding pipeline, `RenderBatchPayload` deserialization, lock-free atomic progress tracking, and live render status event emissions implemented in `render_manager.rs`.
- **Frontend Migration:** COMPLETED (Branch: `feature/tauri-migration`)
  - **Tauri & Vite Integration:** Core IPC pipeline, native commands, Telemetry modal, Master List search filter, bulk streak selection, and proportional streak timeline canvas fully operational with 1:1 parity against legacy egui implementation.
- **Dynamic Drive Failover:** COMPLETED
  - **AOT Capture Routing:** Automated Ahead-Of-Time capacity simulation loop that calculates disk footprint before execution and deploys NTFS directory junctions to swap output drives when a disk drops below 15 GB.
  - **Duration Math Parity:** Abstracted a unified `calculate_total_capture_duration` method on `PatcherConfig` to ensure UI disk estimates and backend AOT math accurately isolate recording boundaries and exclude non-capturing engine phases.
  - **JIT Render Routing:** Just-In-Time threshold polling loop for the FFmpeg pipeline that guarantees a target export drive has >20 GB of free space prior to spawning a high-framerate mezzanine transcode.
  - **UI/UX Polish:** Integrated dynamic vector list reordering (⬆/⬇ swap controls), removed deprecated individual directory pickers, mounted global export free indicators, and implemented floating toast notification controller.

## IDE AI State
* **Current Action:** Architecture baseline updated. All 3 migration phases (Render Studio, Analysis/Telemetry IPC, UX Polish & Timeline Canvas) complete with 1:1 parity against legacy egui implementation. Ready for deployment testing once SAC is bypassed or binary is published.
