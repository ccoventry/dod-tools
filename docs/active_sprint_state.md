## Web AI State
- **Overarching Goal:** Transitioning to creative workflow (DoD movie clips).
- **Last Touched Modules:** Workspace root (Git branch reversion).
- **Completed:** Tauri UI Migration suspended and parked. Stable `dev` branch restored. 
- **Next Active Phase:** Engineering Backlog - Pipeline Enhancement (Director Events Injection).
- **Upcoming Tasks:** Pipe `match_start_tick` from `AnalyzerState` to `DemoData` and inject `[dod-tools] MATCH_START` and `[dod-tools] DEMO_END` director events in `native/src/patch/builder.rs`.

## Active Epics
- **Dynamic Drive Failover:** COMPLETED
  - **AOT Capture Routing:** Automated Ahead-Of-Time capacity simulation loop that calculates disk footprint before execution and deploys NTFS directory junctions to swap output drives when a disk drops below 15 GB.
  - **Duration Math Parity:** Abstracted a unified `calculate_total_capture_duration` method on `PatcherConfig` to ensure UI disk estimates and backend AOT math accurately isolate recording boundaries and exclude non-capturing engine phases.
  - **JIT Render Routing:** Just-In-Time threshold polling loop for the FFmpeg pipeline that guarantees a target export drive has >20 GB of free space prior to spawning a high-framerate mezzanine transcode.
  - **UI/UX Polish:** Integrated dynamic vector list reordering (⬆/⬇ swap controls), removed deprecated individual directory pickers, and mounted a global "Total Export Pool Free" indicator on the Render view.

## IDE AI State
- **Open Documents:** `desktop-studio/src/main.js`, `desktop-studio/index.html`, `desktop-studio/src-tauri/src/render_manager.rs`, `desktop-studio/src-tauri/src/capture_manager.rs`, `native/src/hlcr/scanner.rs`.
- **Current Branch:** feature/tauri-migration
- **Status:** Phase 9-13 (Export Config UI, Parity Audit, Multi-Folder Render Ingestion, Render Studio UI, Render Batch Execution & Telemetry) completed. Workspace clean and local commits finalized. Standing by.
