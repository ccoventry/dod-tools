## Web AI State
* **Current Goal:** Execute the Magic Number Re-Audit across the patcher modules without altering underlying demo parsing logic boundaries (specifically `type_byte == 5`).
* **Last Edited Component:** `docs/` trackers (`staging_lessons.md`, `bugs.md`, `active_sprint_state.md`, `engineering_backlog.md`).
* **Unresolved Bugs:** None. The Tick 0 domain mismatch is fully resolved.

## Active Epics
- **Dynamic Drive Failover:** COMPLETED
  - **AOT Capture Routing:** Automated Ahead-Of-Time capacity simulation loop that calculates disk footprint before execution and deploys NTFS directory junctions to swap output drives when a disk drops below 15 GB.
  - **Duration Math Parity:** Abstracted a unified `calculate_total_capture_duration` method on `PatcherConfig` to ensure UI disk estimates and backend AOT math accurately isolate recording boundaries and exclude non-capturing engine phases.
  - **JIT Render Routing:** Just-In-Time threshold polling loop for the FFmpeg pipeline that guarantees a target export drive has >20 GB of free space prior to spawning a high-framerate mezzanine transcode.
  - **UI/UX Polish:** Integrated dynamic vector list reordering (⬆/⬇ swap controls), removed deprecated individual directory pickers, and mounted a global "Total Export Pool Free" indicator on the Render view.

## IDE AI State
* **Overarching Goal:** Execute the Magic Number Re-Audit to scan and refactor all hardcoded magic numbers into shared module constants.
* **Last Edited File:** `docs/active_sprint_state.md`.
* **Next Intended Edit:** `native/src/patch/mod.rs`.
* **Status:** Workspace prepared and trackers synchronized. Standing by for session reset.
