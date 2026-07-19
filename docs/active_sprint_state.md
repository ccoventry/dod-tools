## Web AI State
* **Current Goal:** Isolate native engine stream slicing mechanisms behind `#[cfg(not(target_arch = "wasm32"))]` conditional compilation gates to ensure WASM GUI compilation safety.
* **Last Edited Component:** `native/src/patch/mod.rs`.
* **Unresolved Bugs:** None. The Tick 0 domain mismatch is fully resolved.

## Active Epics
- **Dynamic Drive Failover:** COMPLETED
  - **AOT Capture Routing:** Automated Ahead-Of-Time capacity simulation loop that calculates disk footprint before execution and deploys NTFS directory junctions to swap output drives when a disk drops below 15 GB.
  - **Duration Math Parity:** Abstracted a unified `calculate_total_capture_duration` method on `PatcherConfig` to ensure UI disk estimates and backend AOT math accurately isolate recording boundaries and exclude non-capturing engine phases.
  - **JIT Render Routing:** Just-In-Time threshold polling loop for the FFmpeg pipeline that guarantees a target export drive has >20 GB of free space prior to spawning a high-framerate mezzanine transcode.
  - **UI/UX Polish:** Integrated dynamic vector list reordering (⬆/⬇ swap controls), removed deprecated individual directory pickers, and mounted a global "Total Export Pool Free" indicator on the Render view.

## IDE AI State
* **Overarching Goal:** Isolate native engine stream slicing mechanisms behind `#[cfg(not(target_arch = "wasm32"))]` conditional compilation gates to ensure WASM GUI compilation safety.
* **Last Edited File:** `native/src/patch/mod.rs`.
* **Next Intended Edit:** Pending review.
* **Status:** Workspace prepared and trackers synchronized.
