## Web AI State
- **Overarching Goal:** Tauri Frontend Migration - Capture Studio Parity. All High-Priority parity gaps (Custom Commands, Config Persistence, Disk Estimator, Process Guard) are now successfully resolved.
- **Last Files Edited:** `desktop-studio/src-tauri/src/capture_manager.rs`, `desktop-studio/src-tauri/src/lib.rs`, `desktop-studio/src-tauri/src/settings_manager.rs`, `desktop-studio/src-tauri/Cargo.toml`, `Cargo.lock`, `desktop-studio/src/capture_pane.js`, `desktop-studio/src/detail_pane.js`, `desktop-studio/src/main.js`, `desktop-studio/src/ipc_bridge.js`, `desktop-studio/index.html` (the three High-Priority parity features, now split into three commits — `b3c47e1`/`f36196c`/`d9b9f10` — with the `sysinfo` `Process::name()` type fix folded into the process-guard commit rather than left as a separate broken-then-fixed commit); `docs/engineering_backlog.md`, `docs/active_sprint_state.md`, `docs/milestones.md` (documentation audit & sync); `docs/staging_lessons.md` (session lessons learned, in progress).
- **Unresolved Errors:** None. `cargo check -p desktop-studio` is clean (verified before the commit split; content is byte-identical after, so the result still holds).
- **Next Steps:** Branch `feature/tauri-migration` is stable and ready for end-to-end functional testing and merge to `dev`, or advancing to Medium Priority parity gaps (e.g., Clear Previews modal, Standalone Game Launch).

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
- **Current Goal:** All High-Priority Capture Studio parity gaps in `engineering_backlog.md` are resolved — `PatcherConfig` Full Persistence, Pre-Flight Disk Allocation & Pre-Scan Estimator, and Running Process Guard & Detector Modal all closed 2026-08-13, on top of Custom Engine Commands Integration from earlier in the sprint. The three feature diffs (previously one uncommitted working-tree blob) have since been split into three scoped commits — `b3c47e1` (settings persistence), `f36196c` (disk estimator), `d9b9f10` (process guard) — on `feature/tauri-migration`. `feature/tauri-migration` is ready for end-to-end verification, testing, and merge to `dev`; only Medium/Low priority polish items remain in the backlog.
- **Last Evaluated:** 2026-08-13
- **Status:** Working tree is clean against `HEAD` except for `docs/staging_lessons.md` (three new engine-quirk/workflow bullets appended this pass, not yet committed) and this file being rewritten in the same pass. No open compile errors or IDE diagnostics; the last `cargo check -p desktop-studio` (pre-split) was clean. Immediate Sprint Focus is cleared (all items resolved, see below); all High Priority parity items in `engineering_backlog.md` are closed. Next step is branch verification/merge, not further feature work.

### Immediate Sprint Focus (Top Priorities)
*(Cleared 2026-08-13 — every item previously tracked here, and every High Priority item in `engineering_backlog.md`'s parity backlog, is resolved. Full detail lives in `engineering_backlog.md`'s Completed Tasks section. Remaining work is Medium/Low priority polish plus end-to-end branch verification before merge.)*

## Sprint Takeaways & Architectural Rules
* **Standalone CLI Portability:** Strictly avoid dynamic disk-based localization lookups for headless binaries (`preview_cli`). Hardcoded literals prevent silent failures on target machines missing dictionary files.
* **Drag-and-Drop Tokenization:** Windows Terminal and PowerShell format drag-and-drop paths. `stdin` parsers must explicitly strip the `& ` evaluation operator and handle single (`'`) and double (`"`) quote wrapping to prevent path fragmentation.
* **Immediate-Mode UI (egui) Consolidation:** Never silo shared data types (e.g., `.dem` queues) across separate UI tabs. Use state-driven UI swaps (e.g., `CaptureMode` toggle) within a unified workspace. Always wrap vertically expanding configuration panels in `egui::ScrollArea::vertical().id_salt(...)` to prevent off-screen clipping.
* **Egui Ownership Safety:** Chaining builder methods that consume `self` (like `Response::on_hover_text`) triggers `E0382` errors. Declare the response as mutable and reassign it before evaluating `.clicked()`.
* **AI Git Hygiene:** Always audit `git show --name-only HEAD` before pushing. Autonomous IDE agents can hallucinate commit messages and silently contaminate unrelated files.