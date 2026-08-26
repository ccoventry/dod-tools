# Phase 4 Manual Test Checklist

Connected-workspace Phase 4 (Workspace vs Quick-Clip modes). Open this file
in VS Code's Markdown Preview (Ctrl+Shift+V) — the checkboxes below are
clickable there and toggle straight back into this file.

See `engineering_backlog.md`'s Phase 4 entry for implementation detail on
anything below; `~/.claude/plans/splendid-shimmying-kahn.md` for the original
plan.

> **Doc-oversight fix (2026-08-24):** the boxes below had never been ticked
> despite `sprint_history_archive.md` confirming Phase 4 was "implemented AND
> extensively manually verified... (2026-08-19/20)" via exactly this
> checklist, with the dated "fixed 2026-08-19/20" annotations throughout this
> file being the direct evidence that testing against each behavior actually
> happened. Retroactively checked off everything with that evidence, same
> treatment already applied to the Merged Blocks/Settings Persistence
> sections below on 2026-08-23. Left unchecked: badge tooltip wording and the
> project-file `mode` field write — `sprint_history_archive.md` explicitly
> names both as genuinely unverified polish, not gates.

## Mode toggle

- [x] Fresh install / first launch starts on Quick-Clip
- [x] Toggling Quick-Clip ↔ Workspace updates the caption text and slider position
- [x] Restart the app — mode persisted from last session
- [x] Toggling mode does **not** deselect the top nav tabs or blank the window (mode-toggle/nav-router collision — fixed 2026-08-19)

## Save / Load

- [x] Quick-Clip: Save button reads "Save as Workspace…"
- [x] Clicking it in Quick-Clip saves a file **and** flips the toggle to Workspace afterward
- [x] Workspace: Save button reads "Save Session" and just re-saves, no mode change
- [x] Loading a project file always lands in Workspace mode, even starting from Quick-Clip
- [x] Save button's width stays fixed — toggling modes doesn't shift the nav tabs (fixed 2026-08-19)

## Re-scan behavior

- [x] Workspace: re-scan a folder with a demo that already has status/notes/kill-range set — all three survive
- [x] Quick-Clip: same re-scan — status/notes/kill-range reset (wholesale replace, intentional)

## Tracked badge

- [x] Setting status to Captured or Rendered shows the bookmark badge
- [x] Setting status to **Pending** does **not** show the badge (narrowed 2026-08-19)
- [x] Adding a note shows the badge — appears after leaving the field (blur/Enter), not per keystroke (fixed 2026-08-19)
- [x] Editing a Kill Range shows the badge — appears after the change commits (fixed 2026-08-19)
- [x] Clicking the Kill Range ↺ reset button removes the badge if that was the only tracked reason
- [x] Removing a note / resetting status to None removes the badge if nothing else is tracked
- [ ] Badge tooltip lists the correct reason(s) (Captured/Rendered status, a note, an edited kill range)
- [x] Badge appears **after** the demo name, not before — every row's name starts at the same x position

## Clear Untracked

- [x] Disabled when the queue is empty
- [x] Removes only untracked demos, toast reports kept/removed counts correctly — same in both modes
- [x] Clicking with nothing untracked shows the "nothing to clear" toast, doesn't error — same in both modes
- [x] With a search active, only removes untracked demos among the visible ones — demos hidden by the search are left alone even if untracked
- [x] Toast includes the "search filter active — only considered N of M" callout when the search actually narrowed what was cleared
- [x] No callout appears when the search box is empty (nothing was narrowed)
- [x] Typing a search that matches zero demos shows "No demos match the current search" instead of clearing anything
- [x] Regression: 2 untracked demos total, search narrows to 1 visible untracked — after Clear Untracked, toast says just "Removed 1 untracked demo(s)" with NO "kept ... with tracked work" claim (the hidden one is untracked too, not kept for being tracked — fixed 2026-08-19)
- [x] After clicking, a "[queue] Clear Untracked: ..." line appears in crash_log.md (%APPDATA%/dod-tools/logs/crash_log.md) with the right count and demo names
- [x] Regression: select a tracked demo (so it survives the clear), click Clear Untracked — that same demo stays selected/highlighted afterward instead of jumping to row 1 (fixed 2026-08-20, see engineering_backlog.md)
- [x] With a search active, select a tracked demo that's visible, click Clear Untracked — same demo stays selected afterward, same as without a search

## Clear Selected

- [x] Disabled until at least one row is checked
- [x] Enables the instant a row is checked, disables again at zero checked
- [x] Header select-all/deselect-all only affects currently visible (search-filtered) rows
- [x] Checking only untracked rows → plain confirm(), no modal
- [x] Checking at least one tracked row → full modal opens, tracked count in summary is accurate
- [x] Modal title reads "Clear Selected Demos" and the Confirm button reads "Clear Selected Anyway" (not "Clear All Anyway" — regression, fixed 2026-08-19)
- [x] Modal Cancel leaves the queue untouched
- [x] Modal Confirm removes exactly the checked rows
- [x] Modal "Save Session First" actually saves before removing; toast says "Saved, then removed…"
- [x] With NO session currently loaded/saved (so "Save Session First" actually triggers a native Save-As dialog): cancelling that dialog leaves the Clear modal open, nothing lost
- [x] With a session already loaded/saved, "Save Session First" writes straight to that existing file — no Save-As dialog appears at all
- [x] Check a row, then type a search that hides it, then click Clear Selected — the hidden checked row is left in the queue, untouched
- [x] Clear the search box afterward — that hidden row is STILL checked, not silently deselected (fixed 2026-08-19, was unchecked along with everything else before)
- [x] That scenario's confirm/toast mentions the hidden checked demo(s) that were left alone
- [x] If every checked row is hidden by the current search, clicking shows a toast saying nothing visible to remove — doesn't error
- [x] After clicking, a "[queue] Clear Selected: ..." line appears in crash_log.md with the right count and demo names
- [x] Regression: select demo A, check a DIFFERENT demo B, click Clear Selected — demo A stays selected/highlighted afterward instead of jumping to row 1 (fixed 2026-08-20, see engineering_backlog.md)
- [x] With a search narrowing 5 demos to 4 visible: select the 4th visible one, check a different visible one, click Clear Selected — the selected demo stays selected afterward, same as without a search

## Clear All

- [x] Disabled when the queue is empty
- [x] Nothing tracked anywhere → plain confirm(), no modal — same in both modes
- [x] Something tracked → full modal, accurate tracked count — same in both modes (Save First still converts Quick-Clip to Workspace)
- [x] Modal title reads "Clear All Demos" and the Confirm button reads "Clear All Anyway"
- [x] With a search active, only removes demos matching the search — demos outside the filter survive regardless of tracked status
- [x] Confirm/modal text and the success toast both include the "search filter active — only considered N of M" callout when scoped
- [x] No callout appears when the search box is empty
- [x] Typing a search that matches zero demos shows "No demos match the current search" instead of clearing anything
- [x] After clicking, a "[queue] Clear All: ..." line appears in crash_log.md with the right count and demo names
- [x] Regression: load a session, then Clear All the entire queue down to 0 demos (both via Confirm Anyway and via plain confirm() with nothing tracked) — no devtools RangeError/stack overflow, toast appears normally (was a real crash, fixed 2026-08-19, see bugs.md)
- [x] Regression: same as above but via a plain "+ Add Demo Files"/"+ Add Folder" scan instead of Load Session — no devtools TypeError ("Cannot read properties of null"), toast appears normally, View Telemetry button disables cleanly (was a real crash, fixed 2026-08-19, see bugs.md)
- [x] Regression: select a demo, then type a search that hides it but leaves other demos visible, then click Clear All — the hidden, still-selected demo survives (it was outside the filter) and Highlight Details still shows it afterward, not reset to row 1 or blanked — no visible row highlight is expected since it's filtered out of view (fixed 2026-08-20, see engineering_backlog.md)

## Row delete (🗑 in Actions column)

- [x] Untracked row deletes instantly, no prompt
- [x] Tracked row opens the same shared modal (not a lesser confirm)
- [x] Modal title reads "Remove Tracked Demo" and the Confirm button reads "Remove Anyway"
- [x] Modal Cancel on a row delete leaves that row in place
- [x] Modal Confirm removes just that row; selection/detail view update correctly
- [x] Modal Save Session First saves, then removes just that row
- [x] Deleting the currently-selected demo clears/updates the detail view correctly
- [x] Deleting a demo above the selected one shifts the selection index down correctly
- [x] Regression: select a demo, then delete a DIFFERENT row's 🗑 (not the selected one) — the blue row highlight stays on the originally selected demo instead of disappearing, matching what Highlight Details shows (fixed 2026-08-20, previously the highlight always vanished regardless of which row's 🗑 was clicked — see engineering_backlog.md)
- [x] After deleting (tracked or untracked), a "[queue] Row delete: ..." line appears in crash_log.md, noting whether it had tracked work

## Layout

- [x] Checkbox column header lines up with row checkboxes at a maximized/ultrawide window size (fixed 2026-08-19)
- [x] Highlights/Pending/Captured/Rendered/Actions headers stay centered over their columns at different widths (fixed 2026-08-19)
- [x] Demo File column still truncates long filenames with an ellipsis and hover tooltip

## Merged blocks (needs a real capture batch)

- [x] Use `find_overlaps` (or tight roll settings) to find/force two highlights that merge into one take. **2026-08-22/23: happened incidentally during `MIN_TAKE_SEPARATION_SECONDS` calibration testing — a real capture using at-the-time-uncalibrated values triggered a genuine merge.**
- [x] After capture, both source highlight rows show the "merged → chain_NN_bN" badge in Highlight Details. **2026-08-22/23: confirmed — both highlights flipped to Captured sharing one `merged → chain_01_b0` badge.**
- [ ] Badge tooltip reads sensibly (names the shared take). **Not specifically verified — the badge itself was confirmed, its tooltip text wasn't separately checked.**

## Settings persistence sanity check

- [x] `studio_mode` round-trips correctly in `settings.json` after a restart. **2026-08-23: confirmed directly — Quick-Clip, F5, still Quick-Clip; switched to Workspace, F5, still Workspace. Re-confirmed via a genuine full close+reopen (not just F5): loaded briefly showing the `"quick-clip"` default before the async `get_settings()` fetch resolved and flipped it to the saved Workspace value — expected startup sequencing, not a bug.**
- [ ] Project file's `mode` field is written on save (informational only — Load always forces Workspace regardless). **Not specifically verified.**
