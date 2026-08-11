## Web AI State
- **Overarching Goal:** Tauri + Vite Architecture Migration. Successfully bridged async IPC boundaries and mapped UX constraints across 7 phases (Push events, Scan Cancellation, Settings Persistence, Demo Auditor, AOT Simulation, Keyboard Nav, Window Config).
- **Last Edited:** `desktop-studio/src/ipc_bridge.js`, `desktop-studio/src/telemetry_pane.js`, `desktop-studio/src-tauri/tauri.conf.json`.
- **Unresolved Errors/Bugs:** None blocking compilation. Awaiting manual user validation of all async IPC pathways and native execution.

## Active Epics
- **Headless Preview CLI:** COMPLETED
  - Secondary binary target `preview_cli` built and tested. Supports interactive prompt fallback (`is_interactive = true`), drag-and-drop file/folder inputs, and automatic localized `previews/` folder generation.
- **Top Navigation & Functional Cancellation:** COMPLETED
  - Migrated vertical navigation to top bar, extracted Export Manager view, implemented non-destructive `INGESTION_CANCEL` thread interruption, and unified localized view footers.
- **Localization Infrastructure:** COMPLETED
  - Migrated hardcoded GUI/CLI/scanner strings to localizations. Updated `analysis::localization` to support transparent dual-key lookups (`#key` and `key`) for Valve KeyValues and AMXX files.
- **HLTV Active Frame Injection:** COMPLETED
  - Standalone `DRC_CMD_INEYE` frame injection implemented in `native/src/patch/engine.rs`.
- **Frontend Migration:** IN PROGRESS (Branch: `feature/tauri-migration`)
  - Transitioning frontend stack to Tauri + Vite architecture in the `desktop-studio/` workspace (`src-tauri/`).
- **Dynamic Drive Failover:** COMPLETED
  - AOT capture routing, duration math parity, JIT render routing, and UI/UX export pool indicators with dynamic vector list reordering.

## IDE AI State
- **Current Goal:** Conclude Tauri/Vite frontend migration integration and document architectural lessons.
- **Last Evaluated:** Updated `docs/staging_lessons.md`, `docs/active_sprint_state.md`, and `docs/engineering_backlog.md`.
- **Status:** All requested IPC boundary mapping and frontend modifications are complete. Syntax/EOF errors in Vite bridge have been corrected.
- **Next Task:** Manual user testing of Vite and Tauri dev servers.

## Sprint Takeaways & Architectural Rules
* **Standalone CLI Portability:** Strictly avoid dynamic disk-based localization lookups for headless binaries (`preview_cli`). Hardcoded literals prevent silent failures on target machines missing dictionary files.
* **Drag-and-Drop Tokenization:** Windows Terminal and PowerShell format drag-and-drop paths. `stdin` parsers must explicitly strip the `& ` evaluation operator and handle single (`'`) and double (`"`) quote wrapping to prevent path fragmentation.
* **Immediate-Mode UI (egui) Consolidation:** Never silo shared data types (e.g., `.dem` queues) across separate UI tabs. Use state-driven UI swaps (e.g., `CaptureMode` toggle) within a unified workspace. Always wrap vertically expanding configuration panels in `egui::ScrollArea::vertical().id_salt(...)` to prevent off-screen clipping.
* **Egui Ownership Safety:** Chaining builder methods that consume `self` (like `Response::on_hover_text`) triggers `E0382` errors. Declare the response as mutable and reassign it before evaluating `.clicked()`.
* **AI Git Hygiene:** Always audit `git show --name-only HEAD` before pushing. Autonomous IDE agents can hallucinate commit messages and silently contaminate unrelated files.
