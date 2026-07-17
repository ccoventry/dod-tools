## Web AI State
- **Overarching Goal:** Pipeline Enhancement - Director Events Injection (Phase 1).
- **Last Touched Modules:** `main.rs`, `workspace.rs`, `mod.rs` (Capture View).
- **Current Status:** Non-destructive path resolution logic and Master List UI sync bugs are fully resolved. Build is compiling successfully.
- **Next Action:** Extend `DemoData` struct (in `native/src/bin/gui/types.rs`) with `match_start_tick` and `demo_end_tick`, and update the ingestion scanner (`native/src/patch/scanner.rs`) to capture these events.

## Active Epics
- **Dynamic Drive Failover:** COMPLETED
  - **AOT Capture Routing:** Automated Ahead-Of-Time capacity simulation loop that calculates disk footprint before execution and deploys NTFS directory junctions to swap output drives when a disk drops below 15 GB.
  - **Duration Math Parity:** Abstracted a unified `calculate_total_capture_duration` method on `PatcherConfig` to ensure UI disk estimates and backend AOT math accurately isolate recording boundaries and exclude non-capturing engine phases.
  - **JIT Render Routing:** Just-In-Time threshold polling loop for the FFmpeg pipeline that guarantees a target export drive has >20 GB of free space prior to spawning a high-framerate mezzanine transcode.
  - **UI/UX Polish:** Integrated dynamic vector list reordering (⬆/⬇ swap controls), removed deprecated individual directory pickers, and mounted a global "Total Export Pool Free" indicator on the Render view.

## IDE AI State
- **Open Documents:** `native/src/bin/gui/main.rs`, `native/src/bin/gui/views/capture/workspace.rs`.
- **Current Branch:** dev
- **Status:** Handed off from resolving egui deprecation warning. Prepared to begin Phase 1 of Director Events Injection pipeline.
- **Next Intended Command:** `cargo run -r --bin dod-tools-gui`
