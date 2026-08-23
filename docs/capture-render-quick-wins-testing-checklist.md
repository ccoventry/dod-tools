# Testing Checklist — Capture/Render Quick Wins

Manual verification for commit `976034b` on `feature/capture-render-quick-wins`
(six fixes picked from `docs/capture-render-ux-audit.md`). Run via
`npm run tauri dev` from `desktop-studio/`. Check off each item and note any
deviation from the expected result.

**Status as of 2026-08-22: none of this has been run yet.** All 21 boxes below
are unchecked — deferred to a dedicated testing pass, not skipped. Branch is
pushed to `origin/feature/capture-render-quick-wins` (`aa5c272`) but **not
merged to `dev`** until this checklist (plus the two still-open
`feature/capture-block-manifest` sections it stacks on — Merged Blocks and
Settings Persistence, see `active_sprint_state.md`) actually gets run.
Item 4's "close the app while a field still has focus" case is worth
prioritizing — it's a real open question about whether an edit can get
silently dropped, not just a sanity check.

**Update 2026-08-23:** Testing pass started (`npm run tauri dev`). Section 1
(Reveal in Explorer) done except the crash-recovery sub-item, and found +
fixed two real bugs along the way — see that section for detail and
`engineering_backlog.md` for the full writeup of both (reveal-in-explorer
silently opening the wrong folder, and render job failures never writing
anything to the activity log).

**Final update 2026-08-23: entire checklist complete.** All 7 sections done
(Section 1's crash-recovery sub-item and Section 6's zero-takes item both
came back around and got done too, not left open). Six real bugs found and
fixed along the way, none part of the original 6-fix audit this checklist
covers — full writeups in `engineering_backlog.md`:
1. Reveal-in-Explorer silently opened the wrong (default) folder on a
   missing path instead of erroring.
2. Render job failures never wrote anything to the activity log.
3. Render-recovery prompt fired on every F5 reload during an active render,
   even though nothing was interrupted.
4. Closing the app while an Init/Custom Command field still had focus
   silently dropped the edit (plus a `core:window:allow-destroy` permission
   gap found fixing it, which briefly meant the app couldn't close via X
   at all).
5. Capture Output's disk-space checks trusted a mount-point prefix match
   with no check the actual path was usable — affects real capture/render
   drive allocation (`patch/builder.rs`, `hlcr/renderer.rs`), not just this
   UI.
6. A follow-up inconsistency from fixing #5: a not-yet-created-but-valid
   path was flagged as a problem in the warning list while the footer
   correctly counted it as usable — two signals disagreeing about the same
   path.

Plus the new Capture Output per-path validation feature itself (Section 7),
built mid-session in response to bug #5/#6, with its own yellow (partial
pool) and blue (will-be-created) informational states beyond the original
red blocking case. Branch is otherwise unchanged from before this pass —
still not merged to `dev` (see `active_sprint_state.md` for the two
remaining `feature/capture-block-manifest` sections — Merged Blocks and
Settings Persistence — this checklist doesn't cover).

---

## 1. Reveal in Explorer (render job rows)

- [x] Scan a render folder with a few takes, don't start rendering yet. Each
      **Queued** row shows an "📁 Open Take Folder" button. Click it — Explorer
      opens with that take's source folder selected. **2026-08-23: "Scan for
      Takes" alone doesn't populate the job table/Actions column — that's by
      design (scan only fills the lightweight preview list; the real job rows
      appear once "Start Render Batch" queues them). Verified via
      Start Render Batch instead — button worked correctly.**
- [x] Start a render batch, let one job finish. The **Finished** row's button
      now reads "📁 Open Output" — click it — Explorer opens with the actual
      rendered file selected (not just the folder). **2026-08-23: confirmed.**
- [x] Cancel a job or let one **Error** out. Its button should still read
      "Open Take Folder" (no `output_path` was ever set for it). **2026-08-23:
      confirmed on a Cancelled row — correct label, opened the right folder,
      plus a bonus "View Log" button showed "Cancelled by user".**
- [x] Crash-recovery path: start a batch, let one job finish, force-quit the
      app before the batch fully completes, relaunch, and use the render
      recovery prompt. Confirm the recovered **Finished** job still shows
      "Open Output" pointing at a real file — this exercises the
      `output_path` persistence fix in `write_autosave`/`recover_render_batch`,
      which didn't work before this change. **2026-08-23: confirmed. Force-quit
      (killed `desktop-studio.exe` directly) correctly took the whole
      `tauri dev` wrapper down with it (exit 0, expected — it watches the
      child). On relaunch, "Render Batch Interrupted" prompt showed accurate
      Completed:1/Pending:1 counts; Recover Render Batch restored both rows
      correctly, and the recovered Finished row's "Open Output" opened
      Explorer with the real rendered .mp4 highlighted (file played back fine
      in VLC, video+audio both good). Minor Explorer-scroll-position quirk
      noted — not app-controllable, not a bug.**
- [x] Edge case: manually delete or rename a take folder on disk, then click
      its "Open Take Folder" button. Should surface an error toast, not crash
      the app. **2026-08-23: found a real bug here — see engineering_backlog.md
      ("Open Take Folder silently opened the wrong folder..."). Fixed and
      reverified: now toasts "Could not open folder: Path no longer exists:
      ..." instead of silently opening Explorer's default location.**

## 2. Dead code removal (`export_manager.rs`)

- [x] App launches normally via `npm run tauri dev` with no missing-module or
      startup errors. (Build already verified clean; this just confirms
      nothing at runtime was quietly depending on the deleted file.)
      **2026-08-23: confirmed across multiple launches this session (initial
      start, two hot-reload rebuilds, and a post-crash relaunch) — all clean,
      no missing-module/startup errors.**

## 3. Deduped export-dir fallback (`capture_engine.rs`)

- [x] Run a capture batch with **Media Output Directory** left blank (uses the
      exe-relative fallback). Confirm the batch completes and files land in
      the same place they did before this change (should be identical
      behavior — this was a pure refactor, not a behavior change).
      **2026-08-23: this field no longer exists — "Media Output Directory"
      was removed 2026-08-17 and replaced by the mandatory "Capture Output"
      list (`capture_pane.js`/`capture_manager.rs` — no separate Primary
      Media Dir field anymore, per `test_config_from_payload_primary_media_dir_is_first_drive`).
      The exe-relative fallback code technically still exists in
      `capture_engine.rs` but is unreachable through the UI now — the Start
      button is blocked with an error toast if Capture Output is empty, so
      there's no way to leave it "blank." Superseded, not testable as written.**
- [x] Run a capture batch with **Media Output Directory** explicitly set.
      Confirm normal behavior, unaffected. **2026-08-23: covered — real HLAE
      capture batches ran successfully multiple times earlier this session
      with Capture Output populated (the render-job tests in Section 1 all
      depended on a prior successful capture batch).** While testing this
      item found and fixed a real bug in the disk-space warning banner (see
      `engineering_backlog.md`, "Capture Output's 'unusable pool' warning
      gave the same generic message...") — an invalid Capture Output entry
      showed the same "not configured" message as an empty list. Also chased
      what looked like a second bug (long list of invalid paths silently
      passing validation) that turned out to be a leftover valid drive still
      in the list, not a defect — the aggregate check correctly treats "any
      one drive has room" as passing, by design.

## 4. Settings debounce (`list_editor.js` — Init/Custom Commands, numeric fields)

- [x] Type a multi-character value into a Custom Commands or Init Commands
      text field. While typing, confirm the app stays responsive (no
      per-keystroke lag/hitching). **2026-08-23: confirmed, no lag.**
- [x] After typing, click elsewhere (blur) — reopen the settings/reload the
      pane and confirm the typed value actually persisted correctly.
      **2026-08-23: confirmed — blurred, pressed F5 to reload the whole
      webview, value was still there.**
- [x] **Edge case to specifically watch for:** type a value into one of these
      fields and close the app *without* clicking away or pressing Enter
      first (i.e. while the field still has focus). Since the save now fires
      on `'change'` instead of `'input'`, an unblurred edit may not be
      persisted. Confirm whether this matters in practice — if it does, we
      should add a blur-on-close flush. **2026-08-23: confirmed it matters —
      edited a row, closed via the window's X without blurring, reopened,
      edit was gone. Fixed same session: `main.js` now hooks
      `getCurrentWindow().onCloseRequested()` to flush `persistAppSettings()`
      before the window actually closes (see `engineering_backlog.md`).
      **Re-verified: first retest hit a second bug (`window.destroy` blocked
      by missing `core:window:allow-destroy` permission — app couldn't close
      via X at all until fixed). After that fix, confirmed directly against
      `settings.json` on disk: typed a value, closed without blurring,
      the value was there. Fully resolved.**
- [x] Add / remove / reorder rows in these lists (not just edit text) — confirm
      those still save immediately as before (unaffected code path).
      **2026-08-23: confirmed on both lists.** Init Commands: added 3 rows
      (AAA/BBB/CCC on a cleared list), moved AAA down, moved CCC up, deleted
      CCC, reloaded — persisted as exactly `BBB, AAA`. Custom Commands: added
      2 rows (`first cmd`/Before/1, `test cmd`/After/5 on a cleared list),
      moved `test cmd` up, reloaded — persisted as exactly `test cmd`
      (After, 5) then `first cmd` (Before, 1), all fields intact.

## 5. NVENC concurrency warning (Render Studio)

- [x] Set Codec to **H.264 (NVENC GPU, MP4)** and Max Concurrent Renders to
      **4 or higher** — a warning toast should appear. **2026-08-23: incidental
      confirmation while testing Section 1 (those settings were already active)
      — the orange "4 concurrent NVENC renders may exceed your GPU's encoder
      session limit..." toast fired correctly.**
- [x] Lower Max Concurrent Renders to **3 or below** with NVENC still
      selected — no warning. **2026-08-23: confirmed, no toast.**
- [x] Switch codec away from NVENC (ProRes/DNxHR/software H.264) with
      concurrency still at 4+ — no warning. **2026-08-23: confirmed for all
      3 other codec options; switching back to NVENC re-triggered the
      warning correctly too.**
- [x] Load the app with NVENC + concurrency >3 already saved from a previous
      session (don't touch either field), then click **Start Render Batch**
      directly — warning should still fire (tests the click-time check, not
      just the change-event one). **2026-08-23: confirmed — reloaded with
      NVENC/4 already set, clicked Start Render Batch without touching
      either field, warning toast fired at click time alongside the
      Initializing/Queued toasts.**

## 6. Live render-scan status

- [x] Click **Scan Render Directories** on a folder with several takes.
      Confirm the status line updates incrementally (e.g. "found 1 take(s)
      so far", then 2, then 3...) rather than staying static until the whole
      scan finishes. **2026-08-23: with 7 real takes on fast local storage
      the scan completes too quickly to visually perceive the increments —
      confirmed via code instead (`render_manager.rs:51-89`):
      `scan_folder_background` emits one `render_scan_status` event per take
      as it's found, drained concurrently on its own thread, genuinely
      incremental at the implementation level, not batched at the end.**
- [x] Scan a folder with no valid takes — status should settle on "0 take(s)
      found" without erroring. **2026-08-23: confirmed, using a nonexistent
      (well-formed) folder path rather than an empty existing one — arguably
      a better test, since it also exercises the scanner's existence guard.
      Verified in code too (`native/src/hlcr/scanner.rs:100-103`):
      `scan_folder_background` explicitly checks
      `!source_folder.exists() || !source_folder.is_dir()` and skips before
      ever attempting to walk it — settled cleanly on "0 take(s) found", no
      error, even under repeated rapid scanning.**
- [x] Scan, then scan again (a second click) — the counter should reset to 0
      at the start of the second scan, not continue accumulating from the
      first. **2026-08-23: confirmed robustly — user rapid-clicked Scan for
      Takes a dozen+ times in a row, every single scan correctly reported
      "Scanned 7 render take(s)", never accumulating (14, 21, etc.).**

## 7. Capture Output validation (found + fixed mid-session, 2026-08-23 — not part of the original 6-fix audit)

Not in scope when this checklist was written; added retroactively so this
specific regression is documented for future testing passes rather than only
living in `engineering_backlog.md`. All items below already verified once
this session via manual edge-case testing in Capture Studio → Configuration
→ Capture Output.

- [x] Capture Output list empty → red banner reads "No Capture Output
      directories configured..."; Start Capture Batch disabled.
- [x] Every configured entry unusable (e.g. all relative paths like `"a"`,
      or all nonexistent) → red banner names the *specific* reason per entry
      (not absolute / malformed / doesn't exist / not a directory), not the
      generic "not configured" message; Start Capture Batch disabled.
- [x] Mixed pool — at least one real, valid, spacious drive plus one or more
      bad entries → non-blocking **yellow** banner lists the bad entries and
      states capture will proceed using the valid one(s); Start Capture
      Batch stays enabled.
- [x] Long list of bad entries (8+) → reason list caps at 3 bullets with an
      "...and N more" tail, point-form (one bullet per line via
      `white-space: pre-line`), not a run-on wall of text.
- [x] A genuinely full-but-valid drive (0 bytes free, not an invalid path)
      still gets the generic "have any free space" message, not misattributed
      to a path problem. *(Verified via code path, not a live full-drive test
      — revisit if a real 0-free-space drive is ever available to test.)*

---

## General regression pass

- [x] Full capture batch → render batch → finished movie, end to end, still
      works with no new errors in the console or toasts. **2026-08-23:
      covered cumulatively rather than as one isolated pass — multiple real
      capture batches (including a fresh capture mid-session) and multiple
      real NVENC render batches all completed cleanly with correct toasts
      throughout tonight's testing; every unexpected console error that did
      surface got found and fixed (see the 6 bugs above), not left unhandled.**
- [x] Other Settings fields (not Custom/Init Commands) still save on their
      existing triggers — nothing else was changed, but worth a spot check
      since `persistAppSettings()` fan-out is shared. **2026-08-23: HLAE/
      Half-Life/FFmpeg paths, resolution, codec, concurrency, etc. all
      persisted correctly across the many app reloads/restarts this session
      — same `persistAppSettings()` fan-out Init/Custom Commands use.**
