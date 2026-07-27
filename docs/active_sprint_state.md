## Web AI State
* **Current Goal:** Frontend Migration (Tauri & Vite Integration) - Resolving UI parity and broken functionality.
* **Last Evaluated:** Decoupled the Vite frontend into modular ES components (`ipc_bridge.js`, `capture_pane.js`, `render_pane.js`, `master_pane.js`, `detail_pane.js`) and implemented base `egui` CSS layout wrappers.
* **Unresolved State:** The Capture Studio UI is non-functional. "Scan Demos" fails because native directory routing (via Tauri dialog plugin) is disconnected. Export Configuration fields are missing from the DOM. A structural decision is pending on whether to finalize the Master-Detail architecture or revert to the legacy tabbed wizard layout.
* **Next Step:** Await confirmation on UI layout direction, wire the native folder pickers to the IPC bridge, and restore missing configuration inputs.

## Active Epics
- **Rendering & Finalization:** COMPLETED (Branch: `feature/tauri-migration`)
  - **FFmpeg Transcoding & Render Studio:** Native FFmpeg encoding pipeline, `RenderBatchPayload` deserialization, lock-free atomic progress tracking, and live render status polling implemented in `render_manager.rs`.
- **Frontend Migration:** IN PROGRESS (Branch: `feature/tauri-migration`)
  - **Tauri & Vite Integration:** Decoupled monolithic `main.js` into modular ES components (`ipc_bridge.js`, `capture_pane.js`, `render_pane.js`, `master_pane.js`, `detail_pane.js`) with Master-Detail split pane CSS layout.
- **Dynamic Drive Failover:** COMPLETED
  - **AOT Capture Routing:** Automated Ahead-Of-Time capacity simulation loop that calculates disk footprint before execution and deploys NTFS directory junctions to swap output drives when a disk drops below 15 GB.
  - **Duration Math Parity:** Abstracted a unified `calculate_total_capture_duration` method on `PatcherConfig` to ensure UI disk estimates and backend AOT math accurately isolate recording boundaries and exclude non-capturing engine phases.
  - **JIT Render Routing:** Just-In-Time threshold polling loop for the FFmpeg pipeline that guarantees a target export drive has >20 GB of free space prior to spawning a high-framerate mezzanine transcode.
  - **UI/UX Polish:** Integrated dynamic vector list reordering (⬆/⬇ swap controls), removed deprecated individual directory pickers, and mounted a global "Total Export Pool Free" indicator on the Render view.

## IDE AI State
* **Current Goal:** Frontend Migration - UI Parity & Native Plugin Dialog Integration.
* **Last Evaluated:** Decoupled `desktop-studio/src/main.js` into modular ES components (`ipc_bridge.js`, `master_pane.js`, `detail_pane.js`, `capture_pane.js`, `render_pane.js`).
* **Status:** Frontend architecture decoupled cleanly; awaiting confirmation on native folder pickers and configuration inputs restoration.
* **Next Command:** `cd desktop-studio && npx tauri dev`
* **Next File to Edit:** `desktop-studio/src/ipc_bridge.js`
