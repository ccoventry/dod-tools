---
trigger: always_on
description: Critical rules and constraints regarding the GoldSrc engine, HLAE, and demo parsing.
---

# 🎮 GoldSrc Engine & HLAE Quirks

### Engine Alignment & Memory
* **Strict 468-Byte NetworkMessage Alignment:** GoldSrc network frames (types 0 and 1) require exactly 468 bytes of header data before reading the message payload length (read offset `464..468`). Miscalculating this by 4 bytes (e.g., using 464 bytes) causes the parser to read subsequent string characters as message length integers, prompting massive allocation attempts (up to 1.7GB) that crash at EOF.
* **Memory Buffer Initialization Crash:** GoldSrc console commands (e.g., `host_framerate`, `startmovie`) MUST be injected AFTER the DemoStart (Type 2) frame to prevent uninitialized memory buffer crashes. Injecting them before this frame causes the engine to crash.
* **Directory Offset Patching:** Any modification to demo frame counts (such as injecting new commands) requires a manual binary patch to both the `frame_count` and `file_length` integers in the Demo Directory at the end of the file, alongside a physical shift of the start offset for all subsequent directory entries.
* **Directory Offset Boundary Termination:** GoldSrc demo files append a frame directory at the end of the file. The frame parsing loop must check `stream_position >= directory_offset` (or the calculated `original_offset`) to terminate cleanly. Failure to check this boundary results in parsing directory bytes as frame headers, causing desync.
* **NextSection (Type 5) Traversal:** A type 5 frame (`NextSection`) is written between the LOADING and PLAYBACK sections. The patcher must not exit parsing on the first `NextSection` frame, or it will skip patching highlights located in the PLAYBACK entry.
* **DemoBuffer (Type 9) Integrity:** Do not write the `buffer_length` twice when reconstructing or patching type 9 frames, as this corrupts sequential frame offsets.
* **The Uptime Tick Trap:** The `LOADING` directory frames (pre-`DemoStart`) contain massive tick values inherited from server uptime. Never inject `ConsoleCommand`s based purely on tick counts without first validating that the `DemoStart` frame has passed and the tick clock has reset.
* **Filename Truncation:** `playdemo` filenames must be strictly < 40 characters to avoid silent truncation by the 1998 engine parser.

### Data & Logic
* **Demo Handshake Dependency:** GoldSrc demos are not standard file streams; the first ~2 seconds of the file contain critical server handshake packets (e.g., `SvcServerInfo`, `SvcResourceList`). Slicing a demo by timestamp without preserving these headers causes the engine to crash on load.
* **Reconnect Stat Reset:** Standard DoD 1.3 servers reset a player's kills/deaths/score to zero upon reconnection, even if the Steam ID is identical. The `Frags` and `ScoreShort` network packets reflect these reset values, effectively overwriting pre-disconnect stats.
* **DeathMsg Persistence:** Unlike absolute stat packets, `DeathMsg` packets are broadcast events that exist in the demo stream for the entire session. Counting kills/deaths via `DeathMsg` is the only source-of-truth for cumulative player stats across reconnects. Local analyzers must track deltas via these packets.
* **Stateless Event Pipeline:** The analyzer core must avoid stateful accumulators (like `HashMap<u8, (i32, i32)>`) in the primary parser. Stats should be derived from a sequence of stateless `TimelineEvent` objects.
* **POV vs. HLTV Discrepancies:** POV demos contain local client inputs and console commands; HLTV demos contain director camera slots and network broadcasting data. Timestamp and event parsing must be validated separately for both architectures.
* **Demo Type Heuristics:** Guessing demo type based on filename is unreliable. Metadata should be persisted (e.g., in a `.dod-tools-cache.json` or database) using file modification time as a fingerprint to avoid visual flipping in the UI explorer.
* **HLTV Proxy Name:** HLTV proxies broadcast their identity via `SvcUpdateUserInfo` messages. These must be filtered from roster lists as they are not human players.
* **Decal Engine Flush:** To wipe blood/bullet decals between recordings without a full map reload, one must use the command sequence `host_framerate 0; r_decals 0; r_decals 5555`.

### HLAE Execution Context
* **HLAE Output Commands (Hallucination Warning):** GoldSrc HLAE does *not* support modern CS:GO commands (e.g., `mirv_streams`). Frame exports must be driven strictly via `+mirv_movie_filename <dir>` and `+mirv_movie_separate_hud <0|1>`. Use `playdemo <name>` instead of `demo_gototick` (GoldSrc cannot scrub backwards or jump arbitrarily). Use `cam_track 1` instead of `spec_player`.
* **HLAE Initialization Context:** `hlae.exe` will silently fail to inject `AfxHookGoldSrc.dll` if it cannot resolve its internal dependencies. You must explicitly chain `.current_dir(hlae_folder_path)` to the `std::process::Command` builder before invoking `.spawn()`.
* **Semantic Engine Terminology:** The execution engine relies entirely on HLAE injection for features like `mirv_movie_filename`. Do not rename capture architecture to "Native", as this breaks the execution context and abandons necessary hook parameters like `-hookDllPath` and `-programPath`.
* **Engine Timing & `host_framerate`:** In GoldSrc, `host_framerate` defines **frame time** (virtual demo time advanced per physical frame rendered), not frames-per-second.
  * `host_framerate 0` (Dynamic / Real-time): Disables fixed frame pacing. The engine uses the system clock to advance the demo, resulting in exactly **1.0x normal speed** (though it may uncap rendering FPS).
  * `host_framerate 0.01` (Fixed Normal Speed): Forces exactly 0.01 seconds per frame. At 100 FPS, this equals **1.0x normal speed**.
  * `host_framerate 0.2` (Fast Forward): Forces 0.2 seconds per frame. At 100 FPS, this is **20x speed**.
  * `host_framerate 1.0` (Insane Fast Forward): Forces 1 second per frame. At 100 FPS, this is **100x speed**.
  * *(Reference: [GoldSrc Engine Physics & Frame Rate](https://www.jwchong.com/hl/game.html#frame-rate))*
