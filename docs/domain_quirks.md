# 🎮 GoldSrc Engine & HLAE Quirks

## Memory & Execution Rules
- **The Initialization Rule:** The `DemoStart` (Type 2) frame must be processed *before* any `ConsoleCommand` (Type 3) frames are written. Injecting commands prior forces the engine to read uninitialized memory buffers, triggering fatal `MAX_POSSIBLE_MSG` crashes.
- **The Cbuf Overflow (Buffer Bomb):** GoldSrc has a strict 64-byte payload limit for command strings inside macro frames. Injecting long absolute paths alongside configuration commands in a single tick saturates `Cbuf_AddTextToBuffer`, silently discarding commands. Command payloads must be staggered across multiple ticks prior to the target frame.
- **Audio Desync on Time Warping:** Fast-forwarding (`host_framerate 1`) breaks engine audio buffers. The speed must drop back to real-time (`host_framerate 0`) exactly 2 to 4 seconds prior to injecting `mirv_recordmovie_start` to flush and resync the audio engine.
- **The First-Load Black Map Bug:** GoldSrc fails to render lighting on the first demo load of a session. A stripped "Primer Demo" must be loaded first, which then daisy-chains into the real demo via `playdemo` to pre-cache map assets.
- **The Post-Roll Jailbreak:** Injecting the terminal capture command (e.g., `DOD_BATCH_DONE`) at `record_stop_tick` acts as an immediate kill switch, jailbreaking the engine out of the configured post-roll screen time. Terminal commands must strictly be delayed until `post_roll_end_tick`.
- **High-Precision Frame Pacing:** `host_framerate` accepts high-precision decimals (e.g., `0.00001`). This can be used as an "infinite microscope" for frame-by-frame engine debugging, or to artificially stretch network packet processing across thousands of physical frames to prevent `SZ_GetSpace: overflow on netchan->message` crashes during heavy map initialization bursts.

## Data Parsing Rules
- **Playdemo Stream Streamlining:** The `playdemo` command acts as a pure sequential stream reader and bypasses the trailing directory index table. It is immune to directory offset mismatch crashes, making it mandatory for automated pipelines over `viewdemo`.
- **British Faction Mis-assignment:** The native parser drops British players into Allies or Unassigned categories. Faction tracking requires dynamically upgrading Allies entities to the British faction when `BritishRifleman` or `BritishMortar` classes are explicitly detected.
- **Reconnect Stat Wipes:** DoD 1.3 servers forcefully reset a player's scoreboard stats to zero upon reconnecting. The analyzer must ignore absolute server totals on reconnects and manually accumulate raw `DeathMsg` packet deltas.
- **Alignment:** Network messages expect exactly 468 bytes of header data before reading payload lengths.
- **Filenames:** Filenames for playdemo calls must be strictly alphanumeric with underscores (_) and under 40 characters.
- **HLAE Commands:** Use mirv_movie_filename and mirv_movie_fps. Do not use modern CS:GO commands like mirv_streams.
- **Alias Nested Quotes (The Exec Bypass):** GoldSrc cannot parse nested quotes inside aliases. When complex commands like absolute paths require quotes, they must be written to individual `.cfg` files and executed via `exec filename.cfg` instead of injected directly into an alias.
- **Strict Path Slashes:** HLAE strictly requires backslashes (`\`) for file paths. Forward slashes (`/`) will cause directory creation or path resolution failures. When generating paths dynamically in Rust, you must explicitly sanitize and replace all forward slashes with double-escaped backslashes (`.replace("/", "\\\\")`).
