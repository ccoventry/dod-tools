---
trigger: always_on
---

# 🧠 AI Context: dod-tools & HLCR

## 👋 New AI Onboarding: Start Here
Welcome to the `dod-tools` and HLCR (Half-Life Clip Renderer) workspace! 
This document is your **Primary Source of Truth** for the project's architecture, domain logic, and strict constraints. It is *not* a changelog or a task list (refer to `local/backlog.md` for active tasks).

**Your Expectations:**
1. **Read this file first:** Before proposing complex architectural changes or binary patcher logic, consult the constraints listed here.
2. **Respect the Engine:** Half-Life/GoldSrc is a 1998 engine. It has extreme quirks (468-byte alignments, missing memory bounds, silent truncations). Do not try to apply "modern" file-handling assumptions to the `.dem` binary parser.
3. **Respect the Seams:** This app relies on strict thread isolation to keep the `egui` interface running at 60fps. Never block the main UI thread.

---

## 🎯 Project Overview
`dod-tools` is a high-performance utility suite for parsing, analyzing, and manipulating Half-Life/Day of Defeat 1.3 demo files (`.dem`). It includes a rich graphical dashboard for match analytics, a background rendering queue for movie makers (HLCR), and command-line interfaces for batch processing. 

The application is built for high-performance desktop environments but is architected to compile safely to WebAssembly (WASM) for browser usage where applicable.

---

## 🛠️ Technology Stack & Dependencies
* **Core Language:** Rust
* **GUI Framework:** `egui`, `eframe`, `egui_extras`, `egui_plot`, `egui-file-dialog`
* **Parsing:** `nom` (binary-parsing combinators), `dem` (locally patched via `[patch.crates-io]` pointing to `dem-patch`).
* **Concurrency & Timing:** `tokio` (native runtime scheduler), `web-time` (WASM-compatible timing).
* **State Management:** `serde` / `serde_json` for serialization/caching.
* **Scripting/Automation:** Python (HLCR external process sequencers).

---

## 📁 Project Structure & The Architectural Seams
* **`dod/` (Low-Level Parser):** Parses raw protocol messages. Models `UserMessage`s (e.g., RoundState, ScoreInfoLong), team classifications, and `nom` byte-level parsers.
* **`analysis/` (Metrics & Analytics):** Coordinates mapping frame events to state. Contains specialized analyzers (`chat.rs`, `round.rs`, `scoreboard.rs`, `time.rs`) and manages translation mechanisms.
* **`native/` (Interface):** Handles GUI rendering (`main.rs`, `views/`) and native file/CLI management (`explorer.rs`).
* **`HLCR` (Half-Life Clip Renderer):** A module driving HLAE BMP frame sequences and audio exports through FFmpeg (ProRes, H.264, DNxHR).

### 🧵 The Threading Boundaries (Critical Hand-offs)
* **The UI Thread (`views/capture/mod.rs`):** Renders the `egui` interface. **Never** hold locks across asynchronous boundaries or during a render pass.
* **The Patcher Thread (`patch/engine.rs` & `patch_worker`):** Spawned via `std::thread::Builder`. Performs heavy binary I/O. Communicates back to the UI strictly via an `mpsc::Sender<GuiMessage>`.
* **The Capture Thread (`capture_engine.rs`):** Executes external `std::process::Command` calls to HLAE. Uses a polling loop to monitor an `Arc<AtomicBool>` cancellation token to safely `kill()` the child process if the user aborts.

---

## 🌊 Data Flow Lifecycle
How a demo moves through the system:
1. **Raw Bytes (`.dem`):** Loaded from disk or browser memory.
2. **`dod` Parser:** `nom` parses headers, directory entries, and discrete network messages (filtering out unneeded audio/sprite bytes via `is_relevant_message`).
3. **`analysis` Engine:** Consumes `dod` events to build `AnalyzerState` (rounds, scores, chat) and player-specific `PovStats`.
4. **Serialization:** `DemoInfo` and `AnalyzerState` are combined into `Analysis` and cached to disk as JSON.
5. **Capture/Patching:** The user selects life-bounded killstreaks. The `patch/engine.rs` rewrites the `.dem` binary, injecting `playdemo` and `startmovie` console commands exactly where needed.
6. **HLAE Execution:** `capture_engine.rs` pilots Half-Life to render the patched `.dem` into raw BMPs/WAVs.

---

## 📐 Architectural Rules
* **UI Threading:** All heavy tasks (binary patching, HLAE execution) MUST run on background OS threads using `std::thread` and communicate via `mpsc` channels to prevent `egui` blocking. *[Boundary: `views/capture/mod.rs` <-> `patch_worker`]*
* **State Interruption:** The capture engine must remain fully interruptible via an `Arc<AtomicBool>` cancellation token, using polling loops to safely `kill()` the active HLAE process if the user cancels mid-batch. *[Defined in: `capture_engine.rs`]*
* **Process Execution Context:** Anchor `std::process::Command` for `hlae.exe` strictly to `.current_dir(hlae_folder_path)` so its local dependencies (like `AfxHookGoldSrc.dll`) initialize correctly, preventing silent spawn failures.
* **File Handle Safety:** Always aggressively drop `File` handles, invoke explicit `sync_all()` drops, and use manual memory streams with `FILE_SHARE_READ` (`.share_mode(1)`) on Windows to prevent `os error 32` sharing violations during demo I/O.
* **Grouping Modes & Segmentation:** Grouping modes are deprecated; the hierarchy is strictly flat (`Demo File -> Table of Streaks`). Killstreaks are strictly life-bounded (ended by `DeathMsg` or Round Restart), avoiding arbitrary temporal-gap logic.

---

## ☠️ Strict Anti-Patterns (What NOT to do)
* **Never block the UI Thread:** Do not put `std::fs::read` or heavy loops inside an `egui` `update()` loop.
* **Never use `.unwrap()` in the binary parser:** The `dod` parser MUST handle malformed/corrupted `.dem` files gracefully. A panic in a background thread will silently kill the capture queue. Always use `Result` and log skipping.
* **Never hold `Arc<Mutex>` locks globally:** Clone the state you need under a micro-lock, drop the lock, and then perform your heavy rendering or calculations.

---

## 🏗️ Target Architecture Constraints
The codebase safely compiles to both `x86_64-pc-windows-msvc` and `wasm32-unknown-unknown`.
* **Native (`#[cfg(not(target_arch = "wasm32"))]`):** Has access to standard background multi-threading, direct file system exploration (`std::fs`), and external process spawning (`std::process::Command`).
* **WASM (`#[cfg(target_arch = "wasm32")]`):** Completely sandboxed. Relies entirely on web-safe callbacks, browser canvas setups, drag-and-drop file inputs, and in-memory byte buffers. Disk-based localizations are statically embedded via compiler macros.
* **Rule:** Any feature requiring file scanning (e.g., HLCR) or external tools must be strictly gated using conditional compilation.

---

## 🎮 Domain Logic: GoldSrc & DoD 1.3
* **Optimized Parsing:** The parser executes `is_relevant_message` on incoming message names, discarding non-essential frames (e.g., sounds, effects) to optimize sequential iteration.
* **UserMessage Decoding:** Network messages (e.g., `DeathMsg`) are matched against names and parsed with custom structures inside `UserMessage::new`. Null-terminated strings utilize a dedicated `null_string` combinator.
* **Clan Match Logic:** Automatic competitive validation watches for distinct round sequences (Reset -> Start transitions), `mp_clan_timer` countdown offsets, and reinforcement wave lengths to mark a match "live."
* **British Faction Mapping:** Inspects selected classes (e.g., BritishRifleman). Upon positive detection, it dynamically upgrades Allies entities, scoreboards, and chat logs to reference the British faction.
* **Reconnect Stat Wipes:** DoD 1.3 servers reset a player's score to 0 upon reconnecting. Accurate aggregation requires tracking individual `DeathMsg` deltas across disconnect boundaries.

---

## 🛑 GoldSrc Engine Constraints
* **Memory Buffer Initialization:** GoldSrc console commands (e.g., `host_framerate`, `startmovie`) MUST be injected AFTER the DemoStart (Type 2) frame to prevent uninitialized memory buffer crashes.
* **Directory Offset Patching:** Any modification to demo frame counts (injected commands) requires a manual binary patch to the `frame_count` and `file_length` integers in the Demo Directory at the end of the file, and a physical shift of the start offset for all subsequent directory entries.
* **Filename Truncation:** `playdemo` filenames must be < 40 characters to avoid silent truncation by the 1998 engine parser.
* **Strict 468-Byte NetworkMessage Alignment:** The GoldSrc binary parser expects exactly 468 bytes of header data before reading the payload length for `NetworkMessage` frames. Do not alter this alignment.

### 🛑 Anti-Hallucination Dictionary (GoldSrc vs Source/CS:GO)
AIs frequently hallucinate modern Source engine HLAE commands. STRICTLY enforce these mappings:
* **BAD:** `mirv_streams` -> **GOOD:** `mirv_movie_filename`
* **BAD:** `demo_gototick` -> **GOOD:** `playdemo <name>` (GoldSrc cannot scrub backwards or jump arbitrarily)
* **BAD:** `spec_player` -> **GOOD:** `cam_track 1`

---

## 🤖 AI Coding Directives
When generating code or proposing solutions for this project, adhere strictly to these rules:
1. **Clear, Point-Form Instructions:** Provide step-by-step logic and pseudo-code before generating large blocks of syntax.
2. **No Hallucinations:** Use only standard library functions or the explicitly listed crates. Do not invent UI widgets or third-party wrappers. 
3. **Lock-Free Preferences:** Minimize `Mutex` locking on the main thread to prevent UI micro-stutters. Prefer `RwLock`, `mpsc` messaging, or localized state cloning.
4. **Scannability:** Format responses with clean markdown, minimal nested lists, and clear headers. Use tables for multi-variable comparisons.

---

## 📝 Draft Rules (Pending Review)
*If you uncover a new rule, bug, or workaround during a session, log it in `draft_rules.md`. The user will periodically review this section and promote validated rules to the permanent architecture sections above.*

**Rule Review Protocol:**
When the user asks to "review draft rules", you must act as a critical sounding board. Do not simply agree to promote everything in `draft_rules.md`.
1. **Analyze:** Evaluate each drafted rule against existing knowledge in `goldsrc-quirks.md`, `rust-architecture-constraints.md`, and `agent-protocols.md`.
2. **Deduplicate:** Flag any rules that are already covered or are just re-phrasings of existing constraints.
3. **Filter:** Point out rules that might just be one-off flukes or trivial syntax fixes rather than genuine systemic constraints.
4. **Migrate:** Help the user refine the phrasing of the valuable rules and suggest exactly which permanent rule file and category they belong in.