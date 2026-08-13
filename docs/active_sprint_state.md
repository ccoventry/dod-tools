## Web AI State
- **Overarching Goal:** Tauri + Vite Architecture Migration (Capture Studio Parity & Pipeline Stabilization).
- **Last Edited:** `desktop-studio/src/capture_pane.js`, `desktop-studio/src/detail_pane.js`, `desktop-studio/src-tauri/src/capture_manager.rs`.
- **Unresolved Errors/Bugs:** Critical payload defect in `capture_directories` (junction abort on file paths) and un-stamped `session_id`.

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
- **Current Goal:** Resolve critical pipeline bugs, achieve Capture Studio feature parity, and prepare `feature/tauri-migration` for merge to `dev`.
- **Last Evaluated:** 2026-08-13
- **Status:** Audit complete; 2 critical execution blockers identified, micro-gaps cataloged in `engineering_backlog.md`.

### Immediate Sprint Focus (Top Priorities)
1. **[CRITICAL BUG] Fix `capture_directories` Drive Mapping:** `capture_pane.js` maps `scanPaths` (individual demo files) to output pools, causing `mklink /J` junction failures and batch aborts.
2. **[CRITICAL BUG] Implement `session_id` Timestamping:** Stamp ISO timestamp (`session_YYYYMMDD_HHMMSS`) in payload so outputs route to dedicated subfolders instead of colliding in export root.
3. **[FEATURE] Global & Per-Demo Bookmark Previews:** Implement `viewdemo` + `BOOKMARK` director event patching for single-demo and global "Preview All" actions (replacing per-streak preview).
4. **[UI FIX] Detail Pane & Formatting:** Restore "Sel" checkboxes on highlight table; fix leading comma formatting bug and missing weapon name in details view.
5. **[UI POLISH] Tab Branding & Custom Controls:** Rename "Workspace & Master Queue" back to "Capture Studio"; add CSS dropdown indicators/chevrons; remove/wire dead "POV Only" input field.

## Sprint Takeaways & Architectural Rules
* **Standalone CLI Portability:** Strictly avoid dynamic disk-based localization lookups for headless binaries (`preview_cli`). Hardcoded literals prevent silent failures on target machines missing dictionary files.
* **Drag-and-Drop Tokenization:** Windows Terminal and PowerShell format drag-and-drop paths. `stdin` parsers must explicitly strip the `& ` evaluation operator and handle single (`'`) and double (`"`) quote wrapping to prevent path fragmentation.
* **Immediate-Mode UI (egui) Consolidation:** Never silo shared data types (e.g., `.dem` queues) across separate UI tabs. Use state-driven UI swaps (e.g., `CaptureMode` toggle) within a unified workspace. Always wrap vertically expanding configuration panels in `egui::ScrollArea::vertical().id_salt(...)` to prevent off-screen clipping.
* **Egui Ownership Safety:** Chaining builder methods that consume `self` (like `Response::on_hover_text`) triggers `E0382` errors. Declare the response as mutable and reassign it before evaluating `.clicked()`.
* **AI Git Hygiene:** Always audit `git show --name-only HEAD` before pushing. Autonomous IDE agents can hallucinate commit messages and silently contaminate unrelated files.