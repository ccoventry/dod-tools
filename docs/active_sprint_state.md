## Web AI State
- **Current Goal:** Tauri Migration - Desktop Studio Workspace & Master Queue Audit
- **Active Environment Path:** `~/dod-tools/desktop-studio` (Native WSL ext4)
- **Build Status:** Compiling and launching cleanly via `npx tauri dev`.
- **Pending Engineering Backlog (Workspace & Master Queue):**
  - [TASK-01] Replace broken status glyph with functional SVG/CSS trash button in Master Queue.
  - [TASK-02] Restrict demo parser strictly to local recording player POV (SteamID/Name).
  - [TASK-03] Connect Telemetry IPC parser outputs to Match Telemetry Analysis modal.
  - [TASK-04] Move visual density timeline barcode chart into Advanced Diagnostics drawer.
  - [TASK-05] Remove redundant "POV Only" top-bar toggle.
  - [TASK-06] Implement 1-based sequential row indexing in Highlights table.
  - [TASK-07] Add interactive selection checkboxes to "Sel" column.
  - [TASK-08] Implement editable Kill Range (validation, undo button, dynamic recalculation of Kills & Time).
  - [TASK-09] Convert Status column to interactive dropdown (`N/A`, `Pending`, `Captured`, `Rendered`).
  - [TASK-10] Format Details column to show kill sequence and interval offsets (`(+0:05)`).
  - [TASK-11] Prevent `Min Kills` filter from hiding or unchecking user-selected rows.

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
* **Current Action:** Workspace migrated to WSL ext4 filesystem (`~/dod-tools/desktop-studio`). Build friction rules documented in `staging_lessons.md`. Ready to proceed with Desktop Studio Workspace & Master Queue backlog tasks.
