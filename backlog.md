# 📋 Project Backlog & Future Improvements

## 1. Completed Tasks (Recently Done)

### Memory Optimization & Patcher Robustness
* **Memory Optimization:** Migrated to a flat `egui_extras::TableBuilder` hierarchy per demo.
* **Parser Desync Fix:** Fixed `NetworkMessage` alignment to 468 bytes.
* **Decoupled Patcher & Telemetry:** Threaded the batch patcher, added f32 progress, and 2MB safety caps.
* **Timeline Pre-calculation:** Built `K98, (+0:15) Luger` strings in background via absolute frame timestamps.
* **UI-Level POV Filtering:** Added UI toggle for non-recording players to retain safe parser data.

### Phase 6 (The Ingestion Engine)
* **Multithreading & Memory Safety:** Replaced single-threaded blocking UI with a background worker thread (`std::thread::Builder`) using a 16MB stack. Migrated all fixed-size array byte buffers in `patch.rs` to heap-allocated `Vec<u8>` to permanently resolve `STATUS_STACK_BUFFER_OVERRUN` crashes.
* **Fault Tolerance:** Eliminated all panics (`.unwrap()`) in the binary parser. The engine now safely catches malformed or corrupted `.dem` files, logs a warning, and gracefully skips them without crashing.
* **HLTV Safeguard:** Implemented a header-inspection function (`is_hltv_demo`) to automatically detect and skip HLTV proxy network streams before they break the POV parser.
* **UI Sync:** Added live progress tracking ("Scanning demo X of Y") and disk-based crash logging (`crash_log.txt`) without choking the `egui` render loop.
* **Grouping:** Finished caching models for queue grouping (By Demo, By Player, Flat) to prepare highlights for batch processing.

### Phase 7 (The HLAE Game Capture Engine)
* **Goal:** Built the engine that physically pilots Half-Life Advanced Effects (HLAE) to record the patched demos.
* **Architecture:** Implemented a background worker thread (`capture_engine.rs`) that executes HLAE (`std::process::Command`) with AFX Hook DLLs, waits sequentially, and verifies output recording artifacts.
* **Thread-Safe State & Poison Mitigation:** Upgraded global discovery collections (`QUEUED_DEMOS`, caching layers) from basic `Box` wrappers to thread-safe `Arc<Mutex<Vec<T>>>` patterns with full poison recovery (`PoisonError::into_inner`).
* **UI Reactivity:** Decoupled rendering passes from long-lived locks (cloning state under micro-locks) and implemented a time-and-batch-gated repaint manager (>16ms/5-demo throttled `ctx.request_repaint()`).
* **Semantic Constraint:** Formally re-asserted "HLAE Game Capture" pipeline semantics (specifically hook DLL injection command lines like `-hookDllPath` and `-programPath`), reverting a semantic drift to "Native".

## 2. Next Up / High Priority for Next Session: Unify Killstreak Segmentation & UI Parity

### Task A: Re-use Analyzer Streak Segmentation (`demo_parser.rs`)
* **Context:** The Capture Studio currently groups all kills into one streak. However, the `analysis` crate already perfectly segments `Player.kill_streaks` bounded by player life.
* **Implementation:** Audit how the `analysis` module builds the `kill_streaks` vector (specifically how it listens for `DeathMsg` and Round Restarts). Replicate this exact life-bounded segmentation logic in the Capture Studio's `demo_parser.rs`.

### Task B: Demo UI Containers & Controls (`capture_ui.rs`)
* **Context:** Wrap each demo in a visual box and add bulk actions.
* **Implementation:** Use `egui::Frame::group`. Below the header, add `Select All`, `Deselect All`, and `🗑 Remove Demo` (mutating the shared state safely post-render).

### Task C: Table Parity with Player Details (`capture_ui.rs`)
* **Context:** The new `TableBuilder` columns must visually match the `render_kill_streaks_table` found in the Player Details UI.
* **Implementation:** 
  - Define columns: `[Checkbox] | [Player Name] | [Kills] | [Duration] | [Details]`.
  - Re-use or replicate the `format_duration_ms` helper to display the duration cleanly.
  - **Crucial:** Render the top-level streak row ONLY. Do NOT port over the nested sub-rows for individual weapon kills, and do not include the weapon filter checkboxes.
  - In the Details column, display the pre-calculated `K98, (+0:15) Luger` timeline string.

## 3. Future Tech Debt & Maintenance

### Task D: Architectural Decoupling & File Cleanup
* **Context:** As features have expanded, files like `capture_ui.rs`, `patch.rs`, and the UI views have become monolithic. 
* **Implementation:** Review the project to decouple logic, abstract UI components into separate modules/files, and clean up deprecated code. This is an ongoing radar item to maintain code health and readability.

### Crate Upgrade Radar
* **HLTV Parser Upgrade:** Reverse-engineer the GoldSrc HLTV proxy byte structure in `patch.rs` to allow the engine to parse massive server-side demos and generate player-specific queues.
* **Rendering & Finalization:** Handling the resulting `.mov` files.