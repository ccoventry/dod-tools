## Web AI State
* **Current Goal:** Frontend Migration (Tauri & Vite Integration on branch `feature/tauri-migration`).
* **Last Evaluated:** `desktop-studio/src/main.js` and `desktop-studio/index.html` (Phase 3 complete: batch capture execution button and live `capture_status` polling wired with real-time Master List row badge updates).

## Active Epics
- **Frontend Migration:** IN PROGRESS (Branch: `feature/tauri-migration`)
  - **Tauri & Vite Integration:** Phase 3 complete. Batch capture trigger, native directory dialogs, and real-time `capture_status` polling badges fully integrated.
- **Dynamic Drive Failover:** COMPLETED
  - **AOT Capture Routing:** Automated Ahead-Of-Time capacity simulation loop that calculates disk footprint before execution and deploys NTFS directory junctions to swap output drives when a disk drops below 15 GB.
  - **Duration Math Parity:** Abstracted a unified `calculate_total_capture_duration` method on `PatcherConfig` to ensure UI disk estimates and backend AOT math accurately isolate recording boundaries and exclude non-capturing engine phases.
  - **JIT Render Routing:** Just-In-Time threshold polling loop for the FFmpeg pipeline that guarantees a target export drive has >20 GB of free space prior to spawning a high-framerate mezzanine transcode.
  - **UI/UX Polish:** Integrated dynamic vector list reordering (⬆/⬇ swap controls), removed deprecated individual directory pickers, and mounted a global "Total Export Pool Free" indicator on the Render view.

## IDE AI State
* **Overarching Goal:** Frontend Migration (Tauri & Vite Integration).
* **Current Branch:** feature/tauri-migration
* **Status:** Phase 3 batch capture execution and live status listener integration verified on `feature/tauri-migration`. Standing by.
