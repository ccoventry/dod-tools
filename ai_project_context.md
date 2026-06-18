# 🧠 AI Context: dod-tools & HLCR

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

## 📁 Project Structure
* **`dod/` (Low-Level Parser):** Parses raw protocol messages. Models `UserMessage`s (e.g., RoundState, ScoreInfoLong), team classifications, and `nom` byte-level parsers.
* **`analysis/` (Metrics & Analytics):** Coordinates mapping frame events to state. Contains specialized analyzers (`chat.rs`, `round.rs`, `scoreboard.rs`, `time.rs`) and manages translation mechanisms.
* **`native/` (Interface):** Handles GUI rendering (`main.rs`, `views/`) and native file/CLI management (`explorer.rs`).
* **`HLCR` (Half-Life Clip Renderer):** A module driving HLAE BMP frame sequences and audio exports through FFmpeg (ProRes, H.264, DNxHR).

---

## 🧱 Core Data Models
* **`AnalyzerState`:** The primary orchestrator. Tracks active players, rounds, team scores, chat logs, server connection metadata, and game timing.
* **`Player`:** Manages participant identities, mapping Steam IDs, class/team history, connection state, and lifetime metrics.
* **`PovStats`:** Captures stats from the recording player's perspective (zoom states, damage taken, suicides, and per-weapon details like scopes vs. noscopes).
* **`DemoInfo`:** Summarizes physical header info (POV vs. HLTV, map names, lengths, directory definitions).
* **`Analysis`:** Combines `DemoInfo` and `AnalyzerState` into a single serializable object.

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

## 🤖 AI Coding Directives
When generating code or proposing solutions for this project, adhere strictly to these rules:
1. **Clear, Point-Form Instructions:** Provide step-by-step logic and pseudo-code before generating large blocks of syntax.
2. **No Hallucinations:** Use only standard library functions or the explicitly listed crates. Do not invent UI widgets or third-party wrappers. 
3. **Lock-Free Preferences:** Minimize `Mutex` locking on the main thread to prevent UI micro-stutters. Prefer `RwLock`, `mpsc` messaging, or localized state cloning.
4. **Scannability:** Format responses with clean markdown, minimal nested lists, and clear headers. Use tables for multi-variable comparisons.