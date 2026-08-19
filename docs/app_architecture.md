# System Architecture Decisions

## Tech Stack Core
- **Language/Runtime:** Rust targeting native desktop and `wasm32-unknown-unknown` GUI compilation.
- **UI Framework:** `egui` (Zero-allocation immediate-mode rendering loop).
- **Frontend Stack (Migrated):** Tauri backend with Vite/JS frontend (`feature/tauri-migration`).

## Workspace Module Boundaries
- `native/`: Handles direct GoldSrc hooks, binary stream patching, and memory injection layers.
- `src/ui/`: Contains the immediate-mode GUI modules. Standard native threading and direct `std::fs` operations are strictly forbidden inside this module.
- `shared/`: Holds immutable type definitions, message schemas, and atomic communication channels passing events between native loops and the UI thread.

## State, Memory, & Concurrency Rules
- **State Ingestion:** Immediate-mode execution. Telemetry data feeds sequentially from native processes into active UI state tracking blocks via bounded sync channels.
- **Micro-Stutter Protection:** Highly accessed internal asset catalogs are wrapped in `std::sync::RwLock`. Under no circumstances should blocking mutex synchronization primitives be introduced directly on the UI frame loop thread.
- **Network Header Alignment:** Network messages expect exactly 468 bytes of header data before reading payload lengths. Ensure byte streams slice exactly on this structural boundary before passing to structural deserializers.
- **Memory Safety Guardrails:** Never use fixed-size stack buffers (`[u8; N]`) for binary stream slicing inside the engine memory layer. Always use heap-allocated `Vec<u8>` gated by explicit 2MB payload limits to avoid stack overruns during memory injection.
- **Pure Float Timestamps:** Purge all average `demo_fps` math estimations when synchronizing highlights. Timeline alignment must rely strictly on extracting absolute binary float timestamps (`Arc<Vec<f32>>`) during the initial engine scan and mapping them via binary search to prevent drift over long matches.
- **Eager Layout Gating:** Always toggle active UI state flags (e.g., `capture_engine_running = true`) to true *prior* to launching asynchronous background threads. Wrap the triggering widgets in strict `ui.add_enabled(!state)` blocks to prevent the immediate-mode loop from executing double-launch thread allocations.
- **Array Bounds Clamping Danger:** When executing time-offset walkbacks (e.g., `find_tick_backwards`), out-of-bounds guards must explicitly return the requested `start_frame` unmodified. Using `.min()` on an array truncates valid late-game frame indices to local minimums.
- **Serialization Fallback State:** Struct fields wrapped in `Arc<Vec<f32>>` marked with `#[serde(skip)]` will deserialize into empty arrays, not `None`. Fallback validation logic must strictly check `array.is_empty()`.
- **Multi-Demo Array Routing:** When passing global state (like `frame_times` arrays) into a batch processor, strictly use a `HashMap<String, Arc<Vec<T>>>` keyed by demo filename to prevent out-of-bounds slice mapping on secondary files.

## Frontend & UI Integrity
- **Tauri IPC Silent Failures:** Asynchronous Tauri `invoke` commands silently swallow Rust backend errors if `.catch()` is not implemented. Always enforce `.catch((err) => ...)` at all IPC boundaries.
- **Dialog API Enforcement:** Manual text inputs for OS file paths introduce severe string escaping vulnerabilities. Exclusively mandate native folder pickers (e.g., `@tauri-apps/plugin-dialog`) for filesystem routing.
- **UI vs. Data Initialization Desync:** Applying data filters exclusively at the UI rendering layer causes active state blocks to process visually hidden data. Enforce data filtering constraints directly on struct defaults during backend ingestion.
- **State Management:** Prioritize read-only status labels over duplicate inputs for shared values to maintain a single source of truth and prevent configuration drift.
- **Immediate-Mode GUI State Sync:** Following data ingestion events (e.g., `ProjectLoaded`), clear stale application state explicitly (`queued.clear()`) and invoke `ctx.request_repaint()` to prevent UI desynchronization.
- **Timeline Hydration:** User-selected JSON index parameters (e.g., `start_kill: 0`) must not be parsed as physical ticks. Map them to float times and execute a linear search against the global `frame_times` array. Calculate `DEMO_END` using `total_demo_frames`, not localized streak lengths.
- **Tauri Plugin Architecture:** A Vite frontend capability (e.g., `fs:default`) will silently fail if the corresponding backend Rust crate (e.g., `tauri-plugin-fs`) is missing from `Cargo.toml` or not initialized in the builder chain.
- **IPC Hardware Queries:** Cross the IPC boundary to execute OS-level `sysinfo` queries to gauge capacity. Do not rely on local frontend arrays (`targetDrives.length`) for hardware estimations.

## Filesystem & IO Routing
- **NTFS `read_dir` Non-Determinism:** `std::fs::read_dir` order is non-deterministic on Windows. Sort shared state collections (`QUEUED_DEMOS`) precisely at the background ingestion layer using `binary_search_by` and `insert`. Do not use `.push()` followed by `.sort_by()` to avoid O(N log N) overhead choking the thread.
- **Filesystem Semantics:** Enforce `remove_dir_all` strictly for HLAE/Engine signal folders (`DOD_TOOLS_EXIT_TRIGGER`). Reserve `remove_file` for persistent configuration artifacts.
- **Waterfall Resolution Pattern:** External tools (e.g., FFmpeg) must utilize a prioritized resolution chain: User Override -> Bundled Local -> System Path.
- **Data Portability:** Path resolution (Saved Path -> Project Root -> Last Used Directory -> Game Directory) must be dynamically computed in memory. Never destructively overwrite relative saved JSON paths with locally resolved absolute paths.
- **Watermarking Pattern:** Output generated assets tracking using 1:1 sidecar files (`.dodtools_preview`) to maintain atomic cleanup and file-system renaming resilience, avoiding master list locks.

## Architectural Non-Goals
- No support for native engine multi-threading layers operating inside immediate-mode UI thread blocks.
- No direct architectural support for alternative Source/GoldSrc modifications outside of Day of Defeat 1.3.

## Recent Architectural Changes
- **Dynamic Per-Block Drive Routing:** The AOT simulation (`build_batch_queue`) now evaluates capacity and allocates target drives on a per-block basis rather than per-demo.
- **Two-Tier Block Cutting (2026-08-18):** `build_batch_queue` used to ask one question — "do the pre/post-roll windows collide?" — and answer it by merging the two highlights into a single take, which produced one clip full of dead air whenever only the *speed-change* bookkeeping overlapped. Now split in two, both via `patch::builder::blocks_merge` with different padding: highlights merge into one take only when their **recordings** overlap (start-lead/stop-trail) or sit closer than `MIN_TAKE_SEPARATION_SECONDS` (a conservative guard against a `mirv_recordmovie` stop/start cycle too tight to flush a take); otherwise they stay separate takes, and if the fast-forward round trip between them doesn't fit, it's simply dropped — playback stays at normal speed across the gap (`chained_to_previous`, which also suppresses the `stopsound` flush, since that only exists to repair fast-forward audio drift). Both tiers key off `first_kill_frame`/`last_kill_frame` so the decision uses the same frames the record marks do, and a Kill Range edit moves both together.
- **First-Fit-Decreasing Bin-Packing (2026-08-18):** The `Chronological` allocation strategy (Next Fit, no drive backtracking — could fail a batch outright even with enough total free space across drives) was removed entirely; `DriveAllocationStrategy` no longer exists. `build_batch_queue` now always allocates each demo's clip blocks largest-byte-estimate-first (First-Fit-Decreasing) across the configured capture drives, so a big clip doesn't get stranded because an earlier, smaller clip already claimed the only drive with room for it.
- **Junction-Based Pathing:** Bypassed GoldSrc string limit and escape constraints by generating temporary OS-level directory junctions (`_route_N`) in the game directory.
- **Alias Injection:** The VDM generation now maps `mirv_movie_filename` commands to the generated junctions via aliases in `dodtools_helper.cfg`.
- **UI State Fixes:** 
  - Surfaced background thread capacity errors to the UI by storing them in the `egui` temporary context.
  - State-gated the error banner to strictly render only when `current_state == CaptureStudioState::Select`.
  - Changed default scanner initialization so `CaptureStreak` is unchecked (`is_selected: false`) to support an opt-in workflow.
- **GC Architecture:** Garbage collection policies (configuration) are decoupled from mechanisms (execution), ensuring the `Drop` trait remains lightweight.