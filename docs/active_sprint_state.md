## Web AI State
- **Overarching Goal:** Transitioning to creative workflow (DoD movie clips); Infrastructure maintenance complete.
- **Last Touched Modules:** `builder.rs`, `capture_engine.rs`, `types.rs`, `widgets.rs`, `panels.rs`, `payload.rs`.
- **Completed:** Capture Studio Pipeline (Dynamic Routing, Capacity Simulation, VDM Generation).
- **Next Active Phase:** Render View Multi-Folder Registry (Phase 1).
- **Upcoming Tasks:** Transition the Render view UI from a single `source_folder` input to a `queued_folders: Vec<PathBuf>` collection to support reading from the newly generated multi-drive routing paths. Update the backend scanner hook to accept and iterate through this vector.

## Active Epics
- **Dynamic Drive Failover:** COMPLETED
  - **AOT Capture Routing:** Automated Ahead-Of-Time capacity simulation loop that calculates disk footprint before execution and deploys NTFS directory junctions to swap output drives when a disk drops below 15 GB.
  - **Duration Math Parity:** Abstracted a unified `calculate_total_capture_duration` method on `PatcherConfig` to ensure UI disk estimates and backend AOT math accurately isolate recording boundaries and exclude non-capturing engine phases.
  - **JIT Render Routing:** Just-In-Time threshold polling loop for the FFmpeg pipeline that guarantees a target export drive has >20 GB of free space prior to spawning a high-framerate mezzanine transcode.
  - **UI/UX Polish:** Integrated dynamic vector list reordering (⬆/⬇ swap controls), removed deprecated individual directory pickers, and mounted a global "Total Export Pool Free" indicator on the Render view.

## IDE AI State
- **Open Documents:** `docs/staging_lessons.md`, `docs/active_sprint_state.md`, `docs/engineering_backlog.md`.
- **Current Branch:** dev
- **Status:** All requested refactorings, policy-driven GC changes, Drop trait safety mechanisms, and documentation audits are fully implemented. Standing by.
