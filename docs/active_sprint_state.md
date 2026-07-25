## Web AI State
* **Current Goal:** Rendering & Finalization Epic Completed.
* **Last Evaluated:** `desktop-studio/src-tauri/src/render_manager.rs` and `desktop-studio/src/main.js` (Native FFmpeg batch rendering, lock-free atomic progress tracking, and Render Studio IPC interop fully completed).

## Active Epics
- **Rendering & Finalization:** COMPLETED (Branch: `dev`)
  - **FFmpeg Transcoding & Render Studio:** Native FFmpeg encoding pipeline, `RenderBatchPayload` deserialization, lock-free atomic progress tracking, and live render status polling implemented in `render_manager.rs`.
- **Frontend Migration:** COMPLETED (Branch: `dev`)
  - **Tauri & Vite Integration:** Completed frontend stack migration from native `egui` to Tauri + Vite architecture with full Master-Detail UI feature parity, native IPC handlers, dialogs, and `@tauri-apps/plugin-fs` session persistence.
- **Dynamic Drive Failover:** COMPLETED
  - **AOT Capture Routing:** Automated Ahead-Of-Time capacity simulation loop that calculates disk footprint before execution and deploys NTFS directory junctions to swap output drives when a disk drops below 15 GB.
  - **Duration Math Parity:** Abstracted a unified `calculate_total_capture_duration` method on `PatcherConfig` to ensure UI disk estimates and backend AOT math accurately isolate recording boundaries and exclude non-capturing engine phases.
  - **JIT Render Routing:** Just-In-Time threshold polling loop for the FFmpeg pipeline that guarantees a target export drive has >20 GB of free space prior to spawning a high-framerate mezzanine transcode.
  - **UI/UX Polish:** Integrated dynamic vector list reordering (⬆/⬇ swap controls), removed deprecated individual directory pickers, and mounted a global "Total Export Pool Free" indicator on the Render view.

## IDE AI State
* **Overarching Goal:** Rendering & Finalization Epic.
* **Current Branch:** dev
* **Status:** Rendering & Finalization epic completed on `dev`. All documentation synchronized across workspace. Standing by.
