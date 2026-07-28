# Tauri & Vite Migration Backlog

This document outlines the audited completion status of the Tauri and Vite frontend migration for `dod-tools`, followed by a prioritized backlog of remaining tasks.

---

## 1. Completed Migration Tasks

- **Rust Backend Infrastructure (`src-tauri`)**
    - Initialized Tauri v2 application workspace in `desktop-studio/src-tauri`.
    - Integrated native engine dependencies (`native`, `dod`, `analysis`, `tokio`, `serde`) into `Cargo.toml`.
    - Implemented managed state system using `CaptureManager` and `RenderManager` with cross-thread `Arc<Mutex<T>>` wrappers.
    - Offloaded blocking I/O and demo parsing operations to `tokio::task::spawn_blocking`.
    - Exported core Tauri IPC commands:
        - `scan_directory` / `scan_demos`: Scans directory trees for `.dem` files and extracts highlight streaks via `native::patch::scan_demo_for_highlights`.
        - `start_capture_batch`: Converts IPC payloads (`CapturePayload`) to `PatcherConfig` and dispatches `native::capture_engine::spawn_capture_engine`.
        - `cancel_capture_batch` & `capture_status`: Manages active capture batch execution state and atomic cancellation flags.
        - `calculate_export_pool_space`: Queries OS disk capacity across target output drives using `native::sys::disk::get_available_bytes`.
        - `scan_render_directories`: Scans take directories in parallel via `native::hlcr::scanner::scan_folder_background`.
        - `execute_render_batch`, `render_status`, `cancel_render_batch`: Manages asynchronous FFmpeg transcoding pipelines and job tracking.
        - `test_bridge`: Scaffolding bridge command for IPC verification.

- **Vite & Frontend Architecture (`desktop-studio`)**
    - Configured Vite build tooling and package scripts in `desktop-studio/package.json`.
    - Modularized monolithic JavaScript logic into ES components:
        - `ipc_bridge.js`: Encapsulates Tauri `@tauri-apps/api/core` `invoke` calls and `@tauri-apps/api/event` listeners with `.catch()` error handling.
        - `master_pane.js`: Handles rendering and selection state for the Master Demo Queue table.
        - `detail_pane.js`: Renders streak breakdown cards and POV filtering.
        - `capture_pane.js`: Manages configuration inputs, payload serialization, and capture batch execution triggers.
        - `render_pane.js`: Controls render take discovery, FFmpeg batch invocation, and progress state polling.
    - Integrated native OS dialog plugins (`@tauri-apps/plugin-dialog`) for directory browsing (`open`) and project saving (`save`).
    - Built split-pane layout with custom CSS (`styles.css`) adapting dark-theme styling parameters.

---

## 2. Audit Findings: Identified Missing Requirements & Gaps

- **Backend (Tauri Commands)**
    - Missing executable path verification command (`validate_executable_paths`) for `hlae.exe` and `hl.exe`.
    - `analysis` crate pipeline (`run_analyzer`) is unmapped; match statistics, player mortality, kill matrices, and chat logs cannot be requested by the frontend.
    - `execute_render_batch` in `render_manager.rs` does not emit Tauri progress events (`app_handle.emit("render_status", ...)`), forcing frontend polling.
    - No backend schema validation or sidecar (`.dodtools_preview`) watermarking for loaded/saved project sessions.

- **Frontend (Vite UI)**
    - `capture_pane.js` hardcodes fallback executable paths (`C:\dummy\hlae.exe` and `C:\dummy\hl.exe`) due to missing input fields and pickers in `index.html`.
    - Lacks bulk streak selection controls ("Select All", "Deselect All", minimum kill count filter).
    - Missing UI pane for deep demo telemetry and analysis (scoreboards, chat logs, round chronologies).
    - Lacks interactive timeline / frame-level scrubber component.
    - IPC event listener `initRenderProgressListener` is disconnected from backend emission channels.

---

## 3. Prioritized Task Backlog

### Phase 1: Critical Pipeline & Path Wiring (High Priority)
- [x] Restore Executable Path Inputs & Pickers
    - Add HTML inputs and `Browse` buttons for `hlae.exe` and `hl.exe` paths to `export-config-panel` in `index.html`.
    - Update `main.js` to persist executable paths in application state and project JSON configuration.
    - Update `capture_pane.js` to extract dynamic executable paths from DOM elements instead of hardcoded dummy strings (`C:\dummy\...`).
- [x] Implement Backend Path Validation Command
    - Create a `validate_paths` command in `src-tauri/src/lib.rs` to verify executable existence and read permissions before starting batch capture.
    - Bind `validate_paths` in `ipc_bridge.js` and enforce UI validation flags on `start-capture-btn`.
- [x] Implement Real-Time IPC Event Emission for Render Batch
    - Update `execute_render_batch` in `render_manager.rs` to take `tauri::AppHandle` and emit `render_status` events on progress updates.
    - Wire `initRenderProgressListener` in `render_pane.js` to update `#render-progress-bar` dynamically without polling overhead.

### Phase 2: Analysis & Telemetry Integration (Medium Priority)
- [x] Bind Analysis Crate to Tauri IPC
    - Implement `analyze_demo` command in `src-tauri/src/lib.rs` wrapping `native::run_analyzer_with_progress`.
    - Create `SerializedAnalysis` DTO in Rust for serializing scoreboard, chat log, mortality, and round data across IPC.
    - Add `analyzeDemo` wrapper function in `ipc_bridge.js`.
- [x] Build Telemetry & Stats UI Components
    - Create `telemetry_pane.js` to render match scoreboards, player kill matrices, and chat logs.
    - Add a `Telemetry` tab or modal view in `index.html` linked to selected demo item in `master_pane.js`.

### Phase 3: UX Polish & Selection Enhancements (Low Priority)
- [x] Bulk Selection & Filtering Controls
    - Add "Select All", "Deselect All", and "Min Kills" filter controls to `detail_pane.js`.
    - Implement search/filter input in `master_pane.js` for demo filename and map filtering.
- [x] Interactive Timeline & Frame Scrubber
    - Build a visual timeline canvas in `detail_pane.js` displaying kill timestamps and pre-roll/post-roll window spans.
- [x] Native Toast & Error Notification System
    - Replace raw `<p>` status text elements with floating UI toast notifications for error alerts and completion status.
