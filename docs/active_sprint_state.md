# Active Sprint State

> **Two independent work streams exist in this repo.** Everything below is the
> Capture/Render Studio (Tauri migration + capture-block-manifest) track. There is a
> second, unrelated stream â demo-derived match stats for the KTP league, living on
> `dev`/`main` only â tracked in `docs/demo_stats_feasibility.md`, not here. Check which
> one you're resuming before reading further.

## Current State (2026-08-24)

`dev`'s tip is `911604f`. Everything through PR #7 is merged. Merge chain, oldest to newest: `feature/tauri-migration` (2026-08-18) â `fix/dem-patch-delta-description-crash` (PR #1) â `chore/capture-allocation-cleanup` (PR #2) â `feature/capture-block-manifest` (PRs #3, #5) â `feature/capture-render-quick-wins` (PR #4) â `feature/ui-string-consolidation` (PR #6) â `fix/capture-disk-space-gates` (PR #7, unified the three disagreeing capture disk-space gates).

**In progress:**
- `chore/docs-cleanup-issues-migration` â docs/CLAUDE.md/CI reconciled, open items migrated to GitHub Issues. Not yet merged into `dev`.
- `feature/decal-flush-r-and-d` — decal hygiene between capture clips ([#60](https://github.com/ccoventry/dod-tools/issues/60)). **Code complete and measuring clean; the only thing left is watching a capture in game.** The ring sweep was validated in game (2026-08-24) and `m_Size` measured at ~4 units (a radius, so decals overlap only within ~8 units and a position may sit ~3 units off a surface). **Wiring** (`c9989cf`): `CaptureBlock` carries `record_start_tick`/`record_stop_tick`, `PatcherConfig` carries `decal_flush`/`decal_ring_limit`/`capture_fov`, `init_commands` pins `r_decals` last, and the clean runs as a pre-pass inside `StreamPatcher::patch` so its four call sites cannot drift; the injected-cvar frame stays off because it would shift every later frame ordinal by +1. **Tiling** (`f25853f`): positions tiled across planes fitted to real decals. **Camera-filter fixes** (`4aab914`): clearance became a ranking rather than a hard gate, and the pass no longer bails out when a demo has no settled spawn. **Map coordinate store** (`b886809`): proven world coordinates pooled per map build, keyed on name AND checksum, world decals only, exact values not grid-rounded; read-only seed dirs are supported so a shipped store could be distributed by an updater. **FOV fix** (`aa87cac`): the on-screen cone is derived from the capture FOV and frame shape rather than a fixed 40 degrees — at `mirv_fov 105` the frame corner is ~56 degrees off axis, so the old value was silently treating in-shot positions as hidden. **BSP geometry** (`a4bed14`, `1c86571`, `a9c9eab`, `da99dd7`): `patch::bsp` reads map surfaces, the node tree and visibility, and placement now asks whether a spot is genuinely hidden — PVS first, then the frame cone, then a line-of-sight trace — instead of asking only whether it falls outside the frame.

  **Measured 2026-08-26** across 85 demos spanning all 18 maps in the user's library, at FOV 105: **85/85 reach a full 68-position sweep, zero in-clip frames show a flush position, all decided from geometry.** Before the occlusion work three of those demos were getting a single position. The BSP parser is validated against 107,584 coordinates the engine provably accepted (98.93% land on a world face within 1 unit). End-to-end through the real capture path: frame ordinals preserved, one `r_decals` pin, no scratch left behind, ~6s of flush per demo.

  **Remaining: watch a pipeline capture in game.** Nobody has, and per the issue that is the only test that catches this feature's failure modes. Design for what comes after is in `docs/decal_flush_bsp_surfaces.md` — BSP-derived coordinates (stage 3) and an adaptive maximum sweep that would retire the `r_decals` pin entirely (stage 6, now reachable: every demo has ≥3,916 camera-safe candidates against the 1,028 it needs). Diagnostics: `native/src/bin/validate_bsp`, `survey_decal_flush`, `verify_decal_pipeline`. Note for testing: `monday-wsod25_r07_m1*` and `_m3*` demos are corrupt (`_m2` is fine).
Test demos and screenshots now live in `local/demos/` and `local/screenshots/`, both gitignored via `/local/`. Repo-relative paths were repointed to match in PR #61 (`chore/demos-into-local`, merged into `dev` 2026-08-25).

**Next work:** pick from the open GitHub Issues, or whatever the user brings to a fresh session.

**Local cleanup outstanding:** a stray local-only branch `audit/capture-render-ux` â its one unique doc was cherry-picked over to `fix/capture-disk-space-gates` before that merged, the branch itself was never deleted (`git branch -d audit/capture-render-ux`), harmless but unused.

Full narrative history through 2026-08-23 (the detailed day-by-day bug hunts, phase completions, etc.) has been moved to `docs/sprint_history_archive.md` â this file was getting too large to serve its actual purpose (a quick landing point for a fresh session). Check `engineering_backlog.md` for anything still open.

## Sprint Takeaways & Architectural Rules
* **Standalone CLI Portability:** Strictly avoid dynamic disk-based localization lookups for headless binaries (`preview_cli`). Hardcoded literals prevent silent failures on target machines missing dictionary files.
* **Drag-and-Drop Tokenization:** Windows Terminal and PowerShell format drag-and-drop paths. `stdin` parsers must explicitly strip the `& ` evaluation operator and handle single (`'`) and double (`"`) quote wrapping to prevent path fragmentation.
* **Immediate-Mode UI (egui) Consolidation:** Never silo shared data types (e.g., `.dem` queues) across separate UI tabs. Use state-driven UI swaps (e.g., `CaptureMode` toggle) within a unified workspace. Always wrap vertically expanding configuration panels in `egui::ScrollArea::vertical().id_salt(...)` to prevent off-screen clipping.
* **Egui Ownership Safety:** Chaining builder methods that consume `self` (like `Response::on_hover_text`) triggers `E0382` errors. Declare the response as mutable and reassign it before evaluating `.clicked()`.
* **AI Git Hygiene:** Always audit `git show --name-only HEAD` before pushing. Autonomous IDE agents can hallucinate commit messages and silently contaminate unrelated files.
