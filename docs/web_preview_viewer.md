# Web Preview Viewer — Handoff

**Status:** Proof-of-concept. Transcoder verified against real demos; renderer unproven.
**Last updated:** 2026-08-13
**Spans two repos:** `dod-tools/xash-transcode/` (this repo) and `../dod-web-demo-viewer/` (sibling).

---

## Goal

An in-app demo viewer for **triage only** — deciding whether a detected highlight is worth capturing. Not a capture path.

Requirements, in priority order:

- World (map) + player models + a first-person POV.
- Some HUD for the POV. Nice-to-have, not blocking.
- **No audio required.**
- Highlight list beside the viewer; clicking a highlight jumps to that clip.
- Must load fast — each preview is one short, single-highlight demo.

### Why this exists

The current "Preview Demo" flow launches `hl.exe` with HLAE injected and hands the user a full demo with `svc_director` events in it. The user must then drive playback manually through DoD's events window. Most of the DoD 1.3 community doesn't know that window exists. The flow also dumps the user out of the app entirely.

### Architectural decision

**Tauri's frontend is a webview, so the web viewer and the in-app viewer are the same artifact.** Build it once as a static web app; embed it in `desktop-studio/` and publish it to GitHub Pages from the same source. There is no desktop-vs-web fork to maintain.

---

## Engine findings (verified — do not re-derive)

Read from `xash3d-fwgs/engine/client/cl_demo.c` on `master`, cross-checked against `dem-patch/src/demo_parser.rs`.

### Xash3D cannot open a DoD demo

Different containers. `CL_ParseDemoHeader` rejects on `hdr->id != IDEMOHEADER` before reading anything else.

| | GoldSrc (`.dem` from `hl.exe`) | Xash3D |
|---|---|---|
| magic | `"HLDEMO\0\0"` (8 bytes) | `"IDEM"` (i32) |
| demo protocol | 5 | 3 |
| path fields | `[260]` | `[64]` |
| extras | `map_checksum` | `host_fps` (f64), `comment` |

The "GoldSrc protocol 48 support" in Xash3D is for **connecting to GoldSrc servers** (`connect ip:port gs`), not for reading demo files. Don't be misled by it — see `Documentation/goldsrc-protocol-support.md` upstream.

### The one door that is open

```c
#define PROTOCOL_GOLDSRC_VERSION_DEMO (PROTOCOL_GOLDSRC_VERSION | BIT(7))  // 176
```

`CL_ParseDemoHeader` accepts `net_protocol == 176`, and `CL_GetProtocolFromDemo` then returns `PROTO_GOLDSRC`, so the engine decodes the contained messages with GoldSrc protocol-48 semantics. A DoD demo's payload is already exactly that. **Only the wrapper is wrong** — hence the transcoder.

### Structural facts confirmed against real demos

- GoldSrc `DemoInfo` block preceding each network message is **436 bytes**: `timestamp(4) + RefParams(232) + UserCmd(52) + MoveVars(132) + view(12) + viewmodel(4)`.
- `SequenceInfo` maps **field-for-field** onto Xash's `CL_ReadDemoSequence` — same seven i32s, same order. Straight copy, no translation.
- DoD demos have two directory entries: `LOADING` (type 0) and `Playback` (type 1). Maps cleanly onto Xash's `DEMO_STARTUP` / `DEMO_NORMAL`.
- Largest observed network message: **27,545 B**, under `MAX_INIT_MSG` (0x8000). Headroom exists but is not large.
- `DirectoryEntry.frame_count` in the header is **unreliable** — it counts only network-message frames, not total frames. Walk the frames instead.

### Frame type mapping (9 GoldSrc → 6 Xash)

| GoldSrc | Xash | note |
|---|---|---|
| `NetworkMessage(Start)` | `dem_norewind` (1) | signon |
| `NetworkMessage(Normal)` | `dem_read` (2) | gameplay stream |
| `DemoStart` | `dem_jumptime` (3) | resets section clock |
| `NextSection` | `dem_stop` (6) | terminator; a section without one is treated as corrupt |
| `DemoBuffer` | `dem_userdata` (4) | opt-in (`--userdata`), off by default |
| `ConsoleCommand` | — | dropped |
| `ClientData` | — | dropped |
| `Event` / `WeaponAnimation` / `Sound` | — | dropped |

**Dropping `ConsoleCommand` is confirmed acceptable.** Those carry injected director commands for the real capture demo; preview demos need none of that. This is why transcoded demos are explicitly *not* a capture path.

---

## What exists now

### `dod-tools/xash-transcode/` — standalone crate

Declares its own `[workspace]`, so it does **not** join the dod-tools workspace and the root `Cargo.toml` is untouched. Add it to `members` when you want `cargo build --workspace` to cover it.

```
src/lib.rs        transcode / cut / validate  — pure, no I/O, wasm32-clean
src/idem.rs       Xash IDEM constants + layout, annotated with cl_demo.c references
src/resources.rs  svc_resourcelist + BSP entity-lump extraction — pure, wasm32-clean
src/writer.rs     minimal LE byte writer (dem::byte_writer is a private module)
src/packer.rs     filesystem + zip layer for `pack` — binary only, NOT in the lib
src/main.rs       CLI
reference/        Python oracles the Rust was derived from — keep as cross-check
fixtures/         a pre-built 20s transcoded clip for testing the viewer
```

```bash
cd xash-transcode
cargo run --release -- inspect  ../demos/analysis_target_pov.dem
cargo run --release -- convert  ../demos/analysis_target_pov.dem /tmp/full.dem
cargo run --release -- cut      ../demos/analysis_target_pov.dem /tmp/clip.dem 300 320
cargo run --release -- validate /tmp/clip.dem
cargo run --release -- pack     ../demos/analysis_target_pov.dem /tmp/pack.zip \
                                --game-root "C:/.../steamapps/common/Half-Life"
```

**Parse mode matters.** Use `MessageDataParseMode::Raw` for transcode/cut — payloads stay borrowed bytes, so it's byte-exact and much faster. `pack` needs `Parse` to reach `svc_resourcelist`; that's why it's slow, and it runs once per demo rather than once per preview.

### `../dod-web-demo-viewer/` — sibling repo, static site

```
index.html             canvas host, CDN ESM bootstrap, ZIP mount, ?demo= query wrapper
coi-serviceworker.js   COOP/COEP shim (GitHub Pages cannot set headers)
assets/README.md       content layout + the game-DLL problem
```

Non-obvious details already handled — don't "fix" these:

- **Every engine `.wasm` is explicitly mapped in `filesMap`.** The wrapper's `locateFile` is `filesMap[path] ?? path` with no fallback to the module's origin; an unmapped binary resolves against the *page* URL and 404s.
- **Import must be `https://cdn.jsdelivr.net/npm/xash3d-fwgs@1.2.2/+esm`.** `dist/index.js` is TS output using extensionless relative specifiers, which native browser ESM cannot resolve.
- **Root-absolute asset paths are rewritten** through `document.baseURI`, because a GitHub Pages project site serves from `/<repo>/`.
- **`downloadAndExtractZip` is ours, not the library's.** `xash3d-fwgs` exposes only the raw Emscripten FS. Implemented in `index.html` (store + deflate + ZIP64 via native `DecompressionStream`) and grafted on via `Object.create(xash.em.FS)`.
- **Boot order is load-bearing:** `new Xash3D()` → `await init()` → mount assets → `main()`. Writing files before `init()` throws; after `main()` races the gamedir scan.

---

## Measured results

`analysis_target_pov.dem` — dod_Emmanuel, 66.5 MB, 986 s:

| output | size | % of source |
|---|---|---|
| full transcode | 16.2 MB | 24.3% |
| 86 s clip | 1.5 MB | 2.3% |
| 30 s clip | 524 KB | 0.8% |
| **20 s clip** | **369 KB** | **0.6%** |

The shrink comes from dropping `ClientData` + `DemoBuffer`, which are ~76% of frames and carry nothing the engine needs for playback. A 369 KB preview satisfies the load-fast requirement comfortably.

---

## Verification status — read this before trusting anything

| Component | Status |
|---|---|
| HLDEMO struct layout | **Verified.** All four sections of two real demos walked to *exactly* their declared `file_length`. |
| HLDEMO → IDEM transcode | **Verified** in Python against real demos; output passes a validator reimplementing `CL_ParseDemoHeader`, `CL_PlayDemo_f`, and the `CL_DemoReadMessage` walk. |
| Clip cutting | **Verified** structurally (valid output at several time windows). Visual correctness unproven. |
| Rust port of the above | **Builds and runs clean (2026-08-13).** `cargo build` succeeded with zero errors/warnings on first try — no fixes needed. `inspect`/`convert`/`cut`/`validate` all ran against `analysis_target_pov.dem`: full-transcode and 300–320s-cut sizes match this doc's earlier measured-results table almost exactly (16,156,373 B vs. "16.2 MB"; 368,943 B vs. "369 KB"), and the fresh cut is **byte-for-byte identical** to the checked-in `fixtures/dod_Emmanuel_300-320s.idem.dem`. `validate` (the `CL_ParseDemoHeader`/`CL_PlayDemo_f`/`CL_DemoReadMessage` reimplementation) passes on both outputs. Could not re-run the Python oracles directly for a true side-by-side diff — this machine has no real Python interpreter (only the Windows Store stub alias); the fixture comparison above is the closest available substitute. |
| BSP wad/sky extraction | **Logic verified** in Python across 11 cases (backslash/forward-slash paths, duplicates, junk segments, missing keys, worldspawn scoping, header offsets). Rust port unproven. |
| `svc_resourcelist` extraction | **Exercised and fixed (2026-08-13).** Ran `pack` against `analysis_target_pov.dem` + the real `dod` install and found two real bugs, both fixed in `xash-transcode/src/resources.rs` (see below). After fixing, `pack` reports **"All required files found"** (268 wanted, 262 packed, 80.4 MB → 53.5 MB zipped) with 28 legitimate size-mismatched viewmodels (`v_*.mdl`, local ~3× the server-declared size — this install has HD/modified viewmodels, not the set the demo's recording client used; a real content-mismatch, not a bug). |
| Viewer JS (ZIP, query string, path resolution) | **Verified** — byte-exact ZIP roundtrip on deflate + store, 22 query-string cases, zip-slip rejection. |
| **Does Xash actually replay a transcoded DoD demo?** | **UNKNOWN. This is the project's central open question.** |

---

## Bugs found & fixed while running `pack` (2026-08-13)

First real run — `cargo run -- pack ../demos/analysis_target_pov.dem <sibling>/assets/dod_custom_pack.zip --game-root "D:\...\Half-Life - PRE-Anniversary for Movies"` — reported **376 required files MISSING**, all `model *N` (e.g. `*1`, `*120`). Both turned out to be bugs in `xash-transcode`, not missing content:

1. **Inline BSP submodels treated as files.** GoldSrc names brush entities baked into the map itself (doors, buttons) `*N` — an index into the BSP's own model lump, not a `.mdl` on disk. `ResourceKind::Model::is_file()` didn't exclude them the way `Decal` already was. Fixed with `is_inline_bsp_model()` in `resources.rs`, filtering them out of `resources()` entirely (same treatment as decals). Dropped the false-missing count from 376 to 209.

2. **The bigger one: `BitSliceCast::get_string()` (`dem-patch/src/bit.rs`) doesn't strip NUL padding.** It dumps a fixed-size byte buffer straight to `String` with no C-string termination handling, so short names come back with trailing embedded `\0` bytes (e.g. `"maps/dod_Emmanuel.bsp\0"`). A `\0`-suffixed path can never resolve on any filesystem, so this alone accounted for the rest — confirmed by finding stock files (`dod/models/null.mdl`, `dod/models/allied_ammo.mdl`) reported missing while sitting right there on disk. Fixed with `clean_resource_name()` in `resources.rs`, truncating at the first NUL right where `get_string()` is read.

   **This second bug is scoped narrowly to `xash-transcode` on purpose.** `get_string()` is a shared `dem-patch` API used workspace-wide (`analysis`'s player-name/chat decoding included), and CLAUDE.md gates public-API changes behind explicit request. **The same corruption plausibly affects other `get_string()` callers outside this crate** (e.g. scoreboard player names, chat text) — worth a dedicated look if anyone's seen truncated-looking names or odd trailing characters elsewhere in the app. Not chased further here; out of scope for this handoff.

Both fixes have unit tests in `resources.rs` (`inline_bsp_models_are_excluded`, `resource_names_are_truncated_at_the_first_nul`).

---

## Open risks

1. **`PROTO_GOLDSRC` may never have been exercised for demo playback.** That code path exists for live GoldSrc server connections. The transcoder makes this testable for the first time; it may simply not work. This is the highest-impact unknown and should be resolved before any further investment.
2. **No DoD client library exists for wasm.** Valve never released DoD's source, so it can't be compiled — it would have to be written. See below.
3. **Delta compression on cuts.** `svc_deltapacketentities` encodes against earlier frames, so a cut landing mid-stream shows corrupt entities until the next full update. `--preroll` (default 3 s) is a blunt mitigation. Proper fix — walk forward to the first non-delta `svc_packetentities` — needs `dem::netmsg_doer` and a second parse pass. TODO in `lib.rs::cut`.
4. **Custom content mismatch.** DoD's custom-model culture means demos reference files a given install may lack or have different versions of. `pack` reports size mismatches against server-declared sizes to surface this.

---

## DoD client library — assessment

**The blocker is not compilation, it's that the source doesn't exist publicly.** DoD is not in the HLSDK. "Compile it" isn't the task; "write it" is.

For triage-only viewing, far less is needed than a full client:

- **World (BSP v30)** — engine-side. Any client works.
- **Player models** — engine-side studio renderer, driven by entity state from the demo stream. Any client works.
- **POV camera** — comes from the recorded stream; stock `V_CalcRefdef` should be close.
- **HUD** — the only part genuinely needing DoD code.

`hlsdk-portable`'s client wasm is already published on npm, so testing "does DoD render at all with a foreign client" is cheap. Expect world + players + movement and no meaningful HUD — a silent film, which for triage may be sufficient.

If a minimal DoD HUD is later wanted, note that **the wire formats are already reverse-engineered in this repo** — `dod/src` has the message parsers and `analysis/` decodes scoreboards, kills and chat. Writing DoD usermessage handlers in C++ is transcription from existing Rust, not discovery. Health/ammo/class is plausibly weeks. Full parity is not worth attempting.

---

## Next steps, in order

1. ~~`cargo build` the crate and fix compile errors.~~ **Done (2026-08-13) — built clean, no errors.** CLI validated against `analysis_target_pov.dem`; output matches this doc's measured sizes and is byte-identical to the checked-in fixture for the 300–320s cut (see verification table above). A true byte-for-byte diff against the Python oracles is still outstanding — needs a machine with a real Python interpreter (not the Windows Store stub).
2. ~~Build a content pack for one map.~~ **Done (2026-08-13).** `dod_custom_pack.zip` written to `../dod-web-demo-viewer/assets/` (gitignored there already) from `analysis_target_pov.dem` against the real `dod` install — 262 files, 53.5 MB zipped, all required files found after fixing the two bugs above. Only open item: the 28 mismatched viewmodels noted in the verification table — decide whether the *viewer's* content pack should prefer a vanilla viewmodel set over whatever install happens to build the pack, so played-back demos don't quietly pick up a movie-mod's oversized models.
3. **Point the viewer at `hlsdk-portable`'s client/server wasm** with `-game dod` (see `assets/README.md` for the exact `CONFIG` override), load `fixtures/dod_Emmanuel_300-320s.idem.dem`, and open with `?dev=1`. **The engine console output is the deliverable here** — it will say precisely where playback dies.
4. **Decide based on step 3.** If the world renders: pursue it, and consider a minimal DoD HUD. If `PROTO_GOLDSRC` demo playback is fundamentally broken: stop, and fall back to option B below.

### Fallback if 3D playback proves unworkable

- **Fallback A (cheapest, solves the actual user pain):** keep `hl.exe`, but pre-cut each highlight into its own short `.dem` so "Preview Highlight" launches straight into the clip. The events window disappears from the user's workflow entirely. Uses only the patcher and writer already in this repo. Same delta-compression caveat as risk 3.
- **Fallback B:** 2D replay viewer on the map overview — player dots, killfeed, scrubber. Every position and event is already in `analysis`. Click-to-jump becomes an array index, so seeking is instant and exact. Runs in Tauri and on Pages from one codebase. Rated a worse viewing experience by the project owner, so treat as a supplement rather than the primary.

---

## Conventions for whoever picks this up

- Keep `lib.rs`, `idem.rs`, `resources.rs`, `writer.rs` **free of I/O** — the same transcoder is intended to run in the browser viewer and in Tauri. All filesystem work belongs in `main.rs` / `packer.rs`.
- Keep `reference/*.py` in sync. If Rust and Python disagree for the same input, one has drifted, and the Python is the one that was validated against real demos.
- Don't commit game content or transcoded demos — Valve assets. Both repos' `.gitignore` files already cover this.
