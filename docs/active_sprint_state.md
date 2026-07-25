## Web AI State
* **Current Goal:** Frontend Migration (Tauri & Vite Integration on branch `feature/tauri-migration`).
* **Last Evaluated:** `desktop-studio/` workspace directory and remote branch `origin/feature/tauri-migration`.

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
* **Overarching Goal:** Frontend Migration (Tauri & Vite Integration).
* **Status:** Active frame injection implementation and dynamic target player index extraction verified and completed.
