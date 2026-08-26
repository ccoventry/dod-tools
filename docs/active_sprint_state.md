# Active Sprint State

> **Two independent work streams exist in this repo.** Everything below is the
> Capture/Render Studio (Tauri migration + capture-block-manifest) track. There is a
> second, unrelated stream — demo-derived match stats for the KTP league, living on
> `dev`/`main` only — tracked in `docs/demo_stats_feasibility.md`, not here. Check which
> one you're resuming before reading further.

## Current State (2026-08-24)

`dev`'s tip is `911604f`. Everything through PR #7 is merged. Merge chain, oldest to newest: `feature/tauri-migration` (2026-08-18) → `fix/dem-patch-delta-description-crash` (PR #1) → `chore/capture-allocation-cleanup` (PR #2) → `feature/capture-block-manifest` (PRs #3, #5) → `feature/capture-render-quick-wins` (PR #4) → `feature/ui-string-consolidation` (PR #6) → `fix/capture-disk-space-gates` (PR #7, unified the three disagreeing capture disk-space gates).

**In progress:**
- `chore/docs-cleanup-issues-migration` — docs/CLAUDE.md/CI reconciled, open items migrated to GitHub Issues. Not yet merged into `dev`.
- `feature/decal-flush-r-and-d` — decal hygiene between capture clips ([#60](https://github.com/ccoventry/dod-tools/issues/60)). **R&D complete; production wiring is what remains.** The ring sweep was validated in game (2026-08-24), and the engine constant it depended on was measured (2026-08-25): `m_Size` for the small bullet hole is ~4 units, so a position may sit 3 units off a surface outward or 4 inward. Movement *along* a fitted plane is unconstrained, so flush positions get tiled across a plane rather than harvested one at a time — which closes the old 30-of-68 position shortfall. Remaining: tile positions in `decal_strip.rs`, choose `ring_limit` from a real capture, thread `record_start_tick`/`record_stop_tick` into `CaptureBlock`. Full findings on the issue, including why the `r_decals` cvar approach cannot work and why structural verification cannot catch this feature's failure mode.
Test demos and screenshots now live in `local/demos/` and `local/screenshots/`, both gitignored via `/local/`. Repo-relative paths were repointed to match in PR #61 (`chore/demos-into-local`, merged into `dev` 2026-08-25).

**Next work:** pick from the open GitHub Issues, or whatever the user brings to a fresh session.

**Local cleanup outstanding:** a stray local-only branch `audit/capture-render-ux` — its one unique doc was cherry-picked over to `fix/capture-disk-space-gates` before that merged, the branch itself was never deleted (`git branch -d audit/capture-render-ux`), harmless but unused.

Full narrative history through 2026-08-23 (the detailed day-by-day bug hunts, phase completions, etc.) has been moved to `docs/sprint_history_archive.md` — this file was getting too large to serve its actual purpose (a quick landing point for a fresh session). Check `engineering_backlog.md` for anything still open.

## Sprint Takeaways & Architectural Rules
* **Standalone CLI Portability:** Strictly avoid dynamic disk-based localization lookups for headless binaries (`preview_cli`). Hardcoded literals prevent silent failures on target machines missing dictionary files.
* **Drag-and-Drop Tokenization:** Windows Terminal and PowerShell format drag-and-drop paths. `stdin` parsers must explicitly strip the `& ` evaluation operator and handle single (`'`) and double (`"`) quote wrapping to prevent path fragmentation.
* **Immediate-Mode UI (egui) Consolidation:** Never silo shared data types (e.g., `.dem` queues) across separate UI tabs. Use state-driven UI swaps (e.g., `CaptureMode` toggle) within a unified workspace. Always wrap vertically expanding configuration panels in `egui::ScrollArea::vertical().id_salt(...)` to prevent off-screen clipping.
* **Egui Ownership Safety:** Chaining builder methods that consume `self` (like `Response::on_hover_text`) triggers `E0382` errors. Declare the response as mutable and reassign it before evaluating `.clicked()`.
* **AI Git Hygiene:** Always audit `git show --name-only HEAD` before pushing. Autonomous IDE agents can hallucinate commit messages and silently contaminate unrelated files.
