# 📋 Project Backlog & Future Improvements

## 1. Completed Tasks (Recently Done)

### Memory Optimization & Patcher Robustness
* **Memory Optimization:** Eliminated `0xc0000409` stack buffer overruns by stripping nested layout virtualization and migrating to a flat `egui_extras::TableBuilder` hierarchy per demo.
* **Parser Desync Fix:** Exorcised the "4-byte ghost" in `patch.rs` `NetworkMessage` parsing (alignment fixed to 468 bytes), preventing the parser from reading garbage strings as massive integer payloads.
* **Decoupled Patcher & Telemetry:** Wrapped the batch patcher in a background thread to unfreeze the UI, added an `f32` granular progress bar, and implemented a 2MB safety cap with explicit byte-offset logging.
* **Timeline Pre-calculation:** Built the `K98, (+0:15) Luger` strings in the background ingestion worker using absolute frame timestamps (`f32` elapsed seconds) to prevent immediate-mode GUI heap allocations.
* **UI-Level POV Filtering:** Added a UI toggle to hide non-recording players, ensuring the background parser retains ALL data safely without dropping strings due to parser edge-cases.

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

## 2. Next Up / High Priority for Next Session

### Task A: Complete & Test Capture Studio Steps
* **Context:** The Select, Capture, and Render wizard steps within the Capture Studio views have not yet been fully completed or verified end-to-end.
* **Implementation:** Add integration checks and run manual verification loops for the patching (Select), HLAE program hooking/recording (Capture), and FFmpeg/mov output processing (Render) states.

### Task B: Streak Segmentation by Life
* **Context:** The parser currently groups all of a player's kills into a single streak. We need to segment them bounded strictly by the player's life.
* **Implementation:** In `demo_parser.rs`, maintain a tracking `HashMap<usize, HighlightStreak>`. On kill: add to/create active streak. On death: calculate `duration: f32`, push to finalized vector, and clear from tracker. On Round Restart/Map Reset: finalize all active streaks immediately.

### Task C: Demo UI Containers & Controls
* **Context:** The Selection UI needs better visual boxing and bulk actions per demo.
* **Implementation:** In `capture_ui.rs`, wrap each demo's header and table inside an `egui::Frame::group`. Below the header, add a horizontal block with three functional buttons: `Select All`, `Deselect All`, and `🗑 Remove Demo` (which must safely mutate the shared `Arc<Vec<DemoData>>` to eject the file).

### Task D: Table Column Redesign
* **Context:** Update the `TableBuilder` headers to reflect the new segmented data.
* **Implementation:** The columns must be exactly: `[Checkbox] | [Player Name] | [Kills] | [Duration] | [Details]`. Render the new `streak.duration` field formatted to one decimal place (`{:.1}s`).

## 3. Future Technical Debt / Phase 8+
* **HLTV Parser Upgrade:** Reverse-engineer the GoldSrc HLTV proxy byte structure in `patch.rs` to allow the engine to parse massive server-side demos and generate player-specific queues.
* **Rendering & Finalization:** Handling the resulting `.mov` files.