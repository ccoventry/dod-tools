<!--
IMMUTABLE MILESTONES ARCHITECTURE RULES:
1. Standard markdown headings must remain intact.
2. Completed items must be explicitly checked (changed from - [ ] to - [x]).
3. Active Sprint Priorities must reflect the current sprint's focus.
4. Historical phases must be archived using <details> tags.
-->

# Project Milestones

## 🚀 Active Sprint Priorities

### Game Capture Pipeline
- [x] Task: HLTV Active Frame Injection — Standalone `DRC_CMD_INEYE` frame injection implemented and verified in `native/src/patch/engine.rs` with dynamic `target_player_id` extraction.
- [x] Task: Graceful Degradation for Clutch Clips — Update `builder.rs` to handle edge cases where a kill occurs inside the 3.0-second EOF danger zone. The patcher must gracefully sacrifice the post-roll and schedule the `DOD_TOOLS_EXIT_TRIGGER` exactly 5 ticks before the absolute final frame to ensure the highlight is captured without crashing the batch.
- [x] Feature: Config Injection — Ensure the engine dynamically injects `+exec movie.cfg` into the HLAE command line arguments to preserve custom capture framerates and HUD settings.
- [x] Task: Long Demo Validation — Validate capture sequence for 30+ minute demos.
- [x] Task: Packet Audit — Injected a strict 2MB bounds guard into the binary scanner to prevent memory overruns and alignment drift on extended demos.
- [x] Chore: Cruft Purge — Aggressively pruned deprecated `target_player` and `min_kills` filters from configuration structs and ingestion loops.
- [x] Chore: Standardize Milestones Architecture — Design and document a strict, immutable markdown layout for this file to completely prevent AI-driven format drift across future sessions.
- [x] Task: Director Events Pipeline - Fixed premature DEMO_END assignment and properly anchored MATCH_START director events to the parsed frame times.

### Rendering & Finalization
- [x] Task: Rendering & Finalization — Native FFmpeg transcoding pipeline, raw `.mov` image sequence take ingestion, mezzanine export routing, and render queue execution via Tauri IPC.

### Dynamic Drive Failover
- [x] Task: AOT Capture Routing & JIT Render Routing — Automated capacity simulation loop, unified duration math parity, FFmpeg mezzanine threshold polling (>20 GB guard), and NTFS directory junction drive swapping below 15 GB.

### Frontend Migration
- [x] Feature: Tauri & Vite Integration — Completed frontend stack migration from native `egui` to Tauri + Vite architecture with full Master-Detail UI feature parity, IPC bridge, native folder pickers, and `@tauri-apps/plugin-fs` project session persistence.
- [ ] Task: Verify feature/migrate-tauri branch functionality for dev merge.

### UI/UX Polish
- [x] Feature: Workspace Layout Updates - Transposed paths and disk space estimators, added highlight status columns to the Master List, removed legacy folder pickers, and mounted a global "Total Export Pool Free" indicator with dynamic vector list reordering.

## 📋 General Backlog (Future Roadmap Items)

### Task B: Unify Killstreak Segmentation (demo_parser.rs)
- [x] Refactor: Life-Bounded Streaks — Extracted life-bounded streak definitions from the `analysis` crate. The Capture Studio now natively limits "killstreaks" to events bounded by `DeathMsg` or `ServerReset`, avoiding arbitrary temporal-gap logic that causes recording cross-talk.

### Task D: Architectural Decoupling & File Cleanup
- [ ] Chore: Context — Review the project to decouple logic, abstract UI components into separate modules/files, and clean up deprecated code. This is an ongoing radar item to maintain code health and readability.

### Phase 11 & 12 Enhancements (Deferred)
- [x] Feature: Batch Queue UI Re-integration — Absorbed by the Master-Detail workspace UI, which supports native batch queuing, scheduling, and multi-demo dispatch.
- [x] Task: HLTV Parser Upgrade & Active Frame Injection — Reverse-engineered GoldSrc HLTV proxy byte structure and implemented active standalone `DRC_CMD_INEYE` frame injection in `native/src/patch/engine.rs`.
- [ ] Task: Rendering & Finalization — Handling the resulting `.mov` files.

### Task E: Google Takeout Gemini Logs Parsing (Knowledge Harvest)
- [ ] Task: Implementation — Determine a programmatic or streamlined method to parse offline chat logs and feed them into the Knowledge Harvester to extract and deduplicate workflow protocols, constraints, and engine quirks from historical Web AI sessions.

## ✅ Completed Tasks

### Codebase Audits & Architecture Maintenance (July 2026)
- [x] Task: Magic Number Audit — Scanned codebase for hardcoded magic numbers, buffer sizes, and static offsets; extracted shared module-level constants.
- [x] Refactor: Stream Slicing Macro Control — Isolated native engine stream slicing mechanisms behind explicit `target_arch` macro controls.

### Capture Engine Timeline Stabilization (July 1, 2026)
- [x] Refactor: Time-Aware 0-Indexed Bounds — Resurrected `find_tick_backwards` and `find_tick_forwards` to bypass map change jumps, calculating exact bounds via float timestamps while returning engine-safe 0-indexed frame integers.
- [x] Refactor: Payload Truncation — Extracted logging strings into a `LOG_TAG` constant (`[dod]`) and aggressively abbreviated diagnostic payloads to prevent 64-byte `Cbuf_AddTextToBuffer` overflow corruption.
- [x] Task: EOF Safety Buffer & Audio Guards — Clamped all sequence termination commands within a strict 3.0-second margin from the demo's end to prevent early engine drop, and added lookahead guards to prevent fast-forwarding over overlapping audio-flush windows.

### I/O Optimization & Capture Engine Fixes
- [x] Refactor: Logging Refactor (I/O Bottlenecks) — Replaced `println!` and `eprintln!` with `log_markdown` in `capture_engine.rs` and `views/capture/mod.rs` to eliminate disk I/O bottlenecks and parsing log spam during demo scanning.
- [x] Feature: Capture Configuration Logging — Added exact configuration payload logging `[CAPTURE CONFIG PAYLOAD]` to the session log in `select.rs` before building batch queues.
- [x] Task: Engine Speed Injection — Updated timeline logic in `builder.rs` to validate `tickrate` (via `safe_tickrate` fallback) and explicitly inject the fast-forward command (`host_framerate {speed}`) at tick 0 before iterating over streaks, resolving an issue where the engine recorded the entire demo at normal speed.
- [x] Chore: Capture UI Diagnostics — Added diagnostic debug prints to button state and payload generation in `views/capture/select.rs` to isolate UI state locks.

### AI Workflow & Knowledge Harvester Automation
- [x] Chore: Knowledge Extraction — Harvester prompt expanded to explicitly target `Agent Protocols & Workflow Optimizations` to prevent recurring AI friction points.
- [x] Feature: Automated Harvester Pipeline — Modified the `Chat Closure Protocol` in `agent-protocols.md` so the AI automatically appends harvested rules to `draft_rules.md` instead of just printing them to the chat.
- [x] Chore: Rule Review Protocol — Added strict instructions to `project_context.md` forcing the AI to act as a critical sounding board (Analyze, Deduplicate, Filter, Migrate) when reviewing `draft_rules.md`.
- [x] Refactor: Rule Modularization — Extracted monolithic draft rules into distinct, targeted semantic files (`goldsrc-quirks.md` and `rust-architecture-constraints.md`).

### Memory Optimization & Patcher Robustness
- [x] Task: Directory Offset Shift Logic — Fixed directory offset corruption by mapping injected bytes to their specific physical segments, expanding `file_length` accurately, and preventing 468-byte `NetworkMessage` alignment loss (which was triggering `MAX_POSSIBLE_MSG` crashes).
- [x] Refactor: Robust Crash Logging — Refactored `log_crash_abort!` to dynamically resolve `crash_log.md` relative to the executable path and auto-create the `local/` directory.
- [x] Chore: Debug Tools — Added frame header debug logging to `write_console_cmd` and gated temporary file cleanup behind `cfg(not(debug_assertions))` for hex analysis.
- [x] Refactor: Memory Optimization — Migrated to a flat `egui_extras::TableBuilder` hierarchy per demo.
- [x] Task: Parser Desync Fix — Fixed `NetworkMessage` alignment to 468 bytes.
- [x] Refactor: Decoupled Patcher & Telemetry — Threaded the batch patcher, added f32 progress, and 2MB safety caps.
- [x] Feature: Timeline Pre-calculation — Built `K98, (+0:15) Luger` strings in background via absolute frame timestamps.
- [x] Feature: UI-Level POV Filtering — Added UI toggle for non-recording players to retain safe parser data.

### Phase 12.5: Capture Pipeline Stabilization & Rendering Upgrades (Completed)
- [x] Feature: Rendering Enhancements — Implemented HLCR codec selection, NVENC hardware acceleration, and alpha routing.
- [x] Feature: Drive Failover — Implemented dynamic multi-demo drive failover to handle storage capacity issues mid-batch.
- [x] Feature: Quality of Life & Resilience — Added path collision checks, an AV (Antivirus) file-lock retry loop, configuration persistence, and folder picker bookmarks.
- [x] Chore: Diagnostics — Expanded debug tools and enhanced crash logging specifically for the capture engine.

### Phase 12: Continuous Batch Processing (Completed)
- [x] Feature: Daisy-Chain - Continuous Batch Processing — GoldSrc ignores `quit` commands in demo streams. To solve this and reduce process launch overhead, the engine will process demos in a single continuous batch without closing the game.
- [x] Feature: Daisy-Chain - Stateful Patcher Routing — The patcher must accept `next_demo_filename` as state.
- [x] Feature: Daisy-Chain - Chaining Command — If a next demo exists, inject `mirv_recordmovie_stop; playdemo <next_demo>`.
- [x] Feature: Daisy-Chain - Completion Command — If it is the last demo, inject `mirv_recordmovie_stop; disconnect; clear; echo "[dod-tools] BATCH COMPLETE"`.
- [x] Task: Daisy-Chain - 40-Character Truncation Limit — Patched `.dem` filenames must be strictly truncated or hashed before chaining so the 1998 engine buffer doesn't clip them.
- [x] Task: Primer Demo - First-Load Black Map Bug — To fix the GoldSrc first-load black map bug, the UI/engine must duplicate the first demo in the batch and save it as `primer_ptch.dem`.
- [x] Task: Primer Demo - Asset Pre-Caching — The patcher will strip all killstreaks from `primer_ptch.dem` and inject `playdemo <first_real_demo>` immediately after the `DemoStart` frame to pre-cache map assets.
- [x] Task: Primer Demo - Capture Engine Startup — The capture engine will launch HLAE strictly using `+playdemo primer_ptch`.

### Phase 11: Capture Integration (Completed Items)
- [x] Feature: Target Directory Selector — Implemented a UI directory picker to define where raw BMP/WAV takes are saved. Routed off the OS drive.
- [x] Feature: Disk Space Pre-Flight Check — Integrated `sysinfo` check before launching engine to halt if target drive has < 15GB free space.
- [x] Chore: Temp Demo Cleanup — Gated the `std::fs::remove_file` cleanup step inside `capture_engine.rs` behind `cfg(not(debug_assertions))` to automatically delete patched demos in release builds.

### Capture Engine & Engine Quirk Fixes (June 30, 2026)
- [x] Task: HLAE Dummy Folder Sandbox Escape — Bypassed GoldSrc's engine block on `exec` commands during playback by using `mirv_movie_filename` to create a dummy directory on the OS, triggering our Rust Reaper to safely terminate the batch.
- [x] Task: Taskkill Reaper — Replaced the fragile `child.try_wait()` loop with an aggressive polling loop that explicitly runs `taskkill /F /IM hl.exe` when the dummy directory trigger is detected.
- [x] Refactor: Absolute Time-to-Frame Mapping — Completely purged `demo_fps` average math. Fixed POV timeline drift by implementing a raw binary frame-parsing loop in the scanner to extract exact float timestamps (`Arc<Vec<f32>>`), mapping absolute times to precise frame indices via binary search.
- [x] Refactor: UI Resolution & Defensive I/O — Upgraded the Capture Studio with reactive `width` and `height` resolution settings. Added defensive pathing with `std::fs::create_dir_all` session ID generation, piping I/O errors to the `CaptureState::Error` UI block.
- [x] Feature: Reactive Disk Space Math — Replaced static disk caching with a dynamic frame size multiplication formula tied to the live resolution and `capture_fps` configuration.

### Capture Engine & Engine Quirk Fixes (June 29, 2026)
- [x] Task: Command Truncation Fix — Shortened injected `echo` commands and decoupled `mirv_recordmovie_stop` to respect the strict 64-byte payload limit inside GoldSrc `ConsoleCommand` frames.
- [x] Feature: Local Demo Debug Output — Added a `save_local_patched_copy` UI toggle to the Capture Studio to easily duplicate patched `.dem` files to the workspace `demos/` directory.
- [x] Task: GoldSrc Demo `quit` Filter Bypass — Fixed an issue where the game refused to close after recording because GoldSrc actively drops any `ConsoleCommand` containing the string `quit` during demo playback. Bypassed the security check by scheduling `dodtools_exit` in the `.dem` and mapping it to `+alias dodtools_exit quit` in the engine startup arguments.
- [x] Refactor: Frame-based Tick Insertion — Moved from time-based offsets back to exact `file_tick` mapping for command insertion, ensuring reliable synchronization between highlight bounds and engine rendering frames.

## 📦 Completed Project Phases (Historical Archive)

<details>
<summary>Click to view past phases (Phases 6-10)</summary>

### Phase 6 (The Ingestion Engine)
- [x] Refactor: Multithreading & Memory Safety — Replaced single-threaded blocking UI with a background worker thread (`std::thread::Builder`) using a 16MB stack. Migrated all fixed-size array byte buffers in `patch.rs` to heap-allocated `Vec<u8>` to permanently resolve `STATUS_STACK_BUFFER_OVERRUN` crashes.
- [x] Refactor: Fault Tolerance — Eliminated all panics (`.unwrap()`) in the binary parser. The engine now safely catches malformed or corrupted `.dem` files, logs a warning, and gracefully skips them without crashing.
- [x] Feature: HLTV Safeguard — Implemented a header-inspection function (`is_hltv_demo`) to automatically detect and skip HLTV proxy network streams before they break the POV parser.
- [x] Feature: UI Sync — Added live progress tracking ("Scanning demo X of Y") and disk-based crash logging (`crash_log.txt`) without choking the `egui` render loop.
- [x] Feature: Grouping — Finished caching models for queue grouping (By Demo, By Player, Flat) to prepare highlights for batch processing.

### Phase 7 (The HLAE Game Capture Engine)
- [x] Feature: Goal — Built the engine that physically pilots Half-Life Advanced Effects (HLAE) to record the patched demos.
- [x] Feature: Architecture — Implemented a background worker thread (`capture_engine.rs`) that executes HLAE (`std::process::Command`) with AFX Hook DLLs, waits sequentially, and verifies output recording artifacts.
- [x] Refactor: Thread-Safe State & Poison Mitigation — Upgraded global discovery collections (`QUEUED_DEMOS`, caching layers) from basic `Box` wrappers to thread-safe `Arc<Mutex<Vec<T>>>` patterns with full poison recovery (`PoisonError::into_inner`).
- [x] Refactor: UI Reactivity — Decoupled rendering passes from long-lived locks (cloning state under micro-locks) and implemented a time-and-batch-gated repaint manager (>16ms/5-demo throttled `ctx.request_repaint()`).
- [x] Chore: Semantic Constraint — Formally re-asserted "HLAE Game Capture" pipeline semantics (specifically hook DLL injection command lines like `-hookDllPath` and `-programPath`), reverting a semantic drift to "Native".

### Phase 8 (Async Patcher & Export Config Restoration)
- [x] Refactor: Async Batch Patcher Migration — Replaced the in-UI synchronous patching call with a dedicated OS thread (`patch_worker`) spawned via `std::thread::Builder`. The thread posts `GuiMessage::PatchingComplete` on the mpsc channel on completion. Main loop handles state transition to `CaptureStudioState::Capture`. UI thread blocking is fully eliminated.
- [x] Feature: IS_PATCHING State Guard — Added a `static IS_PATCHING: AtomicBool` in `capture_ui.rs` with public `is_patching()` / `set_is_patching()` accessors. Gates the Proceed button and shows a live spinner + label during patching.
- [x] Feature: Full Export Configuration Restored — `PatcherConfig` now carries all six timing fields (`pre_roll_seconds`, `post_roll_seconds`, `record_start_lead`, `record_stop_trail`, `initial_delay`, `fast_forward_speed`). Initialized from `settings::get_settings()`, all editable via `DragValue` sliders in the Export Configuration panel.
- [x] Feature: Output Directory Routing — `build_batch_queue` routes patched demos into `{hl_exe_parent}/dod/` via an `output_dir: Option<PathBuf>` field on `PatcherConfig`. The directory is auto-created if absent.
- [x] Task: POV Filter Enforcement — Batch payload construction applies the `hide_non_pov` filter before building jobs, consistent with the UI toggle.

### Phase 9: UI Structural Refactor (capture_ui.rs -> views/capture/)
- [x] Chore: Dead Code Purge — Cleaned out stale statics (`WORKER_STATE`, `PROGRESS_MSG`, `PROGRESS_PCT`) and zombie imports leftover from the pre-async patcher architecture.
- [x] Refactor: Modularization — Split the monolithic 891-line `capture_ui.rs` into `views/capture/mod.rs` (dispatcher), `scan.rs`, `select.rs`, and `capture.rs`.
- [x] Refactor: State Encapsulation — Routed all shared state (`CaptureState`, `HighlightRules`, `PatcherConfig`) into `OnceLock<Mutex<T>>` managed by the parent module.

### Phase 10: Backend Structural Refactor (patch.rs -> patch/)
- [x] Refactor: Monolith Retirement — Split the massive 1,197-line `patch.rs` into a structured module (`mod.rs`, `types.rs`, `highlevel.rs`, `scanner.rs`, `builder.rs`, `engine.rs`).
- [x] Chore: WASM Target Protection — Meticulously applied `#[cfg(not(target_arch = "wasm32"))]` gates to ensure WASM builds (which lack disk/process IO) continue to compile against the types.

### Phase 10b/c: Capture Engine Stability Upgrades (June 2026)
- [x] Task: Disk Thrashing Fix — Prevented `settings.json` from being spammed to disk 60 times a second on text input.
- [x] Task: State Machine Protection — Prevented double-clicks on the Launch button and gated the "Proceed to Render" button to prevent user navigation mid-capture.
- [x] Feature: Cancellation Token — Implemented a robust `Arc<AtomicBool>` cancellation mechanism allowing users to instantly kill the HLAE background batch via a `child.kill()` polling loop in the engine.
- [x] Task: Fixes — Dash-in-filename truncation bug, viewdemo frame_count overflow/array corruption.
- [x] Refactor: Memory Optimizations — Refactored StreamPatcher to use `std::io::copy` and a unified `scratch_buf` to eliminate all heap allocations within the loop.

</details>
