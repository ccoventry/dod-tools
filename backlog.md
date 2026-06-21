# 📋 Project Backlog & Future Improvements

## 1. Completed: Phase 6 (The Ingestion Engine)
* **Multithreading & Memory Safety:** Replaced single-threaded blocking UI with a background worker thread (`std::thread::Builder`) using a 16MB stack. Migrated all fixed-size array byte buffers in `patch.rs` to heap-allocated `Vec<u8>` to permanently resolve `STATUS_STACK_BUFFER_OVERRUN` crashes.
* **Fault Tolerance:** Eliminated all panics (`.unwrap()`) in the binary parser. The engine now safely catches malformed or corrupted `.dem` files, logs a warning, and gracefully skips them without crashing.
* **HLTV Safeguard:** Implemented a header-inspection function (`is_hltv_demo`) to automatically detect and skip HLTV proxy network streams before they break the POV parser.
* **UI Sync:** Added live progress tracking ("Scanning demo X of Y") and disk-based crash logging (`crash_log.txt`) without choking the `egui` render loop.
* **Grouping:** Finished caching models for queue grouping (By Demo, By Player, Flat) to prepare highlights for batch processing.

## 2. Completed: Phase 7 (The HLAE Game Capture Engine) - Integration/Debugging
* **Goal:** Built the engine that physically pilots Half-Life Advanced Effects (HLAE) to record the patched demos.
* **Architecture:** Implemented a background worker thread (`capture_engine.rs`) that executes HLAE (`std::process::Command`) with AFX Hook DLLs, waits sequentially, and verifies output recording artifacts.
* **Thread-Safe State & Poison Mitigation:** Upgraded global discovery collections (`QUEUED_DEMOS`, caching layers) from basic `Box` wrappers to thread-safe `Arc<Mutex<Vec<T>>>` patterns with full poison recovery (`PoisonError::into_inner`).
* **UI Reactivity:** Decoupled rendering passes from long-lived locks (cloning state under micro-locks) and implemented a time-and-batch-gated repaint manager (>16ms/5-demo throttled `ctx.request_repaint()`).
* **Semantic Constraint:** Formally re-asserted "HLAE Game Capture" pipeline semantics (specifically hook DLL injection command lines like `-hookDllPath` and `-programPath`), reverting a semantic drift to "Native".

## 3. Next Up / Upcoming
* **Phase 7b:** Validate HLAE injection telemetry and recording artifact generation.
* **Cleanup:** Transition all temporary test codes and debug telemetries marked with `// TODO: Cleanup` into final production code.

## 4. Future Technical Debt / Phase 8+
* **HLTV Parser Upgrade:** Reverse-engineer the GoldSrc HLTV proxy byte structure in `patch.rs` to allow the engine to parse massive server-side demos and generate player-specific queues.
* **Rendering & Finalization:** (Steps 4 and 5) Handling the resulting `.mov` files.