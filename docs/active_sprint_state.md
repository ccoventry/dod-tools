## Web AI State
- **Overarching Goal**: Launch and verify the native FFmpeg batch rendering and UI pipelines on the isolated `feature/tauri-migration` branch.
- **Last Edited**: `desktop-studio/src-tauri/src/capture_manager.rs` and `desktop-studio/package.json`.
- **Unresolved Compiler Errors/Bugs**: Backend compiles successfully. Awaiting Vite dev server launch to confirm `@tauri-apps/plugin-fs` resolution error is fixed.

## Active Epics
- **Rendering & Finalization:** COMPLETED (Branch: `feature/tauri-migration`)
  - **FFmpeg Transcoding & Render Studio:** Native FFmpeg encoding pipeline, `RenderBatchPayload` deserialization, lock-free atomic progress tracking, and live render status polling implemented in `render_manager.rs`.
- **Frontend Migration:** COMPLETED (Branch: `feature/tauri-migration`)
  - **Tauri & Vite Integration:** Completed frontend stack migration from native `egui` to Tauri + Vite architecture with full Master-Detail UI feature parity, native IPC handlers, dialogs, and `@tauri-apps/plugin-fs` session persistence.
- **Dynamic Drive Failover:** COMPLETED
  - **AOT Capture Routing:** Automated Ahead-Of-Time capacity simulation loop that calculates disk footprint before execution and deploys NTFS directory junctions to swap output drives when a disk drops below 15 GB.
  - **Duration Math Parity:** Abstracted a unified `calculate_total_capture_duration` method on `PatcherConfig` to ensure UI disk estimates and backend AOT math accurately isolate recording boundaries and exclude non-capturing engine phases.
  - **JIT Render Routing:** Just-In-Time threshold polling loop for the FFmpeg pipeline that guarantees a target export drive has >20 GB of free space prior to spawning a high-framerate mezzanine transcode.
  - **UI/UX Polish:** Integrated dynamic vector list reordering (⬆/⬇ swap controls), removed deprecated individual directory pickers, and mounted a global "Total Export Pool Free" indicator on the Render view.

## IDE AI State
- **Overarching Goal**: Launch and verify the native FFmpeg batch rendering and UI pipelines on the isolated `feature/tauri-migration` branch.
- **Current Branch**: `feature/tauri-migration`
- **Status**: Lessons learned and context state handoff updated. Ready for manual verification.
- **Next Command**: `cd desktop-studio && npm run tauri dev`
