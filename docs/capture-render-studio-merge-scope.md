# Capture / Render Studio Merge — Scope

Tracks [issue #81](https://github.com/ccoventry/dod-tools/issues/81), which was raised
as an unscoped "R&D" marker out of the OBS-alternate-capture work
([#65](https://github.com/ccoventry/dod-tools/issues/65), `docs/obs_alternate_capture.md`).
This doc records the scope decisions made working through #81's three open questions,
so implementation has a fixed target instead of re-litigating them mid-build.

Branch: `rnd/capture-render-studio-merge`.

---

## 1. Navigation: full merge

Replace the current top-level `Capture Studio` / `Render Studio` nav split
(`nav.js`, `index.html`'s `.nav-tab-btn` list) with a single unified area —
**Capture / Render / Configuration** tabs, per the issue title.

**Precedent already in this codebase:** Capture Studio used to have "Batch
Capture Config" as its own top-level nav tab; it was folded into Capture
Studio itself as an in-workflow phase switch (`main.js`'s
`capture-detail-subtabs`, Highlights ⇄ Configuration). This merge extends
that same pattern to Render Studio rather than inventing a new one. Render
Studio currently has no equivalent subtab — its config is one flat panel —
so this is new structure for it, not a rename of something that already
exists there.

## 2. Location lists: collapse three into two

Today there are three independently-configured folder lists:

| Setting key | Purpose | Current value (this machine) |
|---|---|---|
| `target_drives` | Where Capture Studio **writes** captures (AOT bin-packed) | 5 drives: C, D, E, F, G |
| `render_folders` | Where Render Studio **scans** to find takes to render | 1 drive: C only |
| `render_export_dirs` | Where **finished renders** land (JIT routed) | 1 drive: C only |

`target_drives` and `render_folders` are meant to describe the same set of
physical locations — Render Studio should be looking wherever Capture
Studio might have written. They're just two separately-maintained lists
today, and nothing keeps them in sync. **This is a live bug, not just
duplication**: on this machine `target_drives` has 5 drives but
`render_folders` only has 1, meaning any capture written to D/E/F/G is
currently invisible to Render Studio's scan.

**Decision:** collapse `target_drives` + `render_folders` into one shared
**"Capture locations"** list, used both as Capture Studio's AOT write
targets and as Render Studio's scan input. `render_export_dirs` stays a
separate **"Render output"** list — it's a genuinely different destination
(finished renders, not raw captures), not a duplicate of anything.

## 3. Drive-selection algorithms: keep both, don't unify

Capture uses **AOT** (ahead-of-time) bin-packing —
`allocate_blocks_first_fit_decreasing` (`native/src/patch/builder.rs:399-441`),
which pre-plans every clip's drive assignment before the batch starts, because
the byte cost is exactly knowable in advance: raw, uncompressed BMP frames,
`resolution_width × resolution_height × frame_count` (`builder.rs:795-801`).

Render uses **JIT** (just-in-time) per-job routing (`native/src/hlcr/renderer.rs:126-136`),
because it *can't* pre-plan: output is a quality-targeted encode
(`libx264 -crf 16`, `renderer.rs:252`, similarly variable for ProRes/DNxHR/NVENC),
so the byte cost isn't known until the encode finishes.

**Decision:** this is not a legacy inconsistency to fix — each algorithm is
correct for its own data shape, and forcing one onto the other would make
routing worse, not better. Both stay exactly as they are. (Considered and
rejected: using raw-source byte size as a safe upper bound to give Render an
AOT-style pre-plan too — the bound holds, since real codecs always compress
uncompressed BMP by a wide margin, but it's loose enough — often 10–50x — that
reserving against it across a whole batch would cause premature drive
switching and false "not enough room" rejections for space the render will
never actually need.)

Minor cleanup, not a design change: Capture's headroom threshold
(`MIN_DRIVE_HEADROOM_BYTES` = 15 GiB, `native/src/sys/disk.rs:10`) and
Render's (`EXPORT_THRESHOLD` = 20 GiB, a local literal in `renderer.rs:129`)
aren't shared today. Worth naming Render's as its own documented constant
(it's already intentionally different — bigger buffer for large ProRes
output — so it shouldn't just be silently unified to 15 GiB), rather than
leaving it an unexplained inline literal.

## 4. New: fix the JIT concurrent-drive race

Found while scoping this: `get_available_bytes()` (`renderer.rs:132`) is a
live, unlocked OS query with no shared reservation ledger between
concurrently-running jobs. `spawn_scheduler` (`render_manager.rs:360-387`)
can start multiple jobs in the same scheduling tick — up to
`max_concurrent_renders` (default up to 8). If several jobs' drive checks
land within the same instant, before any of them have written bytes, they
can all see the same free-space number, all pass the 20 GiB threshold, and
all pick the same drive — the threshold was only ever sized to be safe for
one job at a time, not however many race onto a drive together.

**Decision:** fix this as part of the merge, not urgently but folded into
this scope rather than filed separately. Fix shape: a small shared
in-memory reservation ledger (e.g. `Arc<Mutex<HashMap<PathBuf, u64>>>`) —
each job provisionally reserves a byte estimate against the shared ledger
when it claims a drive, and releases the reservation when the job finishes.
Being loose barely matters here, since the reservation is only held for the
few minutes that job is actually encoding, unlike a full-batch AOT plan
which would hold it for the whole queue's lifetime.

That per-job estimate should be **dynamic, not the flat 20 GiB threshold** —
a 5-second 720p clip and a 2-minute clip shouldn't reserve the same amount.
Same math Capture's AOT allocator already uses (`builder.rs:795-801`),
applied per render clip instead of per capture block:

```
raw_bytes ≈ width × height × 3 (24-bit RGB) × frame_count
```

`ClipData.frame_count` (`scanner.rs:19`) is already scanned per-take —
nothing new needed there. Resolution isn't currently tracked anywhere on the
render side (`RenderConfig` has no resolution field, `config.rs:22-32`) —
FFmpeg just reads each BMP's own dimensions at encode time — so this needs
one small addition: read one BMP's header (54 bytes, not the whole image)
at scan time and store width/height on `ClipData` alongside `frame_count`.
This estimate then replaces the flat `EXPORT_THRESHOLD` pre-check too: gate
on "does this drive have at least *this clip's* estimate + a safety margin"
rather than a one-size-fits-all 20 GiB, which today can both reject a drive
that has plenty of room for a short clip and under-reserve for a long one.

Worth being precise about what this still is: it borrows AOT's *estimation
math*, not AOT's *upfront whole-batch planning*. It's decided per-job, at
the moment that job starts (not all at once before the batch begins), the
estimate is a deliberate overestimate of the real (compressed) output
rather than an exact figure, the ledger churns continuously as jobs start
and finish rather than being computed once, and it needs real thread-safety
(concurrent claim/release) that AOT's single synchronous pass never had to
deal with. Still fundamentally JIT — just an accurate, race-safe JIT
instead of a flat threshold with no cross-job awareness at all.

## 5. Quick-Clip / Workspace mode

No new design needed. Render Studio already reads `takeIndex` and
`currentScannedDemos` through the `getTakeIndex`/`getAllDemos` callbacks
wired in `initRenderUI` (`main.js`), which are already mode-influenced by
Capture Studio (`studioMode`, `main.js:363-398`) today. The merge just needs
the mode toggle to stay visible/accessible in whatever header the unified
three tabs share — it isn't gaining new semantics.

---

## Out of scope / deferred

- OBS's render-skip-by-default question — #81 explicitly frames this as
  "revisit once Capture Output and the export pool are one configured
  thing." Still deferred; not part of this pass.
- Full AOT-style pre-planning for render batches — considered in §3 and
  rejected; the per-job reservation fix in §4 is the right level of fix.

## Not yet decided

- Exact layout of the unified Configuration tab beyond the two location
  lists (codec/fps/concurrency presumably stay Render-specific fields, not
  things that need to move)
- Implementation sequencing for the actual code changes
