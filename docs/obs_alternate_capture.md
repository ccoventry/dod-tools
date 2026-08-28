# OBS as an Alternate Capture Method

> **Status 2026-08-28 — write-up only. No code, and nothing here has been run against OBS.**
> Tracks [#65](https://github.com/ccoventry/dod-tools/issues/65).
>
> **Scope, set by the user 2026-08-28**, narrowing the issue's own title:
>
> - **Not a replacement.** It sits alongside the existing capture path.
> - **An alternate option for people who care less about quality** and want the convenience.
> - **Separate HUD is out.** "That won't work with OBS", and it is not wanted here. No effort goes
>   into HUD compositing on this path.
>
> Prerequisite reading: `docs/direct_to_video_capture.md` (#42, shipped in PR #68) answered how a
> non-BMP capture artefact flows through take verification, Render Studio's admission predicate and
> export routing. Several of its answers transfer directly and are cited rather than re-derived.
> `docs/goldsrc_dod_quirks.md` and `docs/hlae_protocols.md` hold the engine facts.
>
> **Everything below marked *(unverified)* needs an experiment.** The sections that are settled are
> settled by reading this repository's own code, not by running OBS.

---

## The one fact everything else follows from

`mirv_recordmovie` does not capture in real time. It pins the engine's timestep to
`1 / mirv_movie_fps` and renders **every** step regardless of wall-clock, so a 40-second clip at 120
fps takes as long as the machine needs and comes out frame-exact every time. The same demo captured
twice is byte-identical.

OBS captures a window in real time. It takes whatever the screen actually showed.

That single difference decides the shape of the whole feature:

| | HLAE (`mirv_recordmovie`) | OBS |
|---|---|---|
| engine timing during a clip | `host_framerate` pinned to `1/fps` | `host_framerate 0` — real time |
| wall-clock per clip | minutes for a 40s clip at 120fps | **exactly the clip's duration** |
| high FPS / slow motion | the point of the path | not possible |
| dropped frames | impossible by construction | whatever the machine drops |
| determinism | byte-identical reruns | no |
| audio | separate `sound.wav`, muxed later | in the file already |
| output | thousands of BMPs, or one lossless video | one finished, playable file |
| render pass | required | optional (see below) |

**What OBS actually buys:** wall-clock. A 15-clip session that takes an hour of capture becomes
roughly the length of the clips plus the fast-forward between them, with a hardware encoder doing
the compression for free and a finished file at the end. That is the whole pitch, and it is a real
one for someone who wants their clips today at 60 fps rather than tomorrow at 300.

**What it costs** beyond quality, and what a user has to be told: capture becomes sensitive to
everything else on the machine. An alt-tab, a stutter on map load, a notification sound landing in
desktop audio, a screensaver — all of these are now *in the clip*, and none of them can reach the
existing path.

---

## The fast-forward still works, and that matters

`sys_fast_forward` is `host_framerate 0.05` and `sys_normal_speed` is `host_framerate 0`
(`native/src/patch/builder.rs`). Nothing is being recorded between blocks, so the fast-forward
between highlights is untouched by this change — a batch is still much shorter than the demos it
walks.

Only the recorded window changes, and it changes by **doing nothing**: with no `mirv_recordmovie` in
play, the engine simply stays at `host_framerate 0` — real time — for exactly the span the pre-roll
already dropped it into. The pre-roll's own reason for existing (flushing the audio buffers the
fast-forward corrupts, `docs/goldsrc_dod_quirks.md`) is unchanged and still needed.

**The consequence worth stating loudly:** at `host_framerate 0` the engine advances demo time by
real elapsed time. So a block's wall-clock duration equals its demo-time duration *exactly*, even on
a machine that stutters — a stall costs frames, not seconds. That fact is what makes the
synchronisation problem below far smaller than it looks.

---

## How OBS gets driven

`obs-websocket` v5 ships inside OBS Studio 28 and later; there is no plugin to install. Default port
`4455`, optional password, JSON over WebSocket, SHA-256 challenge/salt handshake.

The requests this needs:

| request | why |
|---|---|
| `GetVersion` | preflight. Returns `obsVersion`, `rpcVersion` **and `availableRequests`** — which is how to feature-detect the optional requests below rather than version-sniffing. |
| `GetRecordStatus` | `outputActive`, `outputDuration`, `outputBytes`. Per-block verification, and the answer to "is OBS already recording something else". |
| `StartRecord` / `StopRecord` | `StopRecord` returns `outputPath` — the artefact's location, which is what take verification keys off instead of a take folder. |
| `RecordStateChanged` (event) | `OBS_WEBSOCKET_OUTPUT_STARTED` / `STOPPED`, with `outputPath` on stop. Preferable to polling: it reports the moment recording *actually* began, which is not the moment `StartRecord` returned. |
| `GetRecordDirectory` | where files will land, for the preflight report and the disk check. |
| `SetRecordDirectory` | *(verify — believed obs-websocket 5.3+ / OBS 30)* per-block export routing without a cross-drive copy. |
| `SplitRecordFile` | *(verify — believed 5.5+ / OBS 30.2, and may be unavailable for some containers)* the basis of Option B below. |
| `GetSceneList`, `GetCurrentProgramScene`, `GetSceneItemList`, `GetInputList`, `GetInputSettings` | read-only scene inspection for the preflight. |
| `GetVideoSettings` | canvas/output resolution and `fpsNumerator`/`fpsDenominator` — the number that replaces `mirv_movie_fps` in `take_meta`. |

**No WebSocket client is in the workspace today.** `native/Cargo.toml` has `tokio` (with `rt`,
`macros`, `process`, `fs`, `sync`, `time`) and `serde_json`, so `tokio-tungstenite` plus a SHA-256
and base64 crate for the handshake is the addition. That is a new dependency on a crate family this
project has not used, and is worth a moment's thought before it lands.

---

## The synchronisation problem, and why it is smaller than it looks

The pipeline schedules commands at exact frame ordinals *inside the demo*, and injects them as
`ConsoleCommand` frames. OBS is an external process that knows nothing about demo ticks. The capture
engine, meanwhile, spawns one `hl.exe` for the entire batch and then does nothing but poll for
`DOD_TOOLS_EXIT_TRIGGER` (`native/src/capture_engine.rs`) — it has no per-block awareness at all.

Something has to cross that gap. There are three ways, and the second is the recommendation.

### The channel: the console log is already tick-accurate telemetry

`build_safe_echos` already writes an echo at every stage boundary of every block:

    [dod-tools] SPEED_FLUSH - Tick 41250
    [dod-tools] AUDIO_SYNC - Tick 41350
    [dod-tools] START_RECORD - Tick 41450
    [dod-tools] STOP_RECORD - Tick 44950
    [dod-tools] FAST_FORWARD - Tick 45150

plus a `BREADCRUMB` every `BREADCRUMB_INTERVAL_TICKS`. With `-condebug` — **which is on by default**
(`add_condebug: true`, `native/src/patch/types.rs`) — these land in `qconsole.log` beside `hl.exe`, a
file the app already knows about and deletes (`shared::paths::remove_console_log`).

So the signalling channel exists, is tick-accurate, needs no new engine commands, and costs the
capture nothing. **The app has simply never read it.**

The alternative the issue proposed — reusing the `DOD_TOOLS_EXIT_TRIGGER` trick, i.e.
`mirv_movie_filename X; mirv_recordmovie_start; mirv_recordmovie_stop` to make a folder appear —
works, but should be rejected here: it starts a real HLAE recording for an instant, which yanks
`host_framerate` to `1/mirv_movie_fps` and back. That is a visible hitch landing *precisely* at the
first frame of the clip, and under direct-to-video it also spawns an FFmpeg process per block.

*(Unverified: `qconsole.log`'s flush cadence. GoldSrc's debug log is believed to write and flush per
line, which would put latency in the millisecond range — but this must be measured **during a live
capture**, with a tailer running and timestamping each line's arrival. A saved log is no use: it
cannot show when its lines were flushed, only that they eventually were. No OBS required, though.)*

**If the log turns out to be buffered**, the fallback is a console command with a filesystem side
effect that is not a recording. `screenshot` is the candidate: comfortably under the 64-byte Cbuf
limit, writes a numbered file into `dod/` that a watcher can map to a block, and costs one frame
rather than a `host_framerate` yank. It litters the game folder, so it would need the same cleanup
treatment the other signal dirs get in `CaptureCleanupGuard`.

### Option A (recommended): one signal per block, stop by timer

1. Tail `qconsole.log`. On `SPEED_FLUSH` (or `AUDIO_SYNC`) for block *i*, send `StartRecord`.
2. Wait for `RecordStateChanged: STARTED`.
3. Stop after `pre-roll remaining + clip duration + post-roll` has elapsed — a **number the builder
   already computed**, and one that is exact because of the real-time property established above.
4. `StopRecord` returns `outputPath`.

Two properties make this work:

- **OBS's start latency is absorbed by the pre-roll**, the same way the issue predicted the
  filesystem-watch latency would be. The pre-roll is at minimum `AUDIO_RESYNC_SECONDS` = 2.0s of real
  time, which is a very large budget for an encoder start.
- **Only the start needs a signal.** The stop is arithmetic. That halves the exposure to log latency
  and means clip duration is exact rather than jittering with whatever the watcher happened to see.

The cost is that the recording contains the pre-roll and post-roll as head and tail footage. See
"Does Render Studio still run" below — this turns out to be an argument *for* keeping it, not a
problem.

### Option B: record continuously, split at boundaries

If `SplitRecordFile` is available, start recording once per chain and split at each block boundary,
discarding the fast-forward segments. No encoder start/stop per clip at all, so no start latency and
no risk of a stop/start cycle being too tight. Costs disk for the discarded segments — small, since
fast-forward is fast — and depends on a request whose availability and container support both need
checking. Worth prototyping if `SplitRecordFile` is present; Option A is the one that works
everywhere.

### Option C: precompute everything. Rejected.

Fast-forward duration is not predictable — it runs as fast as the machine renders — so wall-clock
offsets from batch start cannot be computed ahead of time. At least one signal per block is
mandatory.

---

## The output, and the recommendation that keeps the rest of the pipeline

OBS writes one file wherever its profile points, named by its own filename formatting. `StopRecord`
returns the authoritative path.

**Recommendation: fold it back into the take-folder shape.** After `StopRecord` returns, move the
file to `<block take_folder>/take0000/all/video.<ext>`. On the same volume that is a rename, i.e.
free. The block's `take_folder` is already computed at dispatch (`native/src/patch/builder.rs:788`),
and `shared::paths::take_key` already matches across the `take0000` nesting.

The payoff is large and mostly consists of *not writing code*:

- `collect_image_folders` already admits a stream folder holding a video (`VIDEO_FILE`).
- `stream_video` / `avi_frame_count` already read frame counts out of the container for the progress
  bar — though `avi_frame_count` is AVI-specific, and OBS defaults to MKV. Either constrain the
  container or teach the scanner a second one *(unverified which is less bad)*.
- `renderer.rs` already has the video-input branch, and already omits `-framerate` for a video take
  because a video carries its own timing.
- Export-pool JIT drive routing, output naming (`{demo}_{take}_{stream}_{hash}.{ext}`) and the
  hash-suffixed uniqueness all keep working untouched.
- `take_key` keeps matching, so the capture side and Render Studio still agree about which take is
  which.

The alternative — leaving the file where OBS put it and teaching every downstream component a second
artefact shape — is strictly more work for a worse result.

### The one predicate change

`is_renderable_take` requires **a wav *and* a stream folder**. An OBS take has the video with audio
already inside it and no wav at all, so it fails. This is the single deliberate change, and its own
doc comment states the rule: *"Shared with the capture-side take verification so 'the capture
succeeded' and 'Render Studio can actually see it' can never silently disagree — if this predicate
changes, both sides change together."*

The shape wanted is a third admissible case: a stream folder holding a video **with an audio
stream**, no wav required. Testing for the audio stream rather than just relaxing the wav rule is
what stops a silently-muted OBS take being admitted as renderable — and unlike the frame-sequence
case, that check is cheap, because the container header carries it.

### Take verification gets *stronger*, not weaker

`take_folder_has_content` is deliberately loose — non-empty folder, ignoring our own metadata — so an
unanticipated HLAE layout degrades to a warning. The OBS path can do much better, because the app
knows things it has never known before:

- `GetRecordStatus` reported `outputActive` and a duration for that block.
- `StopRecord` named a file, and that file exists and is non-trivial.
- **The expected duration is known exactly** (the real-time property again), so the recorded duration
  can be asserted within tolerance.

That last check has no equivalent in the BMP path, and it catches precisely the failure this pipeline
keeps getting bitten by: output that exists, looks plausible, and is wrong. `VerifiedBlock` keeps
both tiers — `captured` becomes "OBS reported a file of about the right length", `renderable` stays
the shared predicate.

`take_meta` needs no structural change: it records the capture rate per take, and OBS's rate from
`GetVideoSettings` goes in the same field. The existing `[render-fps-mismatch]` check keeps working.

---

## Does Render Studio still run?

**Yes, and it should be offered rather than skipped** — the opposite of the issue's title, and the
head/tail footage from Option A is why.

The render pass on this path is not an encode of thousands of bitmaps. It is: trim the pre-roll and
post-roll, apply the pipeline's naming, and route to the export pool. On a compact hardware-encoded
file that is seconds of work, and it delivers exactly the consistency the issue worried about losing.

Trim precision is the one real decision:

- `-ss` with `-c copy` snaps to a keyframe. OBS's default keyframe interval is around 2s, which is
  the same order as the pre-roll — too coarse.
- Re-encoding gives a frame-exact trim, and on an already-compressed clip it is fast. Given this path
  exists for people trading quality for convenience, a second encode is a smaller compromise here
  than anywhere else in the pipeline.

Recommendation: re-encode, and expose "leave the take as OBS wrote it" as the skip. Do **not** ask
the user to change OBS's keyframe interval — see below.

---

## The capture source is the real unknown

OBS needs a source pointed at the game. `hl.exe` runs **windowed** (`-windowed` is hardcoded in
`build_hlae_process`) at the configured `-w`/`-h`, which helps: a windowed target is what Window
Capture handles best, and the resolution is known ahead of time, so the OBS canvas can be checked
against it in the preflight rather than discovered by scaling artefacts.

**Game Capture is the risk.** HLAE's `AfxHookGoldSrc.dll` is already injected into `hl.exe` and
already hooks its OpenGL presentation. OBS Game Capture injects its own graphics hook into the same
process to do the same thing. People do run OBS Game Capture alongside HLAE for Source-engine games,
so this is not obviously fatal — but GoldSrc under `-customLoader` in a 32-bit process is not that
configuration, and it has not been tried here. *(Unverified, and it is the experiment with the most
riding on it.)*

Fall back to **Window Capture**, which does not inject anything and therefore cannot collide, at the
cost of requiring the window to stay unoccluded. Display Capture is the last resort.

**On the scene itself: detect and state, never mutate.** This is the same discipline `cfg_scan`
applies to the game's own `.cfg` files and the direct-to-video work applies to HLAE's `ffmpeg.ini` —
someone's OBS scene collection is their streaming setup, not our scratch space. Creating a scene, or
repointing an existing source, could break a livestream to fix a capture. The preflight should report
what it found and what is missing, in the app, and stop there. The same rule covers the recording
format, encoder, keyframe interval and file-splitting settings: read them, report anything that will
produce a bad result, change nothing.

---

## Audio: solved, and newly breakable

The genuine simplification. OBS records audio into the file, so the separate `sound.wav` and its mux
disappear, along with the FPS-mismatch class of bug that comes from the two being timed independently
(`docs/hlae_protocols.md`: the wav's duration is the frame count's cross-check — with one file there
is nothing to cross-check, because nothing can drift).

Three things it breaks that the existing path cannot:

1. **Desktop Audio captures the whole machine.** A Discord notification during a clip is in the clip.
   Application Audio Capture targeting `hl.exe` avoids this and is the right recommendation, with the
   caveat that it is a newer Windows capture method. *(Unverified against this setup.)*
2. **`stopsound` fires audibly.** `sys_record_start` is `mirv_recordmovie_start; stopsound`, and
   `sys_sound` fires a `stopsound` a second before it. Under HLAE those land outside or at the very
   edge of the captured audio; under OBS the microphone is already open, so a `stopsound` cutting
   sustained sounds is *recorded*. Whether the pre-roll's `stopsound` is needed at all once nothing is
   being time-warped through the record window is worth asking — it exists to fix fast-forward audio
   corruption, and Option A starts OBS before the flush anyway, putting it in the discarded head.
3. **Nothing enforces that audio was captured.** A muted source produces a perfectly valid file.
   Hence the audio-stream test in the predicate above.

---

## Batch lifecycle: what must not be forgotten

The capture engine's failure and cancel paths currently assume the only external process worth
worrying about is one it spawned. OBS is not.

- **Cancellation.** `taskkill /F /IM hl.exe` fires and the batch unwinds — with OBS still recording,
  forever, into the user's drive. `StopRecord` must be part of the cancel path.
- **Crash.** The engine already distinguishes "hl.exe never started", "crashed mid-capture" and "exit
  trigger seen". Every one of those exits needs the same `StopRecord`.
- **`CaptureCleanupGuard`.** This is the right home for it: it already runs on drop for every path out
  of the batch, which is exactly the guarantee needed. It is not currently async and the WebSocket
  client is — worth resolving deliberately rather than by standing up a runtime inside a `Drop`.
- **Preflight, and fail fast.** OBS not running, WebSocket refused, wrong password, already recording,
  no source pointed at the game — every one of these otherwise produces a batch that runs to
  completion and captures nothing. The existing pre-launch drive-headroom check is the precedent:
  refuse before spawning, not after.
- **Disk accounting is wrong for this path.** `build_batch_queue` bin-packs blocks across drives by
  estimated BMP footprint, and the engine re-validates `drive_headroom` before launch. OBS output is
  one or two orders of magnitude smaller and lands wherever OBS points, so those estimates would
  refuse batches that would fit comfortably. The check should move to OBS's own record directory with
  an estimate sized to this path.
- **`_route_N` junctions and `mirv_movie_filename` become dead weight** — HLAE writes nothing. They
  can stay (harmless, and the `DOD_TOOLS_EXIT_TRIGGER` alias still uses the mechanism) or be skipped;
  no reason to touch them in a first version.
- **Capture modes are mutually exclusive.** Frame sequence / direct-to-video / OBS is a three-way
  choice, not three checkboxes. `ffmpeg_capture` and an OBS mode must not both be settable.
- **`MIN_TAKE_SEPARATION_SECONDS`** (1.0s) exists because a `mirv_recordmovie` stop/start cycle that
  tight risks a take landing without audio. The rule transfers — OBS also needs time to finalise a
  file and start an encoder — but the number was derived for HLAE and should be re-derived, or Option
  B used to sidestep it entirely.

---

## Where this touches the codebase

- **`native/src/capture_engine.rs`** — the batch loop gains a log tailer and an OBS client;
  `CaptureCleanupGuard` gains `StopRecord`; the pre-launch checks gain a preflight.
- **`native/src/obs/`** (new) — WebSocket client, handshake, request/event types. Behind
  `#[cfg(not(target_arch = "wasm32"))]` like the rest of the process/IO surface.
- **`native/src/patch/types.rs`** — `PatcherConfig` gains the OBS settings beside `ffmpeg_capture`
  and `ffmpeg_capture_codec`, which are the pattern to copy.
- **`native/src/hlcr/scanner.rs`** — `is_renderable_take` gains the video-with-audio case;
  `avi_frame_count` needs a companion if the container is not AVI.
- **`native/src/hlcr/renderer.rs`** — a trim branch on the existing video-input path.
- **`desktop-studio/src-tauri/src/capture_manager.rs`** — `VerifiedBlock`'s two tiers gain the
  duration assertion; settings plumbing for host/port/password.
- **`desktop-studio/src/`** — capture-mode selector, connection settings, preflight report. Every
  `invoke()` needs its `.catch()`.
- **`native/src/strings.rs`** — new user-facing strings go here, per the centralisation pass.

Nothing in `dod/`, `analysis/`, `dem-patch/` or the patcher's frame-writing code is affected. This
feature does not change a single injected frame — which is the strongest argument for it being a
tractable piece of work.

---

## Staging

0. **Prove Game Capture works, by hand, with no code at all.** Start an ordinary one-clip batch; while
   it runs, add a Game Capture source on `hl.exe` in OBS and hit Record. This is the make-or-break
   unknown and it needs nothing built to answer. If Game Capture fails, try Window Capture — the game
   is already `-windowed` — and the feature survives with a caveat.
1. **Tail the console log.** No OBS involved. Prove `qconsole.log` reports `START_RECORD` promptly
   enough, measured during a live capture (see above — a saved log cannot answer this). If it is slow
   or buffered, Option A needs rethinking, and it is better to know that now than after a WebSocket
   client is written.
2. **Talk to OBS.** Connect, authenticate, `GetVersion`, `GetRecordStatus`, one manual
   `StartRecord`/`StopRecord`, timing the gap between the request returning and
   `RecordStateChanged: STARTED`. Record what `availableRequests` contains — that settles
   `SetRecordDirectory` and `SplitRecordFile` without version archaeology.
4. **Wire one block end to end.** Log tail → `StartRecord` → timed stop → move into the take folder.
   Verify against the demo that the clip contains what it should.
5. **The predicate**, both sides at once, plus the duration assertion in `VerifiedBlock`.
6. **Preflight, cancel and crash paths.** Not optional, and not last in practice — a half-wired
   version that leaves OBS recording after a cancel is worse than no version.
7. **The trim pass in Render Studio**, and the setting to skip it.
8. **Measure it.** Wall-clock and disk against a real batch, next to the same batch through the
   existing path. The speed claim is currently an expectation.

---

## Open questions

- **Does OBS Game Capture coexist with `AfxHookGoldSrc`?** The one that could reshape the feature.
- **`qconsole.log` flush cadence.** Decides whether Option A's latency budget is real.
- **Which container.** MKV is OBS's crash-safe default but is not what `avi_frame_count` reads; MP4
  risks an unfinalised file if OBS dies. Remux-on-stop is OBS-side behaviour worth reading before
  choosing.
- **`SetRecordDirectory` and `SplitRecordFile` availability** on the installed build.
- **Is `stopsound` still wanted in the record window** once nothing is being time-warped through it?
- **What happens when the machine cannot keep up.** Dropped frames are invisible in the output file's
  metadata — the duration is right and the frames are missing. `GetRecordStatus` does not report
  skipped frames; OBS's own stats do. Whether that is reachable over the WebSocket, and whether a
  batch should warn, is unanswered.
- **Whether OBS should be driven at all when it is already streaming.** `GetRecordStatus` says whether
  it is recording; it does not make refusing to touch a live stream automatic. Leaning toward: detect
  an active stream and refuse, loudly.
