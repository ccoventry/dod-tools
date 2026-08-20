# Phase 4 Manual Test Checklist

Connected-workspace Phase 4 (Workspace vs Quick-Clip modes). Open this file
in VS Code's Markdown Preview (Ctrl+Shift+V) — the checkboxes below are
clickable there and toggle straight back into this file.

See `engineering_backlog.md`'s Phase 4 entry for implementation detail on
anything below; `~/.claude/plans/splendid-shimmying-kahn.md` for the original
plan.

## Mode toggle

- [ ] Fresh install / first launch starts on Quick-Clip
- [ ] Toggling Quick-Clip ↔ Workspace updates the caption text and slider position
- [ ] Restart the app — mode persisted from last session
- [ ] Toggling mode does **not** deselect the top nav tabs or blank the window

## Save / Load

- [ ] Quick-Clip: Save button reads "Save as Workspace…"
- [ ] Clicking it in Quick-Clip saves a file **and** flips the toggle to Workspace afterward
- [ ] Workspace: Save button reads "Save Session" and just re-saves, no mode change
- [ ] Loading a project file always lands in Workspace mode, even starting from Quick-Clip
- [ ] Save button's width stays fixed — toggling modes doesn't shift the nav tabs

## Re-scan behavior

- [ ] Workspace: re-scan a folder with a demo that already has status/notes/kill-range set — all three survive
- [ ] Quick-Clip: same re-scan — status/notes/kill-range reset (wholesale replace, intentional)

## Tracked badge

- [ ] Setting status to Captured or Rendered shows the bookmark badge
- [ ] Setting status to **Pending** does **not** show the badge (narrowed 2026-08-19)
- [ ] Adding a note shows the badge — appears after leaving the field (blur/Enter), not per keystroke
- [ ] Editing a Kill Range shows the badge — appears after the change commits
- [ ] Clicking the Kill Range ↺ reset button removes the badge if that was the only tracked reason
- [ ] Removing a note / resetting status to None removes the badge if nothing else is tracked
- [ ] Badge tooltip lists the correct reason(s) (Captured/Rendered status, a note, an edited kill range)
- [ ] Badge appears **after** the demo name, not before — every row's name starts at the same x position

## Clear Untracked

- [ ] Disabled when the queue is empty
- [ ] Removes only untracked demos, toast reports kept/removed counts correctly — same in both modes
- [ ] Clicking with nothing untracked shows the "nothing to clear" toast, doesn't error — same in both modes
- [ ] With a search active, only removes untracked demos among the visible ones — demos hidden by the search are left alone even if untracked
- [ ] Toast includes the "search filter active — only considered N of M" callout when the search actually narrowed what was cleared
- [ ] No callout appears when the search box is empty (nothing was narrowed)
- [ ] Typing a search that matches zero demos shows "No demos match the current search" instead of clearing anything
- [ ] Regression: 2 untracked demos total, search narrows to 1 visible untracked — after Clear Untracked, toast says just "Removed 1 untracked demo(s)" with NO "kept ... with tracked work" claim (the hidden one is untracked too, not kept for being tracked — fixed 2026-08-19)
- [ ] After clicking, a "[queue] Clear Untracked: ..." line appears in crash_log.md (%APPDATA%/dod-tools/logs/crash_log.md) with the right count and demo names

## Clear Selected

- [ ] Disabled until at least one row is checked
- [ ] Enables the instant a row is checked, disables again at zero checked
- [ ] Header select-all/deselect-all only affects currently visible (search-filtered) rows
- [ ] Checking only untracked rows → plain confirm(), no modal
- [ ] Checking at least one tracked row → full modal opens, tracked count in summary is accurate
- [ ] Modal title reads "Clear Selected Demos" and the Confirm button reads "Clear Selected Anyway" (not "Clear All Anyway" — regression, fixed 2026-08-19)
- [ ] Modal Cancel leaves the queue untouched
- [ ] Modal Confirm removes exactly the checked rows
- [ ] Modal "Save Session First" actually saves before removing; toast says "Saved, then removed…"
- [ ] With NO session currently loaded/saved (so "Save Session First" actually triggers a native Save-As dialog): cancelling that dialog leaves the Clear modal open, nothing lost
- [ ] With a session already loaded/saved, "Save Session First" writes straight to that existing file — no Save-As dialog appears at all
- [ ] Check a row, then type a search that hides it, then click Clear Selected — the hidden checked row is left in the queue, untouched
- [ ] Clear the search box afterward — that hidden row is STILL checked, not silently deselected (fixed 2026-08-19, was unchecked along with everything else before)
- [ ] That scenario's confirm/toast mentions the hidden checked demo(s) that were left alone
- [ ] If every checked row is hidden by the current search, clicking shows a toast saying nothing visible to remove — doesn't error
- [ ] After clicking, a "[queue] Clear Selected: ..." line appears in crash_log.md with the right count and demo names

## Clear All

- [ ] Disabled when the queue is empty
- [ ] Nothing tracked anywhere → plain confirm(), no modal — same in both modes
- [ ] Something tracked → full modal, accurate tracked count — same in both modes (Save First still converts Quick-Clip to Workspace)
- [ ] Modal title reads "Clear All Demos" and the Confirm button reads "Clear All Anyway"
- [ ] With a search active, only removes demos matching the search — demos outside the filter survive regardless of tracked status
- [ ] Confirm/modal text and the success toast both include the "search filter active — only considered N of M" callout when scoped
- [ ] No callout appears when the search box is empty
- [ ] Typing a search that matches zero demos shows "No demos match the current search" instead of clearing anything
- [ ] After clicking, a "[queue] Clear All: ..." line appears in crash_log.md with the right count and demo names
- [ ] Regression: load a session, then Clear All the entire queue down to 0 demos (both via Confirm Anyway and via plain confirm() with nothing tracked) — no devtools RangeError/stack overflow, toast appears normally (was a real crash, fixed 2026-08-19, see bugs.md)
- [ ] Regression: same as above but via a plain "+ Add Demo Files"/"+ Add Folder" scan instead of Load Session — no devtools TypeError ("Cannot read properties of null"), toast appears normally, View Telemetry button disables cleanly (was a real crash, fixed 2026-08-19, see bugs.md)

## Row delete (🗑 in Actions column)

- [ ] Untracked row deletes instantly, no prompt
- [ ] Tracked row opens the same shared modal (not a lesser confirm)
- [ ] Modal title reads "Remove Tracked Demo" and the Confirm button reads "Remove Anyway"
- [ ] Modal Cancel on a row delete leaves that row in place
- [ ] Modal Confirm removes just that row; selection/detail view update correctly
- [ ] Modal Save Session First saves, then removes just that row
- [ ] Deleting the currently-selected demo clears/updates the detail view correctly
- [ ] Deleting a demo above the selected one shifts the selection index down correctly
- [ ] After deleting (tracked or untracked), a "[queue] Row delete: ..." line appears in crash_log.md, noting whether it had tracked work

## Layout

- [ ] Checkbox column header lines up with row checkboxes at a maximized/ultrawide window size
- [ ] Highlights/Pending/Captured/Rendered/Actions headers stay centered over their columns at different widths
- [ ] Demo File column still truncates long filenames with an ellipsis and hover tooltip

## Merged blocks (needs a real capture batch)

- [ ] Use `find_overlaps` (or tight roll settings) to find/force two highlights that merge into one take
- [ ] After capture, both source highlight rows show the "merged → chain_NN_bN" badge in Highlight Details
- [ ] Badge tooltip reads sensibly (names the shared take)

## Settings persistence sanity check

- [ ] `studio_mode` round-trips correctly in `settings.json` after a restart
- [ ] Project file's `mode` field is written on save (informational only — Load always forces Workspace regardless)
