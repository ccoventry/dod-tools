---
trigger: always_on
description: Architectural constraints regarding Rust, tokio, egui UI, Threading, and WASM compilation.
---

# 🏗️ Rust Architecture & UI Constraints

### 🖥️ UI & egui Layout
* **egui Stack Overflows (`0xc0000409`):** Nesting dynamically expanding containers (like `CollapsingHeader`) inside virtualized `show_rows` triggers `0xc0000409` 1MB Windows stack overflows. Large lists of structured data must strictly use flat `egui_extras::TableBuilder` hierarchies without inner recursion.
* **Zero-Allocation UI Loops:** `egui` runs in immediate mode. Never use `format!()`, string concatenation, or heavy `.clone()` operations in the render loop; this drops the framerate. Pre-calculate all complex strings in background workers and pass them as static state.
* **Dynamic Table Filtering in TableBuilder:** When filtering rows in `egui_extras::TableBuilder`, pre-calculate visible row indices in a vector and pass the filtered length to `body.rows()`. Relying on conditional skips (`continue`) inside the row drawer without adjusting row count breaks virtualization layout heights.
* **Visual Ingestion Retention (Blank Screen Prevention):** Avoid filtering out non-POV players or specific classes during low-level demo parsing. Retain all records in memory and handle player filters visually at the UI/Table rendering level. This prevents blank frames if the parser hits structural edge-cases.
* **Directory State Override:** The Explorer UI tree must not force auto-expansion on every frame, as this prevents manual folder collapsing. Expansion state must only trigger on selection transitions, not frame-by-frame polling.

### 🧵 Threading & Concurrency
* **Egui Immediate-Mode Thread Blocking:** Executing `std::fs::read`, heavy loops, or holding `Arc<Mutex>` locks globally inside the `egui` `update()` loop freezes the UI. All heavy tasks (binary patching, HLAE execution) must run on background OS threads using `std::thread::Builder`.
* **Egui Repainting During Background Tasks:** To prevent egui from entering a sleeping state while background tasks run, invoke `ctx.request_repaint()` continuously in the UI loop when a background worker is active.
* **Micro-Lock State Extraction:** Never hold `Mutex` or `RwLock` guards across asynchronous boundaries or during complex UI table rendering. Clone required state (e.g., `QUEUED_DEMOS` Arc reference) under micro-locks and drop the guards immediately to prevent deadlocks and frame stuttering.
* **UI Thread Contention (Localization):** Translation lookups and localization file access were initially using `Mutex` locks, causing UI frame stutters. These must use `RwLock` to enable concurrent reader access.
* **Background Thread Traps:** Pre-flight checks inside worker threads that fail silently (e.g., using `.ok_or()` or returning early) will permanently hang the UI progress state. All early thread exits must explicitly send a `GuiMessage::Error` over the MPSC channel.
* **Channel Drop Failures:** Silently dropping MPSC sender channels (via `.ok()`) causes background workers to hang indefinitely if they rely on channel status to know when to terminate. Explicitly check return results (`.is_err()`) to signal worker termination.
* **Double-Fire Background Threads:** Relying purely on the `mpsc::channel` events to update running states allows a one-frame window where users can double-click a launch button. UI state flags (like `capture_engine_running = true`) must be set *eagerly* immediately prior to the `std::thread::spawn` call.
* **Atomic vs Mutex Performance:** For high-frequency progress callbacks (e.g., 30fps debouncing), `Mutex<f32>` introduces unnecessary context-switching overhead. Lock-free `AtomicU32` tracking (milliseconds) is the preferred pattern for progressive UI updates.

### 💾 I/O & File Management
* **OS Error 32 (Sharing Violations):** Windows NTFS aggressively holds file locks on `.dem` files. 
  * Never store `File`, `BufReader`, or `Mmap` inside persistent UI state structs.
  * Explicitly invoke `file.sync_all().unwrap_or_default()` before explicitly `drop()`ping file handles to force an immediate OS I/O buffer flush.
  * If `std::fs::copy` fails due to phantom locks, bypass it using a manual stream copy via `std::os::windows::fs::OpenOptionsExt` configured for `share_mode(1)` (`FILE_SHARE_READ`).
* **Disk Thrashing Vulnerability:** Placing explicit file-write operations (like `settings::save_settings()`) inside an `egui` `text_edit_singleline().changed()` loop causes the application to write to disk 60 times a second during text input. File I/O saves must be explicitly gated behind distinct user actions like "Apply" buttons or returning `FileDialog` callbacks.
* **sysinfo Disk Mounting:** To reliably map logical directories to physical drives for free space estimates, explicitly call `sys.refresh_disks_list()` inside the lock to catch hot-plugged drives, and map paths by matching `user_path.starts_with(disk.mount_point())`.

### ⚙️ Process Execution
* **Windows Ghost Consoles:** Spawning FFmpeg or HLAE via `std::process::Command` on Windows flashes a blocking command prompt. Append `#[cfg(target_os = "windows")]` with `std::os::windows::process::CommandExt` and set `cmd.creation_flags(0x08000000)` to suppress it.
* **Blocking Process Interruption:** Waiting on an external process with `child.wait()` blocks the thread and prevents mid-batch cancellation. Interruptible worker threads must employ a polling loop using `child.try_wait()` and `std::thread::sleep` (e.g., ~16ms cadence), checking a shared `Arc<AtomicBool>` token on every tick. If raised, explicitly trigger `child.kill()` and reap the process before exiting.
* **Graceful Process Termination:** Ensure `tokio` tasks use `kill_on_drop(true)` and propagate an `Arc<AtomicBool>` cancel flag to terminate children immediately when the UI state transitions or the app exits.

### 🧩 Parsing & Validation
* **Background Task Panics Killing Capture Queues:** Using `.unwrap()` in the binary parser (`dod` crate) for malformed `.dem` files causes background thread panics that silently kill the entire capture queue. Always use `Result` and skip malformed frames gracefully.
* **Batch Processing Thread Crashes:** Failing a batch loop entirely because a single `.dem` file fails to parse is bad practice. Include error bypassing (`match` or `if let`) to log the error and continue to the next file.
* **Tuple Parser Type Inference Error (`E0283`):** Using `nom::sequence::tuple` with many parsers can fail to infer the error type `E`. The workaround is to use a typed let-binding (e.g., `let res: nom::IResult<&[u8], _, nom::error::Error<&[u8]>> = (...)`) to explicitly specify input/error types.
* **Borrowing vs. Ownership:** In the parser loop, `UserMessage` variants cannot be cloned repeatedly without performance cost or lifetime conflicts. One must match against a reference `&msg` to extract data before calling `process_event` with the owned `msg`.
* **Memory Safety Caps:** Implement explicit 2MB payload limits for all network/buffer reads in `patch.rs`. If a size exceeds this, log the offset and exit cleanly rather than attempting the allocation.
* **Patience Limit:** To prevent hangs on infinite or malformed loops, a `patience_limit` (e.g. 15,000 frames) exists in the deep fingerprinting parser as a fail-safe trigger for `match_started`.

### 🌐 WASM Constraints
* **WASM Target Restrictions:** The GUI compiles to `wasm32-unknown-unknown`. WASM builds lack native disk and process I/O. Any native file operations or OS thread spawning inside the GUI layer must be strictly protected by `#[cfg(not(target_arch = "wasm32"))]`.
* **WASM/Native IO Divergence:** WASM lacks disk IO, requiring `include_str!` and static/lazy initialization for localization assets, whereas native builds can scan the Steam installation directory dynamically.
* **WASM Message Race Conditions:** In WASM, spawning a worker and sending a `parse` message before the WASM module is fully compiled/initialized causes the message to be discarded by the listener. State must be managed to buffer messages until initialization completes.
