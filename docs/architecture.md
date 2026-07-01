# 🏗️ Rust Architecture & UI Constraints

## 🖥️ Immediate-Mode UI & egui Performance
- **Zero-Allocation Render Loops:** Do not execute `format!()`, string concatenation, or heavy `.clone()` operations inside the core layout or `update()` loop. Pre-calculate complex text layouts in background workers and expose them via static references to prevent frame drops.
- **Anti-Recursion Virtualization:** Never nest dynamically expanding elements (such as `CollapsingHeader`) inside virtualized row structures (`show_rows`). Complex lists of structured records must strictly utilize flat `egui_extras::TableBuilder` hierarchies to prevent 1MB Windows stack overflows (`0xc0000409`).
- **UI-Level Data Ingestion:** Background ingestion parsers must never filter or drop valid data streams to save space. Retain all states cleanly in memory and execute player visibility selections or item filters exclusively at the visual UI layer to guarantee layout safety.
- **Lock-Free Read Parity:** Highly accessed configuration catalogs or translation asset maps must be wrapped in `std::sync::RwLock` to enable concurrent multi-reader access without causing main thread lock contention stutters.

## 🧵 Threading, Concurrency & Telemetry
- **Main Thread Protection:** All long-running disk I/O, binary stream patches, and process management routines are strictly barred from the main UI thread. Offload execution to dedicated background threads using `std::thread::Builder`.
- **Throttled Progress Telemetry:** Background threads reporting progress percentages must debounce updates using a lock-free `Arc<AtomicU32>` tracking framework to throttle channel traffic to ~30fps (33ms), preventing the event loop from flooding while calling `ctx.request_repaint()`.
- **Eager Control Flags:** Toggle active UI state flags (e.g., `capture_engine_running = true`) to true *prior* to spawning threads. Enforce strict layout gating using `ui.add_enabled(!state)` to block double-launch actions.

## 💾 Process Lifecycles & Memory Safety
- **Interruptible Process Management:** Avoid using blocking `child.wait()` statements on external tasks. Use a polling infrastructure executing `child.try_wait()` matched with a ~16ms cadence thread sleep, verifying a shared `Arc<AtomicBool>` cancellation token on every cycle to safely execute `child.kill()`.
- **Headless Process Cleansing:** External process execution wrappers targeting `hlae.exe` or `ffmpeg.exe` must explicitly chain `.kill_on_drop(true)` to guarantee zombie subprocesses are reaped instantly if the parent application closes.
- **Heap-Allocated Parsing Safeguards:** Binary stream slicing operations are prohibited from using fixed-size stack buffers (`[u8; N]`), which risk instant memory failures. All parsing targets must use heap-allocated `Vec<u8>` gated by explicit 2MB payload limits checked right after reading size metrics.
- **Defensive CWD Pathing:** Never execute configuration tracking against relative text filenames. Explicitly bind adjacent configuration parameters to absolute paths derived from `std::env::current_exe()`.
- **Target WASM Segregation:** The graphical interface target compiles to `wasm32-unknown-unknown`. Isolate all native multi-threading, direct file system structures (`std::fs`), and external commands (`std::process::Command`) behind strict `#[cfg(not(target_arch = "wasm32"))]` compilation macros.
