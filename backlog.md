# 📋 Project Backlog & Future Improvements

## 1. Completed: Phase 6 (The Ingestion Engine)
* **Multithreading & Memory Safety:** Replaced single-threaded blocking UI with a background worker thread (`std::thread::Builder`) using a 16MB stack. Migrated all fixed-size array byte buffers in `patch.rs` to heap-allocated `Vec<u8>` to permanently resolve `STATUS_STACK_BUFFER_OVERRUN` crashes.
* **Fault Tolerance:** Eliminated all panics (`.unwrap()`) in the binary parser. The engine now safely catches malformed or corrupted `.dem` files, logs a warning, and gracefully skips them without crashing.
* **HLTV Safeguard:** Implemented a header-inspection function (`is_hltv_demo`) to automatically detect and skip HLTV proxy network streams before they break the POV parser.
* **UI Sync:** Added live progress tracking ("Scanning demo X of Y") and disk-based crash logging (`crash_log.txt`) without choking the `egui` render loop.
* **Grouping:** Finished caching models for queue grouping (By Demo, By Player, Flat) to prepare highlights for batch processing.

## 2. In Progress / Next Up: Phase 7 (The Game Capture Engine)
* **Goal:** Build the engine that physically pilots Half-Life to record the patched demos.
* **Architecture:** Need to build a dedicated background worker (`capture_engine.rs`) utilizing `std::process::Command` to launch `hl.exe` sequentially.
* **Launch Arguments:** Must pass `-game dod -steam -console +playdemo [name]`.
* **Process Lifecycle:** The thread must block (`.wait()`) until the game auto-closes, then immediately verify the existence of the expected `.mov` artifact via `std::fs::metadata`.
* **UI Integration:** Build the Step 3 UI with a file picker for `hl.exe`, a launch button, and a dynamic progress bar reacting to `EngineEvent` channel messages.

## 3. Future Technical Debt / Phase 8+
* **HLTV Parser Upgrade:** Reverse-engineer the GoldSrc HLTV proxy byte structure in `patch.rs` to allow the engine to parse massive server-side demos and generate player-specific queues.
* **Rendering & Finalization:** (Steps 4 and 5) Handling the resulting `.mov` files.