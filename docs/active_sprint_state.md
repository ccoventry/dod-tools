## Web AI State
* **Current Goal:** Transition to the next engineering backlog item (Magic Number Audit). Ensure the timeline hydration and Tick 0 bugs are fully resolved across all edge cases (fresh scan and project load).
* **Last Edited Component:** `native/src/patch/builder.rs` (`find_tick_backwards`, `find_tick_forwards`, `build_batch_queue`).
* **Unresolved Bugs:** None.

## Active Epics
- **Dynamic Drive Failover:** COMPLETED
  - **AOT Capture Routing:** Automated Ahead-Of-Time capacity simulation loop that calculates disk footprint before execution and deploys NTFS directory junctions to swap output drives when a disk drops below 15 GB.
  - **Duration Math Parity:** Abstracted a unified `calculate_total_capture_duration` method on `PatcherConfig` to ensure UI disk estimates and backend AOT math accurately isolate recording boundaries and exclude non-capturing engine phases.
  - **JIT Render Routing:** Just-In-Time threshold polling loop for the FFmpeg pipeline that guarantees a target export drive has >20 GB of free space prior to spawning a high-framerate mezzanine transcode.
  - **UI/UX Polish:** Integrated dynamic vector list reordering (⬆/⬇ swap controls), removed deprecated individual directory pickers, and mounted a global "Total Export Pool Free" indicator on the Render view.

## IDE AI State
* **Overarching Goal:** Execute a Magic Number Audit across the patch module to scan the codebase for hardcoded limits and consolidate them as clean configuration constants.
* **Last Edited File:** `native/src/patch/builder.rs`.
* **Next Intended Edit:** Scan the patch/ and bin/ crates to locate remaining hardcoded numbers.
* **Status:** Resolved all compilation errors and verified tick injection calculations on both scenario paths (fresh scans and project loads) via `check_ticks`. Code builds cleanly and is ready for constants refactoring.
