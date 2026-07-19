## Web AI State
* **Current Goal:** Safely hijack the HLTV Auto-Director by forcing the camera to a specific Player ID.
* **Last Evaluated:** `native/src/patch/engine.rs` (StreamPatcher block).
* **Blocker/Pivot:** Blind byte injection via `diagnostic_hltv_injector.rs` failed due to strict protocol constraints.
* **Next Action:** Implement a custom deserializer loop within the `NetworkMessage` match arm in `engine.rs` to intercept `svc_director` packets, mutate the target entity ID in-place, and rewrite the payload at the exact original byte length.

## Active Epics
- **Dynamic Drive Failover:** COMPLETED
  - **AOT Capture Routing:** Automated Ahead-Of-Time capacity simulation loop that calculates disk footprint before execution and deploys NTFS directory junctions to swap output drives when a disk drops below 15 GB.
  - **Duration Math Parity:** Abstracted a unified `calculate_total_capture_duration` method on `PatcherConfig` to ensure UI disk estimates and backend AOT math accurately isolate recording boundaries and exclude non-capturing engine phases.
  - **JIT Render Routing:** Just-In-Time threshold polling loop for the FFmpeg pipeline that guarantees a target export drive has >20 GB of free space prior to spawning a high-framerate mezzanine transcode.
  - **UI/UX Polish:** Integrated dynamic vector list reordering (⬆/⬇ swap controls), removed deprecated individual directory pickers, and mounted a global "Total Export Pool Free" indicator on the Render view.

## IDE AI State
* **Overarching Goal:** Safely hijack the HLTV Auto-Director by forcing the camera to a specific Player ID via in-place `svc_director` payload mutation.
* **Last Edited File:** `docs/staging_lessons.md`.
* **Next Intended Edit:** `native/src/patch/engine.rs`.
* **Status:** Diagnostic analysis complete; ready for in-place packet deserializer implementation.
