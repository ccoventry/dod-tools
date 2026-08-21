# Testing Checklist — Capture/Render Quick Wins

Manual verification for commit `976034b` on `feature/capture-render-quick-wins`
(six fixes picked from `docs/capture-render-ux-audit.md`). Run via
`npm run tauri dev` from `desktop-studio/`. Check off each item and note any
deviation from the expected result.

---

## 1. Reveal in Explorer (render job rows)

- [ ] Scan a render folder with a few takes, don't start rendering yet. Each
      **Queued** row shows an "📁 Open Take Folder" button. Click it — Explorer
      opens with that take's source folder selected.
- [ ] Start a render batch, let one job finish. The **Finished** row's button
      now reads "📁 Open Output" — click it — Explorer opens with the actual
      rendered file selected (not just the folder).
- [ ] Cancel a job or let one **Error** out. Its button should still read
      "Open Take Folder" (no `output_path` was ever set for it).
- [ ] Crash-recovery path: start a batch, let one job finish, force-quit the
      app before the batch fully completes, relaunch, and use the render
      recovery prompt. Confirm the recovered **Finished** job still shows
      "Open Output" pointing at a real file — this exercises the
      `output_path` persistence fix in `write_autosave`/`recover_render_batch`,
      which didn't work before this change.
- [ ] Edge case: manually delete or rename a take folder on disk, then click
      its "Open Take Folder" button. Should surface an error toast, not crash
      the app.

## 2. Dead code removal (`export_manager.rs`)

- [ ] App launches normally via `npm run tauri dev` with no missing-module or
      startup errors. (Build already verified clean; this just confirms
      nothing at runtime was quietly depending on the deleted file.)

## 3. Deduped export-dir fallback (`capture_engine.rs`)

- [ ] Run a capture batch with **Media Output Directory** left blank (uses the
      exe-relative fallback). Confirm the batch completes and files land in
      the same place they did before this change (should be identical
      behavior — this was a pure refactor, not a behavior change).
- [ ] Run a capture batch with **Media Output Directory** explicitly set.
      Confirm normal behavior, unaffected.

## 4. Settings debounce (`list_editor.js` — Init/Custom Commands, numeric fields)

- [ ] Type a multi-character value into a Custom Commands or Init Commands
      text field. While typing, confirm the app stays responsive (no
      per-keystroke lag/hitching).
- [ ] After typing, click elsewhere (blur) — reopen the settings/reload the
      pane and confirm the typed value actually persisted correctly.
- [ ] **Edge case to specifically watch for:** type a value into one of these
      fields and close the app *without* clicking away or pressing Enter
      first (i.e. while the field still has focus). Since the save now fires
      on `'change'` instead of `'input'`, an unblurred edit may not be
      persisted. Confirm whether this matters in practice — if it does, we
      should add a blur-on-close flush.
- [ ] Add / remove / reorder rows in these lists (not just edit text) — confirm
      those still save immediately as before (unaffected code path).

## 5. NVENC concurrency warning (Render Studio)

- [ ] Set Codec to **H.264 (NVENC GPU, MP4)** and Max Concurrent Renders to
      **4 or higher** — a warning toast should appear.
- [ ] Lower Max Concurrent Renders to **3 or below** with NVENC still
      selected — no warning.
- [ ] Switch codec away from NVENC (ProRes/DNxHR/software H.264) with
      concurrency still at 4+ — no warning.
- [ ] Load the app with NVENC + concurrency >3 already saved from a previous
      session (don't touch either field), then click **Start Render Batch**
      directly — warning should still fire (tests the click-time check, not
      just the change-event one).

## 6. Live render-scan status

- [ ] Click **Scan Render Directories** on a folder with several takes.
      Confirm the status line updates incrementally (e.g. "found 1 take(s)
      so far", then 2, then 3...) rather than staying static until the whole
      scan finishes.
- [ ] Scan a folder with no valid takes — status should settle on "0 take(s)
      found" without erroring.
- [ ] Scan, then scan again (a second click) — the counter should reset to 0
      at the start of the second scan, not continue accumulating from the
      first.

---

## General regression pass

- [ ] Full capture batch → render batch → finished movie, end to end, still
      works with no new errors in the console or toasts.
- [ ] Other Settings fields (not Custom/Init Commands) still save on their
      existing triggers — nothing else was changed, but worth a spot check
      since `persistAppSettings()` fan-out is shared.
