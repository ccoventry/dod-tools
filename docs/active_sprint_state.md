## Web AI State
* **Current Goal:** Frontend Migration (Tauri & Vite Integration on branch `feature/tauri-migration`).
* **Last Evaluated:** Wired `PatcherConfig` payload hydration (`capture_manager.rs`) and OS-level disk capacity checks to the Vite DOM (`main.js`, `index.html`).
* **Next Step:** Implement frontend execution logic for the capture and render batch queues.

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
* **Current Goal:** Frontend Migration (Tauri & Vite Integration on branch `feature/tauri-migration`).
* **Last Evaluated:** `desktop-studio/src-tauri/src/capture_manager.rs`, `desktop-studio/src/main.js`, `desktop-studio/index.html`.
* **Status:** Verified `calculate_export_pool_space` IPC command and `PatcherConfig` input hydration. Application compiles cleanly and executes via `npx tauri dev`.
* **Next Target:** `desktop-studio/src/main.js` (Batch execution & IPC progress event listeners).
