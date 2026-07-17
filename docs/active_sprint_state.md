## Web AI State
* **Current Goal:** Resolve timeline hydration desync and demo truncation bugs to restore accurate highlight capture sequences.
* **Last Edited Component:** `native/src/patch/builder.rs` (`build_batch_queue`).
* **Unresolved Bugs:** The capture engine starts at Tick 3 because it reads UI kill indices as physical frames. Playback truncates at ~15 frames because `DEMO_END` is bound to the localized streak array instead of `total_demo_frames`.

## Active Epics
- **Dynamic Drive Failover:** COMPLETED
  - **AOT Capture Routing:** Automated Ahead-Of-Time capacity simulation loop that calculates disk footprint before execution and deploys NTFS directory junctions to swap output drives when a disk drops below 15 GB.
  - **Duration Math Parity:** Abstracted a unified `calculate_total_capture_duration` method on `PatcherConfig` to ensure UI disk estimates and backend AOT math accurately isolate recording boundaries and exclude non-capturing engine phases.
  - **JIT Render Routing:** Just-In-Time threshold polling loop for the FFmpeg pipeline that guarantees a target export drive has >20 GB of free space prior to spawning a high-framerate mezzanine transcode.
  - **UI/UX Polish:** Integrated dynamic vector list reordering (⬆/⬇ swap controls), removed deprecated individual directory pickers, and mounted a global "Total Export Pool Free" indicator on the Render view.

## IDE AI State
* **Overarching Goal:** Resolve timeline hydration desync and demo truncation bugs by mapping float times to physical frames using a linear search on `frame_times`.
* **Last Edited File:** `native/src/patch/builder.rs`.
* **Next Intended Edit:** `native/src/patch/builder.rs` to implement the linear search mapping for float times.
* **Status:** Restored builder.rs to native bounds. Ready to implement proper float-time mapping for timeline anchors.
