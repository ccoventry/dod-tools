# Web Preview Viewer — Handoff

**Status:** Proof-of-concept. Transcoder verified against real demos. Renderer: engine boots, mounts our content, and **is confirmed parsing the transcoded demo's network stream** (prints the original recording server's hostname/build/map-cycle straight out of it). Blocked on getting past the level-load stage — see "STOP AND READ" below.
**Last updated:** 2026-08-14 (mid-session handoff, more below)
**Spans two repos:** `dod-tools/xash-transcode/` (this repo) and `../dod-web-demo-viewer/` (sibling).

---

## STOP AND READ — exact state as of 2026-08-14 23:10, mid-session

This got interrupted by a context-limit warning before it finished. Picking
this up cold? Do these in order:

1. **A local static server needs to be running.** It was killed when the
   previous session ended (background process, doesn't survive). Restart it —
   the script is `../dod-web-demo-viewer/dev-server.js`, a real committed-repo
   file (not scratchpad), no dependencies:
   ```
   node dev-server.js . 8080
   ```
   (run from inside `dod-web-demo-viewer/`). Sets COOP/COEP headers and
   serves static files, `.wasm` → `application/wasm` included.

2. **The test content pack already exists** at
   `../dod-web-demo-viewer/assets/dod_test_pack_with_demo.zip` (~90 MB,
   gitignored, real file on disk — survives across chat sessions fine, it's
   not in any scratchpad). It is our real `dod_custom_pack.zip` contents
   (built by `xash-transcode pack` against `analysis_target_pov.dem` and the
   real DoD install at `D:\Games\Steam\steamapps\common\Half-Life -
   PRE-Anniversary for Movies`) **plus** every fix listed in "Step 3 findings"
   below, rebuilt with a custom Node STORED-zip writer (not
   `Compress-Archive` — see why in that section). **Don't rebuild it unless
   you're adding a new fix** — it's current as of the last "Step 3 findings"
   entry.

3. **The URL to open** (real, non-headless browser — a human clicking through
   DevTools has been far more informative than headless automation all
   session):
   ```
   http://127.0.0.1:8080/?pack=/assets/dod_test_pack_with_demo.zip&demo=dodEmmanuelClip&dev=1&autostart=1
   ```
   `dev=1` now auto-enables the real engine console (`-console -dev 2`, see
   `index.html`'s `buildArgs()`) — no more manually toggling "Enable
   developer console" in the menu.

4. **What we're waiting on**: the user was about to paste console output from
   a fresh retest (after the `sprites/hud.txt` fix, the last fix applied) into
   a **new chat**, specifically to check whether a headless-only
   `RuntimeError: Aborted(OOM)` (hit while loading real level content, using
   the CPU software renderer forced for headless compatibility) also happens
   in their real browser, which uses hardware-accelerated WebGL2 by default
   and should have a much better memory profile. **If they paste console
   output, read it against the "Step 3 findings" section below before
   reacting — most of the noise in these logs (missing sounds, missing
   `gfx/shell/*` UI images, `GL_INVALID_ENUM`, `ScriptProcessorNode`
   deprecation) is already-triaged and harmless.** Look specifically for:
   whether it gets past `Remote host: KTP - New York 1` into real level
   content, whether the map actually renders, and whether `RuntimeError:
   Aborted(OOM)` or any *new* fatal message appears.

5. **`index.html` still has TEMPORARY TEST EDITS** (grep for `TEMPORARY TEST
   EDIT`): forces `GAME_DIR = 'valve'` + `hlsdk-portable@0.1.3` client/server
   libs (no compiled DoD wasm exists yet), and ties `-console -dev 2` to
   `?dev=1`. These are real edits in a real file (not scratchpad) — they
   persist. Leave them until step 3 is fully resolved or reverted on purpose.

6. **Everything under this session's scratchpad is gone** in a new chat
   (session-scoped temp dir) — the Playwright driver script, the extracted
   `pack_extract/` working folder, and shallow clones of `xash3d-fwgs`/
   `hlsdk-portable` source used to trace the `hud.cpp` bug. **None of that
   is needed to *use* the existing test pack.** If another fix is needed
   (adding/replacing a file in the pack), the zip itself already persists
   (point 2) — unzip it back into a working folder, edit, and rebuild with
   the other new persisted script:
   ```
   node make-test-zip.js <extracted-folder> assets/dod_test_pack_with_demo.zip
   ```
   (also in `dod-web-demo-viewer/`, not scratchpad — do **not** use
   PowerShell's `Compress-Archive` for this, see finding #3 below for why).
   "Step 3 findings" documents exactly what's in the pack and where each
   piece came from, so extending it doesn't require re-deriving anything.

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
| **Does Xash actually replay a transcoded DoD demo?** | **Looking like yes (2026-08-14) — see "Step 3 findings" for the full trail.** The engine parses signon data out of the transcoded IDEM file and prints the *original recording server's* hostname, build number, and map cycle straight from it (`BUILD 8308 SERVER`, `GoldSrc serverdata packet received.`, `Remote host: KTP - New York 1`, `dod_pandemic_aim`/`dod_orange`). `PROTO_GOLDSRC` demo playback is real. Every blocker hit past that point turned out to be a missing *base-engine* file our demo-scoped test pack had no reason to include (base WADs, `delta.lst`, Xash's replacement UI fonts, `sprites/hud.txt` + base sprites — root-caused the fatal "reinstall" message to `hud.cpp`'s `CHud::VidInit` via the real `hlsdk-portable` source, not a CRC check as first suspected) — all fixed as of 2026-08-14. Last blocker hit: an Out-Of-Memory abort loading real level content, but only in headless testing with the CPU software renderer forced — real-browser retest (hardware WebGL2) pending, not yet confirmed either way. Separately, a real double-free bug surfaced in this engine build's shutdown path — doesn't look content-related. |

---

## Bugs found & fixed while running `pack` (2026-08-13)

First real run — `cargo run -- pack ../demos/analysis_target_pov.dem <sibling>/assets/dod_custom_pack.zip --game-root "D:\...\Half-Life - PRE-Anniversary for Movies"` — reported **376 required files MISSING**, all `model *N` (e.g. `*1`, `*120`). Both turned out to be bugs in `xash-transcode`, not missing content:

1. **Inline BSP submodels treated as files.** GoldSrc names brush entities baked into the map itself (doors, buttons) `*N` — an index into the BSP's own model lump, not a `.mdl` on disk. `ResourceKind::Model::is_file()` didn't exclude them the way `Decal` already was. Fixed with `is_inline_bsp_model()` in `resources.rs`, filtering them out of `resources()` entirely (same treatment as decals). Dropped the false-missing count from 376 to 209.

2. **The bigger one: `BitSliceCast::get_string()` (`dem-patch/src/bit.rs`) doesn't strip NUL padding.** It dumps a fixed-size byte buffer straight to `String` with no C-string termination handling, so short names come back with trailing embedded `\0` bytes (e.g. `"maps/dod_Emmanuel.bsp\0"`). A `\0`-suffixed path can never resolve on any filesystem, so this alone accounted for the rest — confirmed by finding stock files (`dod/models/null.mdl`, `dod/models/allied_ammo.mdl`) reported missing while sitting right there on disk. Fixed with `clean_resource_name()` in `resources.rs`, truncating at the first NUL right where `get_string()` is read.

   **This second bug is scoped narrowly to `xash-transcode` on purpose.** `get_string()` is a shared `dem-patch` API used workspace-wide (`analysis`'s player-name/chat decoding included), and CLAUDE.md gates public-API changes behind explicit request. **The same corruption plausibly affects other `get_string()` callers outside this crate** (e.g. scoreboard player names, chat text) — worth a dedicated look if anyone's seen truncated-looking names or odd trailing characters elsewhere in the app. Not chased further here; out of scope for this handoff.

Both fixes have unit tests in `resources.rs` (`inline_bsp_models_are_excluded`, `resource_names_are_truncated_at_the_first_nul`).

---

## Step 3 findings: booting the viewer against `hlsdk-portable` (2026-08-14)

Ran the real viewer against `analysis_target_pov.dem`'s transcoded 300–320s
clip and the real DoD install, alternating between headless Playwright
(fast iteration, but blind to some things) and the user's real browser
(slower to iterate, but far more informative — real `alert()` dialogs, a
working DevTools console, no headless-specific crashes). **Bottom line so
far: `PROTO_GOLDSRC` demo playback is real.** The engine parses the
transcoded IDEM file's signon data and prints the *original recording
server's* own hostname/build/map-cycle straight out of it. Getting from
there to an actual rendered level has been a long chain of "engine wants a
base file our demo-scoped `pack` tool has no reason to include" bugs, fixed
one at a time. Still not fully resolved — see "STOP AND READ" at the top of
this doc for exactly where this stands and what to check next.

### Fixes applied, in the order they were hit

Each of these was found by actually running the viewer and reading the
real error, not by guessing. All are either code fixes (in `index.html` /
`assets/README.md`, real files, committed-repo-adjacent) or additions to
the **test content pack** (`assets/dod_test_pack_with_demo.zip`, gitignored,
not something end users or the real `pack` tool need to worry about — see
below on why).

1. **Stale CDN path.** `assets/README.md`'s documented
   `hlsdk-portable@latest` paths were wrong — the package added a `valve/`
   layer under `dist/` since that snippet was written. Real paths:
   `dist/valve/dlls/hl_emscripten_wasm32.wasm` and
   `dist/valve/cl_dlls/client_emscripten_wasm32.wasm`. Fixed in both
   `index.html` (pinned `@0.1.3`) and `assets/README.md`.
2. **Missing base WADs.** The engine needs core `valve/` WADs (`gfx.wad` at
   minimum) that `pack` correctly never includes (engine-level, not
   referenced by any map/demo). Without it: a bare `Infinity` thrown
   (not a real `Error` — see point 8) during `Host_InitCommon`.
3. **`Compress-Archive` zips are unsafe for this viewer.** PowerShell's
   `Compress-Archive` writes backslash path separators and an explicit
   directory-marker entry (e.g. `models\player\`) for non-empty
   directories with a trailing *backslash*. The viewer's zip reader only
   recognizes a trailing forward slash as a directory marker
   (`entry.name.endsWith('/')`), so the marker becomes a bogus zero-byte
   *file*, and the next real file under that path fails `FS.mkdirTree()`
   with `ENOTDIR`. Not a `pack`-tool bug (its Rust `zip` crate writes
   forward slashes, no directory markers) — only hit building an ad hoc
   test pack on Windows. **Fix: don't use `Compress-Archive` for a pack
   this viewer will read.** Used `dod-web-demo-viewer/make-test-zip.js`
   instead (a real, committed-repo-adjacent file, not scratchpad — see
   "STOP AND READ" point 6 for usage).
4. **`liblist.gam`'s `gamedll`/`gamedll_linux` decide the *actual* server
   wasm filename — independent of the JS `serverLib` config.** The real
   DoD `liblist.gam` (which `pack` always copies in) declares
   `gamedll_linux "dlls/dod.so"`. The engine derives
   `dlls/dod_emscripten_wasm32.wasm` from that at runtime and 404s (no
   compiled DoD wasm exists), regardless of what the JS wrapper's
   `libraries.server` override points at — that override only covers the
   wrapper's *own* initial dylib load, not this liblist-driven one. **Fix
   (test pack only):** edited the packed `liblist.gam`'s `gamedll`/
   `gamedll_linux`/`gamedll_osx` to say `hl` instead of `dod`, and put real
   copies of `hl_emscripten_wasm32.wasm` / `client_emscripten_wasm32.wasm`
   (from `hlsdk-portable@0.1.3`'s CDN dist) directly in the pack under
   `dlls/` / `cl_dlls/` so any lookup path finds them locally.
5. **Missing `delta.lst`.** Another core engine file (network
   delta-encoding field tables — directly relevant to demo/network message
   decoding) that `pack` has no reason to include. Symptom:
   `Delta_InitFields: couldn't load file delta.lst`, fatal. **Fix:** copied
   `valve/delta.lst` from the real install (used `valve/`'s, not `dod/`'s,
   since we're running the generic `hlsdk-portable` server logic under
   `-game valve` — using DoD's own delta.lst would declare fields that
   server binary doesn't know).
6. **Missing UI fonts.** `Unable to read font file gfx/fonts/FiraSans-Regular.ttf!`
   / `tahoma.ttf!`, repeated, then the same fatal "reinstall" message as
   #7 (this was the *first* thing that ever triggered it — turned out to
   be a font problem, not the more interesting cause found later). These
   TrueType fonts are Xash3D-FWGS's own replacement for GoldSrc's original
   bitmap console fonts — not part of any GoldSrc install at all. **Fix:**
   found and extracted from `dist/valve/extras.pk3` in the `xash3d-fwgs@1.2.2`
   npm package (a bundled zip of engine-supplied extras — this is the
   standard place to look for this category of missing file). Both fonts
   live at exactly `gfx/fonts/FiraSans-Regular.ttf` and
   `gfx/fonts/tahoma.ttf` inside it.
7. **The "reinstall" fatal message, found for real.** `"There is something
   wrong with your game data! Please, reinstall"` recurred even after #6,
   and looked at first like it might be a CRC/consistency check (it fires
   right after `GoldSrc serverdata packet received.`, which made a
   checksum mismatch plausible). **It is not that.** Cloned
   `github.com/FWGS/hlsdk-portable` and grepped for the literal string —
   found in `cl_dll/hud.cpp`, inside `CHud::VidInit` (search "number_0" in
   that file to jump straight there): it calls
   `SPR_GetList("sprites/hud.txt", &m_iSpriteCountAllRes)`, and if the
   resulting sprite list doesn't contain an entry named `"number_0"`
   (`GetSpriteIndex("number_0") == -1`), it prints exactly this message via
   `HUD_MessageBox` and issues `quit`. `sprites/hud.txt` is the manifest
   mapping names like `number_0` (HUD ammo/health digit sprites) to actual
   `.spr` files — a standard base-engine HUD file, never referenced by any
   demo's precache list, same category as everything above. **Fix:**
   copied `valve/sprites/hud.txt` and all 164 `valve/sprites/*.spr` files
   from the real install into the pack (`cp -n`, so demo-specific sprites
   already present from `pack`'s own resource-list-driven packing were not
   clobbered). **Confirmed fixed** — headless retest shows no "reinstall"
   message, no fatal shutdown, and the log now continues well past
   `Remote host: KTP - New York 1` into loading (missing, since audio isn't
   packed) sound files — non-fatal `Error: Could not load sound ...` lines,
   not blockers.
8. **Demo filename must not contain a `.` before `.dem`.** The fixture is
   named `dod_Emmanuel_300-320s.idem.dem` on disk. Passing
   `?demo=dod_Emmanuel_300-320s.idem` produced
   `Error: couldn't open dod_Emmanuel_300-320s.dem` — the engine's
   `playdemo` extension handling treats the `.` before `idem` as an
   existing (wrong) extension and *replaces* everything after it with
   `.dem`, silently losing `idem`. Purely a test-fixture naming collision,
   not a transcoder or viewer bug. **Fix:** renamed the packed copy to
   `dodEmmanuelClip.dem` (no embedded dots) and use `?demo=dodEmmanuelClip`.
9. **`-console` doesn't persist across reloads.** The in-game "Enable
   developer console" toggle is stored in a config file the pack doesn't
   carry, so it resets on every fresh VFS mount. **Fix (permanent,
   `index.html`):** `buildArgs()` now pushes `-console -dev 2` whenever
   `?dev=1` is set, so the real engine console (not just the page-level
   `#log` panel) is on automatically every time.
10. **A real, separate bug found along the way:** headless Chromium threw
    an uncaught `SyntaxError: Failed to execute 'querySelector' on
    'Document'` from inside `_emscripten_get_element_css_size` (WASM
    passed a garbage pointer as a CSS selector string) when interacting
    with the canvas (click, or pressing Escape at the main menu in the
    user's real browser too — this reproduced in both headless *and* real
    Chrome, so it's not purely a headless artifact). Also saw, in the
    engine's own shutdown path after the "reinstall" abort:
    `Mem_FreeBlock: not allocated or double freed (free at
    ../engine/common/cmd.c:604)` — a genuine double-free, and the likely
    source of the bare `Infinity` throws seen throughout (Emscripten
    aborts sometimes surface as a non-Error thrown value with no
    `.stack`). **Neither of these looks caused by our content** — they read
    as upstream engine bugs in this `xash3d-fwgs@1.2.2` build. Not chased
    further; flag if they recur.
11. **Headless-only Out-Of-Memory.** After fix #7, headless Playwright
    (forced to the CPU software renderer, `?renderer=soft`, specifically to
    dodge headless GPU quirks) hit `RuntimeError: Aborted(OOM). Build with
    -sASSERTIONS for more info.` right as it started loading real level
    content. Software rendering keeps the whole framebuffer/texture cache
    in WASM heap rather than GPU memory, so this is plausibly specific to
    that renderer choice + headless memory limits, not a real blocker — a
    real browser uses the default `gl4es`/WebGL2 renderer, which is far
    lighter on system RAM. **This is the open question a fresh retest needs
    to answer** — see "STOP AND READ" at the top of this doc.

### Why none of items 2–8 are `pack`-tool bugs

Every missing file above (`gfx.wad`, `delta.lst`, the fonts, `hud.txt` +
base sprites) is **engine/base-install-level**, not something any specific
map or demo precaches — `pack`'s whole design is "package what *this demo*
needs," scoped correctly. A *real* deployment of this viewer would ship
these once, adjacent to the per-map/per-demo packs `pack` produces, not
reinvent them per pack. Worth designing as: a small fixed "base kit" zip
(the base WADs + fonts + `delta.lst` + `hud.txt`/sprites, all pulled from
the same place a copy of Half-Life provides them) that the viewer always
mounts first, with the per-demo `pack` output layered on top. Not built
yet — the test pack just has everything flattened into one zip for
expedience.

### How to reproduce / continue this

See "STOP AND READ" at the very top of this document — it has the current
exact server-restart command, test URL, and what's being waited on.

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
3. **In progress (2026-08-14) — very close.** See "STOP AND READ" at the top of this doc and "Step 3 findings" for the full trail. Confirmed: the engine parses the transcoded demo's signon data for real (prints the original server's hostname/build/map-cycle out of it). A long chain of missing base-engine files (not transcoder bugs) blocked getting further, all fixed one at a time. Last open question: does it actually render the level, or does a headless-only OOM (CPU software renderer) also happen in a real browser (hardware WebGL2)? Waiting on a real-browser retest to answer this.
4. **Decide based on step 3.** If the world renders: pursue it, and consider a minimal DoD HUD. If `PROTO_GOLDSRC` demo playback is fundamentally broken: stop, and fall back to option B below.

### Fallback if 3D playback proves unworkable

- **Fallback A (cheapest, solves the actual user pain):** keep `hl.exe`, but pre-cut each highlight into its own short `.dem` so "Preview Highlight" launches straight into the clip. The events window disappears from the user's workflow entirely. Uses only the patcher and writer already in this repo. Same delta-compression caveat as risk 3.
- **Fallback B:** 2D replay viewer on the map overview — player dots, killfeed, scrubber. Every position and event is already in `analysis`. Click-to-jump becomes an array index, so seeking is instant and exact. Runs in Tauri and on Pages from one codebase. Rated a worse viewing experience by the project owner, so treat as a supplement rather than the primary.

---

## Conventions for whoever picks this up

- Keep `lib.rs`, `idem.rs`, `resources.rs`, `writer.rs` **free of I/O** — the same transcoder is intended to run in the browser viewer and in Tauri. All filesystem work belongs in `main.rs` / `packer.rs`.
- Keep `reference/*.py` in sync. If Rust and Python disagree for the same input, one has drifted, and the Python is the one that was validated against real demos.
- Don't commit game content or transcoded demos — Valve assets. Both repos' `.gitignore` files already cover this.
