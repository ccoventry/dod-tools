## Web AI State
- **Overarching Goal:** Tauri + Vite Architecture Migration (Capture Studio Parity & Pipeline Stabilization).
- **Last Edited:** `desktop-studio/src/capture_pane.js`, `desktop-studio/src/detail_pane.js`, `desktop-studio/src/main.js`, `desktop-studio/src/ipc_bridge.js`, `desktop-studio/index.html`, `desktop-studio/src-tauri/src/capture_manager.rs`, `desktop-studio/src-tauri/src/lib.rs`, `desktop-studio/src-tauri/src/settings_manager.rs`, `desktop-studio/src-tauri/Cargo.toml`.
- **Unresolved Errors/Bugs:** None blocking. All High-Priority Capture Studio parity gaps in `engineering_backlog.md` are now resolved — `feature/tauri-migration` is ready for end-to-end verification, testing, and merge to `dev`.

## Active Epics
- **Headless Preview CLI:** COMPLETED
  - Secondary binary target `preview_cli` built and tested. Supports interactive prompt fallback (`is_interactive = true`), drag-and-drop file/folder inputs, and automatic localized `previews/` folder generation.
- **Top Navigation & Functional Cancellation:** COMPLETED
  - Migrated vertical navigation to top bar, extracted Export Manager view, implemented non-destructive `INGESTION_CANCEL` thread interruption, and unified localized view footers.
- **Localization Infrastructure:** COMPLETED
  - Migrated hardcoded GUI/CLI/scanner strings to localizations. Updated `analysis::localization` to support transparent dual-key lookups (`#key` and `key`) for Valve KeyValues and AMXX files.
- **HLTV Active Frame Injection:** COMPLETED
  - Standalone `DRC_CMD_INEYE` frame injection implemented in `native/src/patch/engine.rs`.
- **Dynamic Drive Failover:** COMPLETED
  - AOT capture routing, duration math parity, JIT render routing, and UI/UX export pool indicators with dynamic vector list reordering.
- **Frontend Migration & Capture Studio Parity:** IN PROGRESS (Branch: `feature/tauri-migration`)
  - Transitioning frontend stack to Tauri + Vite architecture in `desktop-studio/`, restoring parity with legacy `dev` branch.

## IDE AI State
- **Current Goal:** All High-Priority Capture Studio parity gaps in `engineering_backlog.md` are resolved — `PatcherConfig` Full Persistence, Pre-Flight Disk Allocation & Pre-Scan Estimator, and Running Process Guard & Detector Modal all closed 2026-08-13, on top of Custom Engine Commands Integration from earlier in the sprint. `feature/tauri-migration` is ready for end-to-end verification, testing, and merge to `dev`; only Medium/Low priority polish items remain in the backlog.
- **Last Evaluated:** 2026-08-13
- **Status:** Immediate Sprint Focus cleared (all items resolved, see below). All High Priority parity items in `engineering_backlog.md` are closed. Next step is branch verification/merge, not further feature work.

### Immediate Sprint Focus (Top Priorities)
*(Cleared 2026-08-13 — every item previously tracked here, and every High Priority item in `engineering_backlog.md`'s parity backlog, is resolved. Full detail lives in `engineering_backlog.md`'s Completed Tasks section. Remaining work is Medium/Low priority polish plus end-to-end branch verification before merge.)*

## Sprint Takeaways & Architectural Rules
* **Standalone CLI Portability:** Strictly avoid dynamic disk-based localization lookups for headless binaries (`preview_cli`). Hardcoded literals prevent silent failures on target machines missing dictionary files.
* **Drag-and-Drop Tokenization:** Windows Terminal and PowerShell format drag-and-drop paths. `stdin` parsers must explicitly strip the `& ` evaluation operator and handle single (`'`) and double (`"`) quote wrapping to prevent path fragmentation.
* **Immediate-Mode UI (egui) Consolidation:** Never silo shared data types (e.g., `.dem` queues) across separate UI tabs. Use state-driven UI swaps (e.g., `CaptureMode` toggle) within a unified workspace. Always wrap vertically expanding configuration panels in `egui::ScrollArea::vertical().id_salt(...)` to prevent off-screen clipping.
* **Egui Ownership Safety:** Chaining builder methods that consume `self` (like `Response::on_hover_text`) triggers `E0382` errors. Declare the response as mutable and reassign it before evaluating `.clicked()`.
* **AI Git Hygiene:** Always audit `git show --name-only HEAD` before pushing. Autonomous IDE agents can hallucinate commit messages and silently contaminate unrelated files.