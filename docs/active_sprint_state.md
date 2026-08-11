## Web AI State
- **Overarching Goal:** Architectural UI decoupling, Top Navigation migration, and scan cancellation completed. Preview CLI target operational with interactive and headless execution. Next phase: Tauri & Vite frontend migration (`feature/tauri-migration`).
- **Last Edited:** `native/src/bin/cli/main.rs`, `analysis/src/localization.rs`, `docs/engineering_backlog.md`.
- **Unresolved Errors/Bugs:** None.

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
- **Current Goal:** Frontend migration audit & Tauri/Vite integration (`feature/tauri-migration`).
- **Last Evaluated:** Updated `docs/engineering_backlog.md` and `docs/active_sprint_state.md` with complete backlog statuses.
- **Status:** All workspace tests and localization test suites passing. `preview_cli` binary target fully functioning in both headless and interactive modes.
- **Next Task:** Audit scaffolding in `desktop-studio/` and `src-tauri/` on branch `feature/tauri-migration`.
