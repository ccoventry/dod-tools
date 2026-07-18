## Web AI State
* **Current Goal:** Resolve timeline hydration desync and demo truncation bugs (the Tick 0 bug). The pipeline now successfully uses a HashMap to route global frame arrays, but the output still anchors to Tick 0 and truncates at ~56 frames. This heavily implies the `scanner.rs` is still populating the 7th tuple element (`frame_times_arc`) with a localized slice instead of the true global array.
* **Last Edited Component:** `native/src/patch/builder.rs` (`build_batch_queue`), `native/src/patch/scanner.rs`, and `native/src/bin/gui/views/capture/capture.rs`.
* **Unresolved Bugs:** The capture commands still anchor to Tick 0. The core issue remains: the data inside the piped `frame_times` array is still localized to the streak, failing the absolute float-time linear search.

## Active Epics
- **Dynamic Drive Failover:** COMPLETED
  - **AOT Capture Routing:** Automated Ahead-Of-Time capacity simulation loop that calculates disk footprint before execution and deploys NTFS directory junctions to swap output drives when a disk drops below 15 GB.
  - **Duration Math Parity:** Abstracted a unified `calculate_total_capture_duration` method on `PatcherConfig` to ensure UI disk estimates and backend AOT math accurately isolate recording boundaries and exclude non-capturing engine phases.
  - **JIT Render Routing:** Just-In-Time threshold polling loop for the FFmpeg pipeline that guarantees a target export drive has >20 GB of free space prior to spawning a high-framerate mezzanine transcode.
  - **UI/UX Polish:** Integrated dynamic vector list reordering (⬆/⬇ swap controls), removed deprecated individual directory pickers, and mounted a global "Total Export Pool Free" indicator on the Render view.

## IDE AI State
* **Overarching Goal:** Investigate why `frame_times_arc` in `scanner.rs` behaves as a localized slice rather than the global demo frame array, causing Tick 0 anchoring.
* **Last Edited File:** `native/src/bin/gui/main.rs` & `native/src/bin/test_builder.rs`.
* **Next Intended Edit:** `native/src/patch/scanner.rs` to audit how the global `frame_times` array is compiled from demo chunks.
* **Status:** Resolved all compilation errors (`E0308`, `E0063`, `E0425`) resulting from the HashMap pipeline migration. The codebase compiles successfully, but the Tick 0 anchoring bug persists due to localized frame arrays.
