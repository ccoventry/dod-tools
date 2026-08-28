# Direct-to-Video Capture (`mirv_movie_ffmpeg`)

> **Status 2026-08-27 — scoped, not started.** Tracks [#42](https://github.com/ccoventry/dod-tools/issues/42).
> Read `docs/goldsrc_dod_quirks.md` and `docs/hlae_protocols.md` first — the engine and HLAE facts
> this rests on are recorded there and must not be re-derived.
>
> The sibling direction, OBS as an alternate capture method, is
> [#65](https://github.com/ccoventry/dod-tools/issues/65), and should follow this rather than
> precede it: both have to answer how a non-BMP capture artefact flows through take verification,
> Render Studio's admission predicate and export routing, and this one reaches those questions with
> a smaller change and with HLAE still in charge of timing.

---

## What HLAE actually offers

`mirv_movie_ffmpeg` makes HLAE pipe rendered frames **into an FFmpeg process it spawns itself**.
That answers #42's original open question — it is neither a named pipe we build nor a watcher that
tails the BMP sequence as it lands. Two subcommands:

    mirv_movie_ffmpeg <stream-or-group> enabled 0|1
    mirv_movie_ffmpeg <stream-or-group> options <sOptions>

Groups are `all`, `allColor`, `allMain`, `allWorld`, `allEntity`, `allDepth`. Individual streams are
`main`, `mainRight`, `world`, `worldRight`, `entity`, `entityRight`, `depthMain`, `depthMainRight`,
`depthWorld`, `depthWorldRight`, `hudColor`, `hudAlpha`, `debug` — so the separate-HUD path this
pipeline already drives (`mirv_movie_separate_hud`) maps onto `hudColor`/`hudAlpha` directly.

The options string is the tail of an FFmpeg command line, with two tokens:

| token | meaning |
|---|---|
| `{AFX_STREAM_PATH}` | where that stream's output goes |
| `{QUOTE}` | a literal `"` |

The wiki's own example:

    mirv_movie_ffmpeg all enabled 1
    mirv_movie_ffmpeg all options "-c:v rawvideo {QUOTE}{AFX_STREAM_PATH}\video.avi{QUOTE}"

There is also an `optionsEx` subcommand for finer control; `mirv_movie_ffmpeg main optionsEx` with
no value prints the current defaults as a worked example.

**Version support.** Added to AfxHookGoldSrc alongside the AfxHookSource feature. Issue `#842`
(GoldSrc FFmpeg videos flipped) regressed it from 2.18.2 and is fixed. The working install here is
**2.25.3** (2026-07-10), well clear of that.

---

## What we know without running it, and what needs the game

Two facts are already settled by evidence rather than assumption:

- **Audio is not part of this.** `mirv_movie_export_sound` is a separate mechanism with its own
  cvar, and the FFmpeg path carries video only. The WAV also cannot be a second FFmpeg input,
  because it does not exist yet when the process starts.
- **Sound export is on by default.** The pipeline never sets `mirv_movie_export_sound`, and every
  take in the library contains a `sound.wav`. That is the default answering for itself.

Everything below needs one real capture to settle, and none of it can be reasoned out of the bytes:

1. **What `{AFX_STREAM_PATH}` resolves to.** The take folder, or a per-stream subfolder inside it?
   This decides the output layout, whether the video lands as a sibling of `sound.wav`, and how much
   of Render Studio's scanner has to change.
2. **Whether it follows `mirv_movie_filename`.** This is the load-bearing one. The whole batch
   pipeline routes output by aliasing `mirv_movie_filename` per block into `_route_N` junctions, to
   stay under GoldSrc's path limits. If `{AFX_STREAM_PATH}` derives from `mirv_movie_filename`, that
   routing keeps working untouched. If it is absolute or configured separately, the long-path
   problem the junctions exist to solve comes straight back — and this time inside an options string
   that also has to hold an FFmpeg command line.
3. **Whether takes are still auto-numbered.** HLAE writes into `take0000`, `take0001`, … under
   whatever `mirv_movie_filename` names. Does FFmpeg mode keep that, or write a flat file?
   `hlcr::scanner`'s "skip a trailing `take*` component" logic and `shared::paths::take_key` both
   assume it.
4. **Whether `sound.wav` still lands beside it**, and in which of the two folders above.

---

## The constraint that decides where these commands go

**The options string must be set once, at load, from a config file — never as an injected
`ConsoleCommand` frame.** This is the same rule `r_decals` follows, for two independent reasons:

- GoldSrc's `Cbuf_AddTextToBuffer` limit is 64 bytes per injected command, and an FFmpeg command
  line plus a path is several times that. There is no staggering trick that helps: it is one
  argument to one command.
- An injected `ConsoleCommand` frame shifts every later frame ordinal by +1 and desyncs the
  scheduled capture commands. See the decal-ring entry in `docs/goldsrc_dod_quirks.md` for how that
  failure looks — a capture that completes normally and is wrong.

So this belongs in `dodtools_helper.cfg`, alongside the existing aliases, executed via the
`+exec dodtools_helper.cfg` already on the HLAE command line. Path escaping in it follows the
existing rule: forward slashes become `\\\\`.

---

## What the prize actually is

**Not** skipping Render Studio — the separate `sound.wav` means a mux pass is still required. The
prize is deleting the BMP sequence.

A 15-clip session at 1280x720 currently writes tens of thousands of bitmaps. That is the dominant
disk cost of a capture batch, and it is the reason `build_batch_queue` does first-fit-decreasing
bin-packing across a pool of drives at all. Replacing it with one video file per stream changes:

- **Disk**, by roughly the compression ratio chosen (see below).
- **The render pass**, from decoding a BMP sequence to a stream copy plus audio — seconds instead of
  minutes per take.
- **Capture wall-clock**, probably. Writing thousands of small files is not free, though how much of
  the capture time that actually accounts for is unmeasured and should not be assumed.

**The codec choice is the whole story on size.** The wiki example uses `-c:v rawvideo`, which is
essentially as large as the BMPs it replaces — fine as a first correctness test, useless as the
shipped default. A lossless-but-fast intermediate (`utvideo`, `ffv1`, or `libx264 -qp 0`) is the
realistic target, and picking it is a quality decision that wants the user's judgement, not a
default chosen here. It is also the same decision that makes #65 attractive to people who are happy
to trade quality for speed.

---

## Where this touches the codebase

- **`native/src/patch/builder.rs`** — `final_init_commands` and the `dodtools_helper.cfg` writer.
  The `enabled` and `options` lines go here. `mirv_movie_separate_hud` already branches nearby and
  decides whether `hudColor`/`hudAlpha` need their own options.
- **`native/src/hlcr/scanner.rs`** — `is_renderable_take` currently requires a `.wav` **and** a
  subfolder containing `00000.bmp`. A video take has the wav and a video file, so the predicate
  needs a second admissible shape. Its own doc comment records why that matters: it is shared with
  capture-side verification precisely so "the capture succeeded" and "Render Studio can see it"
  cannot silently disagree. Both sides change together or neither does.
- **`native/src/hlcr/renderer.rs`** — a second input branch. The `single` case becomes
  `-i video.<ext> -i sound.wav` with `-c:v copy`. The `hud_only` case is harder and may not be
  copyable at all: it currently `alphamerge`s two BMP sequences, which means decoding both videos
  and re-encoding the result.
- **`desktop-studio/src-tauri/src/capture_manager.rs`** — `take_folder_has_content` and
  `VerifiedBlock`'s two tiers, if the output layout changes shape.
- **`native/src/hlcr/take_meta.rs`** — records the capture FPS per take and warns when a render
  interprets it at a different rate. A video take should carry the same record and the existing
  `[render-fps-mismatch]` check keeps working unchanged. *Note: that module lands with
  `feature/decal-flush-r-and-d`, not on `dev` — so it is not present on this branch yet.*

---

## FFmpeg has to be reachable *by HLAE*, which is a separate question from ours

The app already resolves an FFmpeg for Render Studio through the documented waterfall (User Override
→ Bundled Local → System Path). HLAE does not consult that. It looks in exactly two places, per
`<HLAE>/ffmpeg/readme.advancedfx.txt`:

- `<HLAE>/ffmpeg/bin/ffmpeg.exe`, or
- a path named by `[Ffmpeg] Path=` in `<HLAE>/ffmpeg/ffmpeg.ini`.

**On the working install here, neither exists** — that folder contains only the readme. So this
feature does nothing at all until that is addressed, and it will fail in the least helpful way: a
capture that runs and produces no video.

**Recommended handling: detect, then offer to write the `.ini` — never overwrite one.**

- Writing a two-line `ffmpeg.ini` beats copying the binary. A copy duplicates ~100 MB, and creates a
  second FFmpeg to keep in step with the one Render Studio uses — they would drift, and the two
  halves of the pipeline would be encoding with different builds.
- It should be **offered**, not done silently. It writes into another application's install
  directory, which is not ours the way a scratch file is.
- **An existing `ffmpeg.ini` is never overwritten.** If one is present and points somewhere else,
  report the disagreement and leave it alone. HLAE is shared with other games and projects, and
  silently repointing it would break someone's Source workflow to fix ours. This is the same
  discipline `cfg_scan` applies to the game's own `.cfg` files: detect and state, never assume the
  right to decide.
- If the app's own FFmpeg resolved to a bare `ffmpeg` on `PATH`, it has to be resolved to an
  absolute path before it can be written into the ini.

---

## Staging

1. **Answer the four unknowns with one capture.** A single clip, `-c:v rawvideo`, everything else
   left alone. Record exactly where the video lands, whether `sound.wav` is beside it, whether a
   `take0000` folder still appears, and whether `mirv_movie_filename` steered any of it. Stop here
   and look — items 2 and 3 above decide how much of the rest is a small change or a large one.
2. **FFmpeg availability.** Detection, the offer, and the never-overwrite rule. Independent of
   everything else and safe to land first.
3. **Emit the commands.** `enabled` and `options` into `dodtools_helper.cfg`, behind a setting that
   is off by default. Codec choice exposed, not hardcoded.
4. **Teach the scanner the new shape**, on both sides of the shared predicate at once.
5. **The mux.** Stream-copy plus audio for the simple case. Decide separately whether separate-HUD
   is supported in this mode at first, or falls back to the BMP path — it may not be copyable.
6. **Measure it.** Disk and wall-clock, against a real batch, before and after. The claim that this
   is faster is currently an expectation, not a measurement.

---

## Open questions

- **Does it help capture wall-clock, or only disk?** Assumed, unmeasured. Worth knowing before it
  is described to anyone as a speed feature.
- **What happens when the spawned FFmpeg fails?** The changelog records a fixed deadlock when FFmpeg
  wrote to stderr and exited (`#809`), which says the failure mode was bad enough to hang the game.
  A capture where FFmpeg dies silently mid-batch needs to be detectable — a video file that exists
  but is short is exactly the kind of plausible-looking wrong output this pipeline has been bitten
  by before.
- **Does the take still verify?** `take_folder_has_content` and `is_renderable_take` are what stop a
  failed capture being reported as a success. Neither currently knows what a video take looks like.
- **Separate HUD.** Two video streams that have to be `alphamerge`d cannot be stream-copied, so the
  headline win does not apply to that path. Supported, or excluded at first?
