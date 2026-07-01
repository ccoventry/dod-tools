
---
# File: .cursorrules

Always reference the .cursorrules file in the workspace root for behavioral rules and token efficiency. Use the docs/ folder for modular project knowledge; do not read documentation files unless explicitly needed for the current task. Always provide code as minimal diffs/blocks.

# Global Agent Instructions

1. Authority
   - This file defines global behavioral rules for the entire workspace.
   - Local instructions may extend these rules but must not contradict them.
   - If rules conflict, this file takes precedence.

2. Context Boundaries
   - Assume no knowledge outside the current workspace.
   - The /docs folder contains canonical project context.
   - Do NOT read documentation files unless:
     a) The task explicitly requires it, or
     b) Skipping them would risk incorrect output.

3. Output Discipline
   - Be concise and high-signal by default.
   - Do not restate requirements.
   - Do not summarize unless explicitly requested.

4. Code Changes
   - Always provide minimal diffs or isolated code blocks.
   - Do not rewrite entire files unless explicitly asked.
   - Do not introduce new abstractions, patterns, or dependencies unless required by the task.

5. Assumptions & Ambiguity
   - Do not guess silently.
   - If an assumption materially affects behavior, state it once and proceed.
   - Do not ask clarifying questions unless ambiguity blocks progress.

6. Project Integrity
   - Do not modify:
     - Build or deployment configuration
     - Environment files
     - Formatting or linting rules
     - Public APIs
     unless explicitly requested.

7. Consistency
   - Match existing naming, structure, and style exactly.
   - Do not introduce new conventions unless necessary.

8. Quality Bar
   - Prefer clarity over cleverness.
   - Prefer explicit logic over magic behavior.
   - Every change must directly serve the task.

9. Failure Behavior
   - Fail loudly rather than silently.
   - Avoid hidden fallbacks or implicit defaults.

10. Outcome Priority
    - Correctness first
    - Maintainability second
    - Performance only when relevant

WASM Constraint: The GUI compiles to wasm32-unknown-unknown. Never import standard file I/O (std::fs) or native threading crates inside UI modules without explicit conditional gating: #[cfg(not(target_arch = "wasm32"))].
Lock-Free Layouts: Treat existing atomic debouncers, RwLock architectures, and channel structures as immutable patterns. Avoid introducing blocking mutexes on the UI thread to ensure zero micro-stutters.

---
# File: docs/ai_rules/ai_architecture_protocols.md

# 🧠 Web AI Architecture & Planning Protocols

## 1. System Role & Workflow
You are the Lead Architect and "Brains" for the `dod-tools` project (a high-performance Rust suite for parsing Half-Life/Day of Defeat 1.3 demo files). I am developing this using Antigravity IDE with an integrated AI (the "Hands"), which I refer to as "IDE", or "IDE AI".

Your job is NOT to write raw code for me to copy-paste. Your job is to:
1. Brainstorm solutions, map architecture, and analyze constraints.
2. Run S-Tier Diagnostics on my ideas and the IDE AI's proposed changes.
3. Generate highly optimized, blob-proof, step-by-step prompts for me to feed to the IDE AI.
- **Proactive Context Lookup:** If the user proposes a task that requires specific architectural, domain, or conventional knowledge not found in `bootstrap_payload.md`, I must reference the project tree mapped in README.md and explicitly ask the user for the specific missing file from the docs/ folder before proceeding.

## 2. The S-Tier Diagnostic Framework
Analyze the preceding response through a multi-dimensional evaluation framework that measures both technical excellence and user-centered effectiveness. Begin with a rapid dual-perspective assessment that examines the response simultaneously from the requestor's viewpoint and from quality assurance standards.

Next, conduct a structured diagnostic across five critical dimensions:
- **Alignment Precision**
- **Information Architecture**
- **Accuracy & Completeness**
- **Cognitive Accessibility**
- **Actionability & Impact**

Synthesize your findings into three focused sections:
*   **Execution Strengths:** 1-2 bullet points highlighting what works well and aligns with the project goals.
*   **Refinement Opportunities:** 1-2 bullet points identifying flaws, edge cases, inefficiencies, or lock-contention risks.
*   **Precision Adjustments:** 2-3 concrete, implementable steps to fix the flaws.
*   **Critical Priority Flag:** The single most important improvement that must be addressed immediately.

## 3. IDE AI Prompt Generation
After providing your diagnostic, you must generate the prompt I will feed to the IDE AI.
*   The prompt must be enclosed in a single, continuous standard markdown block.
*   It must use explicit, alphanumeric step labels (e.g., `[STEP 1]`, `(1a)`, `(1b)`) instead of markdown bullets to survive IDE text box formatting.
*   It must instruct the IDE AI strictly on *what* to do and *where* to do it, enforcing lock-free preferences, Rust 2024 idioms, and the "Blind Architect" protocol where applicable.
*   Never use nested triple backticks (e.g., ```rust) inside the main markdown block. To provide code snippets within the prompt, use a 4-space indentation or standard blockquotes (>) to ensure the outer markdown block remains intact for one-click copying.
- **The Execution Ban:** The final step of EVERY prompt must be titled "Execution Ban Reminder". It must explicitly forbid the IDE AI from independently running `cargo check`, `cargo run`, or terminal search commands (like `grep`), instructing it only to report when file edits are complete.
- **Surgical Scope Splitting:** Complex feature requests must be chronologically chunked (e.g., Structs -> Engine Logic -> UI Blueprint) to prevent the IDE AI from getting trapped in multi-file context tracking loops.
- **Token Protection Generation:** Prompts built for the IDE AI must wrap internal code block snippets in a 4-space indent or standard blockquotes rather than nested markdown backticks, protecting the outer markdown block from structural parsing failures.
- **Anti-Tunnel Vision Prompts:** Generation sequences must explicitly anchor task targets to specific existing modules (e.g., pointing directly to established definitions in the analysis crate) to prevent the IDE agent from inventing redundant systems.
- **The Scope Guardian Halt:** If a conversation sequence forces a sudden pivot between disparate workspace domains, immediately halt generation and force a context transfer or new chat window to prevent short-term memory rot.
- **Prompt Titling:** Every prompt generated for the IDE AI must include a clear, descriptive title (e.g., `### 📝 Prompt: Feature X Implementation`). This title MUST be placed *inside* the markdown code block so it is captured automatically when the user clicks 'Copy'.
- **Model Recommendation:** For every generated prompt, explicitly recommend the optimal IDE AI model. Default to `Gemini 3.1 Pro (High)` or `Gemini 3.5 Flash` for standard coding, structural refactors, and file manipulation. Reserve premium models like `Claude Sonnet 4.6 (Thinking)` strictly for highly complex, algorithmic, or deep-reasoning architectural shifts to conserve the user's weekly quota.

## 4. Context Handoff Protocol
Whenever the user types `Initiate Context Handoff`, `wrap up`, or `session over`, you must immediately halt execution and generate a dual-part exit package:

**Part 1: 🧠 Session Lessons Learned (Knowledge Extraction)**
You must act as a forensic knowledge extractor. Scan the entire conversation history of THIS chat:
- Ignore standard syntax or UI changes that worked perfectly. Focus EXCLUSIVELY on "gotchas" (e.g., engine quirks, threading deadlocks, hallucinated commands).
- If we encountered an error and fixed it, extract the underlying rule that prevents that error.
- Capture any workflow friction points: if the AI read too many tokens, hallucinated a tool, or misunderstood a command, extract the protocol needed to prevent it in the future.
- **The Null-Harvest Guardrail:** If the session went perfectly smoothly and no new unique gotchas, engine quirks, or workflow friction points were discovered, you are strictly forbidden from inventing rules or duplicating existing entries from documentation. Instead, you must explicitly output: *"Knowledge Extraction: No new lessons or engine quirks discovered in this session."*
Output the results as a ready-to-copy **IDE AI Prompt** formatted in a single markdown code block. This prompt must provide the exact harvested rules (grouped by `GoldSrc/HLAE Engine Quirks`, `Rust/Architecture Constraints`, etc.) and explicitly instruct the IDE AI to append them to the appropriate documentation files (like `docs/session_lessons.md` or `local/draft_rules.md`). Do not assume you have file editing tools; you must provide the prompt for the user to pass to the IDE.

**Part 2: 📦 Context Payload (State Transfer)**
Output a ready-to-copy IDE AI Prompt containing your minified state payload. Instruct the IDE AI to overwrite the `## Web AI State` section of `docs/active_context.md` with your payload.
The minified payload MUST include:
- The current overarching goal.
- The specific crate, file, and function last edited.
- Any unresolved compiler errors or bugs.

**Final User Action:** After generating the IDE AI Prompt, you must output a completely separate, standalone markdown code block containing exactly: `.\build_bootstrap.ps1`. Explicitly instruct the user to click and run this command in their terminal *after* the IDE AI has successfully saved its state, guaranteeing the `bootstrap_payload.md` is freshly compiled for the next session.

**Part 3: The IDE AI State Write**
When executing the Web AI's Context Handoff prompt, the IDE AI must also evaluate its own immediate state and overwrite the `## IDE AI State` section of `docs/active_context.md` with its own minified payload (including the exact next terminal command to execute or file to edit).

## 5. Web AI Bootstrapping Protocol
When the user starts a fresh chat window with you (the Web AI), they must provide the unified `bootstrap_payload.md` file to establish your context map. Do not ask for the fragmented source documentation files individually.

---
# File: docs/ai_rules/ai_execution_protocols.md

# 🛠️ AI Execution & IDE Protocols

## 1. IDE File Editing & Code Spills
- **No Code Spills:** Only rewrite the specific lines or functions requiring modifications. Do not drop, modify, or aggressively reformat unrelated logic blocks within the same file.
- **File Editing:** Do not output manual diffs, `@@` syntax, or full file reprints in the chat for standard project code. **EXCEPTION:** If you edit a file inside the `.agents\rules\` directory (or similar config), you MUST output a markdown `diff` block in the chat showing exactly what lines you added/removed, because the IDE does not provide visual diffs for these configuration files.
- **Edit Scope:** Restrict file edits strictly to the lines requiring changes. Do not reformat or overwrite untouched functions in the same tool call.
- **Workspace Search Exclusions:** System searches and file queries are strictly prohibited from indexing the target/ directory, the demos/ folder, and the Cargo.lock file. Massive asset maps inside the localizations/ folder must never be read in full; utilize targeted, line-clamped lookups (e.g., grep -m 5).
- **The Table Delete-and-Recreate Rule:** When moving items across layout-critical markdown tables, you must delete the source row entirely before generating the destination row to prevent text string fragments and visual duplication.

## 2. Documentation & Milestones Upkeep
- **Autonomous Upkeep:** When a major feature is verified as working, or when executing a Context Handoff, you must autonomously update `docs/milestones.md` to reflect the new project state.
- **Append and Shift (Never Erase):** When updating task lists or milestones, you may move items from "Active" to "Completed" sections, but you are strictly forbidden from deleting completed tasks, historical data, or future unassigned backlog items. 
- **Format Preservation:** You must perfectly mirror the existing markdown structures in the file (e.g., maintaining `[x]` and `[ ]` checkbox syntax, nested bullet points, or table layouts). Do not reformat the entire document; only modify the targeted lines.

## 3. Terminal Output & `tuf` Pipeline
- **Terminal Errors:** When running terminal commands, do not echo the raw compiler output back to the chat. Provide a one-sentence summary of the failure and immediately propose the fix.
- **Strict Execution Ban:** DO NOT use your internal terminal tools to run `cargo check`, `cargo build`, or any compilation commands. You must output the exact command in a markdown block, append ` 2>&1 | tuf` to it, and instruct the user to run it manually.
- ONLY when a command is meant to capture complex logs, compiler errors, or diagnostics for you to analyze, format it as: `Your-Command 2>&1 | tuf`.
- For simple administrative or deployment actions that do not require your review (e.g., git push, git commit, cd, mkdir), provide the plain standard command directly without piping to `tuf`.
- When a `tuf` command is used, instruct me: "Run the command. Once it finishes, type 'done' and I will automatically analyze the newest file generated in your scratch directory."
- **Strict File Ingestion:** On the turn immediately following a `tuf` execution confirmation, look inside the `scratch/` folder, sort by creation date, and pull the single newest `output_*.txt` file. **Crucial Limit:** Do NOT read the entire file if it is a compiler error. You must explicitly limit your read to the final 30 lines, or specifically grep/extract only the blocks containing the string `error[`.
- Always format terminal or PowerShell commands in a standard Markdown code block (e.g. using triple backticks) so they can be easily copied or run. For administrative/deployment actions that the user will execute via the paste/run button, always format them as a single-line command (using semicolons `;` to chain them if necessary).
- **Terminal Integration Single-Line Rule:** Multi-step sequences or task pipelines executed via terminal commands must be collapsed into a single, chained line (using semicolons `;` as separators in PowerShell) to ensure one-click execution capability for the user.
- **The Single-Line Terminal Mandate:** Multi-step shell actions or sequential sequences must be collapsed into a single, chained sequence using semicolons (;) to enable safe, single-click copy-and-run execution in the environment.
- **Dry-Run Pipeline Default:** Configurations targeting batch processing workflows must implement and verify a clean dry-run mode (printing calculated targets to screen) before enabling live external process executions.

## 4. Internal Reasoning & Protocol
- **Internal Reasoning:** Keep internal CoT (Chain of Thought) focused exclusively on technical logic. Skip all conversational filler, apologies, and summary introductions.

## 5. Context Handoff Execution
The Web AI generates the Context Handoff prompt. When the user pastes it into the IDE, your sole responsibility is to execute it:
1. Append any harvested quirks directly to the target knowledge files as requested. *Condition:* If the Web AI triggers the Null-Harvest Guardrail (stating no new lessons were discovered), and you have no unique local IDE observations to add, skip this file-appending step entirely to prevent duplication.
2. Overwrite the `## Web AI State` section of `docs/active_context.md` with the Web AI's minified payload.
3. Evaluate your own immediate state and overwrite the `## IDE AI State` section of `docs/active_context.md` with your own minified payload (including the exact next terminal command to execute or file to edit).

## 6. IDE AI Bootstrapping Protocol
At the beginning of any new chat session or complex task, you must immediately read `bootstrap_payload.md` to establish the overarching project constraints, engine quirks, and active state. Do not attempt to read the fragmented source documentation files individually unless specifically instructed, as the payload contains the optimized, authoritative state.

---
# File: docs/architecture.md

# 🏗️ Rust Architecture & UI Constraints

## 🖥️ Immediate-Mode UI & egui Performance
- **Zero-Allocation Render Loops:** Do not execute `format!()`, string concatenation, or heavy `.clone()` operations inside the core layout or `update()` loop. Pre-calculate complex text layouts in background workers and expose them via static references to prevent frame drops.
- **Anti-Recursion Virtualization:** Never nest dynamically expanding elements (such as `CollapsingHeader`) inside virtualized row structures (`show_rows`). Complex lists of structured records must strictly utilize flat `egui_extras::TableBuilder` hierarchies to prevent 1MB Windows stack overflows (`0xc0000409`).
- **UI-Level Data Ingestion:** Background ingestion parsers must never filter or drop valid data streams to save space. Retain all states cleanly in memory and execute player visibility selections or item filters exclusively at the visual UI layer to guarantee layout safety.
- **Lock-Free Read Parity:** Highly accessed configuration catalogs or translation asset maps must be wrapped in `std::sync::RwLock` to enable concurrent multi-reader access without causing main thread lock contention stutters.

## 🧵 Threading, Concurrency & Telemetry
- **Main Thread Protection:** All long-running disk I/O, binary stream patches, and process management routines are strictly barred from the main UI thread. Offload execution to dedicated background threads using `std::thread::Builder`.
- **Throttled Progress Telemetry:** Background threads reporting progress percentages must debounce updates using a lock-free `Arc<AtomicU32>` tracking framework to throttle channel traffic to ~30fps (33ms), preventing the event loop from flooding while calling `ctx.request_repaint()`.
- **Eager Control Flags:** Toggle active UI state flags (e.g., `capture_engine_running = true`) to true *prior* to spawning threads. Enforce strict layout gating using `ui.add_enabled(!state)` to block double-launch actions.

## 💾 Process Lifecycles & Memory Safety
- **Interruptible Process Management:** Avoid using blocking `child.wait()` statements on external tasks. Use a polling infrastructure executing `child.try_wait()` matched with a ~16ms cadence thread sleep, verifying a shared `Arc<AtomicBool>` cancellation token on every cycle to safely execute `child.kill()`.
- **Headless Process Cleansing:** External process execution wrappers targeting `hlae.exe` or `ffmpeg.exe` must explicitly chain `.kill_on_drop(true)` to guarantee zombie subprocesses are reaped instantly if the parent application closes.
- **Heap-Allocated Parsing Safeguards:** Binary stream slicing operations are prohibited from using fixed-size stack buffers (`[u8; N]`), which risk instant memory failures. All parsing targets must use heap-allocated `Vec<u8>` gated by explicit 2MB payload limits checked right after reading size metrics.
- **Defensive CWD Pathing:** Never execute configuration tracking against relative text filenames. Explicitly bind adjacent configuration parameters to absolute paths derived from `std::env::current_exe()`.
- **Target WASM Segregation:** The graphical interface target compiles to `wasm32-unknown-unknown`. Isolate all native multi-threading, direct file system structures (`std::fs`), and external commands (`std::process::Command`) behind strict `#[cfg(not(target_arch = "wasm32"))]` compilation macros.

---
# File: docs/domain_quirks.md

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

---
# File: docs/milestones.md

# 📋 Project Backlog & Future Improvements

## 1. Recently Completed Tasks

### Capture Engine Timeline Stabilization (July 1, 2026)
* **Time-Aware 0-Indexed Bounds:** Resurrected `find_tick_backwards` and `find_tick_forwards` to bypass map change jumps, calculating exact bounds via float timestamps while returning engine-safe 0-indexed frame integers.
* **Payload Truncation:** Extracted logging strings into a `LOG_TAG` constant (`[dod]`) and aggressively abbreviated diagnostic payloads to prevent 64-byte `Cbuf_AddTextToBuffer` overflow corruption.
* **EOF Safety Buffer & Audio Guards:** Clamped all sequence termination commands within a strict 3.0-second margin from the demo's end to prevent early engine drop, and added lookahead guards to prevent fast-forwarding over overlapping audio-flush windows.

### I/O Optimization & Capture Engine Fixes
* **Logging Refactor (I/O Bottlenecks):** Replaced `println!` and `eprintln!` with `log_markdown` in `capture_engine.rs` and `views/capture/mod.rs` to eliminate disk I/O bottlenecks and parsing log spam during demo scanning.
* **Capture Configuration Logging:** Added exact configuration payload logging `[CAPTURE CONFIG PAYLOAD]` to the session log in `select.rs` before building batch queues.
* **Engine Speed Injection:** Updated timeline logic in `builder.rs` to validate `tickrate` (via `safe_tickrate` fallback) and explicitly inject the fast-forward command (`host_framerate {speed}`) at tick 0 before iterating over streaks, resolving an issue where the engine recorded the entire demo at normal speed.
* **Capture UI Diagnostics:** Added diagnostic debug prints to button state and payload generation in `views/capture/select.rs` to isolate UI state locks.

### AI Workflow & Knowledge Harvester Automation
* **Knowledge Extraction:** Harvester prompt expanded to explicitly target `Agent Protocols & Workflow Optimizations` to prevent recurring AI friction points.
* **Automated Harvester Pipeline:** Modified the `Chat Closure Protocol` in `agent-protocols.md` so the AI automatically appends harvested rules to `draft_rules.md` instead of just printing them to the chat.
* **Rule Review Protocol:** Added strict instructions to `project_context.md` forcing the AI to act as a critical sounding board (Analyze, Deduplicate, Filter, Migrate) when reviewing `draft_rules.md`.
* **Rule Modularization:** Extracted monolithic draft rules into distinct, targeted semantic files (`goldsrc-quirks.md` and `rust-architecture-constraints.md`).

### Memory Optimization & Patcher Robustness
* **Directory Offset Shift Logic:** Fixed directory offset corruption by mapping injected bytes to their specific physical segments, expanding `file_length` accurately, and preventing 468-byte `NetworkMessage` alignment loss (which was triggering `MAX_POSSIBLE_MSG` crashes).
* **Robust Crash Logging:** Refactored `log_crash_abort!` to dynamically resolve `crash_log.md` relative to the executable path and auto-create the `local/` directory.
* **Debug Tools:** Added frame header debug logging to `write_console_cmd` and gated temporary file cleanup behind `cfg(not(debug_assertions))` for hex analysis.
* **Memory Optimization:** Migrated to a flat `egui_extras::TableBuilder` hierarchy per demo.
* **Parser Desync Fix:** Fixed `NetworkMessage` alignment to 468 bytes.
* **Decoupled Patcher & Telemetry:** Threaded the batch patcher, added f32 progress, and 2MB safety caps.
* **Timeline Pre-calculation:** Built `K98, (+0:15) Luger` strings in background via absolute frame timestamps.
* **UI-Level POV Filtering:** Added UI toggle for non-recording players to retain safe parser data.

### Phase 12.5: Capture Pipeline Stabilization & Rendering Upgrades (Completed)
* **Rendering Enhancements:** Implemented HLCR codec selection, NVENC hardware acceleration, and alpha routing.
* **Drive Failover:** Implemented dynamic multi-demo drive failover to handle storage capacity issues mid-batch.
* **Quality of Life & Resilience:** Added path collision checks, an AV (Antivirus) file-lock retry loop, configuration persistence, and folder picker bookmarks.
* **Diagnostics:** Expanded debug tools and enhanced crash logging specifically for the capture engine.

### Phase 12: Continuous Batch Processing (Completed)
#### Daisy-Chain Architecture
* **Continuous Batch Processing:** GoldSrc ignores `quit` commands in demo streams. To solve this and reduce process launch overhead, the engine will process demos in a single continuous batch without closing the game.
* **Stateful Patcher Routing:** The patcher must accept `next_demo_filename` as state.
* **Chaining Command:** If a next demo exists, inject `mirv_recordmovie_stop; playdemo <next_demo>`.
* **Completion Command:** If it is the last demo, inject `mirv_recordmovie_stop; disconnect; clear; echo "[dod-tools] BATCH COMPLETE"`.
* **40-Character Truncation Limit:** Patched `.dem` filenames must be strictly truncated or hashed before chaining so the 1998 engine buffer doesn't clip them.

#### Primer Demo Strategy (Lighting Fix)
* **First-Load Black Map Bug:** To fix the GoldSrc first-load black map bug, the UI/engine must duplicate the first demo in the batch and save it as `primer_ptch.dem`.
* **Asset Pre-Caching:** The patcher will strip all killstreaks from `primer_ptch.dem` and inject `playdemo <first_real_demo>` immediately after the `DemoStart` frame to pre-cache map assets.
* **Capture Engine Startup:** The capture engine will launch HLAE strictly using `+playdemo primer_ptch`.

### Phase 11: Capture Integration (Completed Items)
* **Target Directory Selector:** Implemented a UI directory picker to define where raw BMP/WAV takes are saved. Routed off the OS drive.
* **Disk Space Pre-Flight Check:** Integrated `sysinfo` check before launching engine to halt if target drive has < 15GB free space.
* **Temp Demo Cleanup:** Gated the `std::fs::remove_file` cleanup step inside `capture_engine.rs` behind `cfg(not(debug_assertions))` to automatically delete patched demos in release builds.

### Capture Engine & Engine Quirk Fixes (June 30, 2026)
* **HLAE Dummy Folder Sandbox Escape:** Bypassed GoldSrc's engine block on `exec` commands during playback by using `mirv_movie_filename` to create a dummy directory on the OS, triggering our Rust Reaper to safely terminate the batch.
* **Taskkill Reaper:** Replaced the fragile `child.try_wait()` loop with an aggressive polling loop that explicitly runs `taskkill /F /IM hl.exe` when the dummy directory trigger is detected.
* **Absolute Time-to-Frame Mapping:** Completely purged `demo_fps` average math. Fixed POV timeline drift by implementing a raw binary frame-parsing loop in the scanner to extract exact float timestamps (`Arc<Vec<f32>>`), mapping absolute times to precise frame indices via binary search.
* **UI Resolution & Defensive I/O:** Upgraded the Capture Studio with reactive `width` and `height` resolution settings. Added defensive pathing with `std::fs::create_dir_all` session ID generation, piping I/O errors to the `CaptureState::Error` UI block.
* **Reactive Disk Space Math:** Replaced static disk caching with a dynamic frame size multiplication formula tied to the live resolution and `capture_fps` configuration.

### Capture Engine & Engine Quirk Fixes (June 29, 2026)
* **Command Truncation Fix:** Shortened injected `echo` commands and decoupled `mirv_recordmovie_stop` to respect the strict 64-byte payload limit inside GoldSrc `ConsoleCommand` frames.
* **Local Demo Debug Output:** Added a `save_local_patched_copy` UI toggle to the Capture Studio to easily duplicate patched `.dem` files to the workspace `demos/` directory.
* **GoldSrc Demo `quit` Filter Bypass:** Fixed an issue where the game refused to close after recording because GoldSrc actively drops any `ConsoleCommand` containing the string `quit` during demo playback. Bypassed the security check by scheduling `dodtools_exit` in the `.dem` and mapping it to `+alias dodtools_exit quit` in the engine startup arguments.
* **Frame-based Tick Insertion:** Moved from time-based offsets back to exact `file_tick` mapping for command insertion, ensuring reliable synchronization between highlight bounds and engine rendering frames.

## 2. 🚨 Active Priority — Finalize Game Capture Pipeline

### WIP: Frontend Migration
* **Tauri & Vite Integration:** Ongoing migration of the frontend stack from native `egui` to a Tauri + Vite architecture. (Note: Excluded from primary architecture docs until finalized).

### Upcoming Tasks
1. **Graceful Degradation for Clutch Clips:** Update `builder.rs` to handle edge cases where a kill occurs inside the 3.0-second EOF danger zone. The patcher must gracefully sacrifice the post-roll and schedule the `DOD_TOOLS_EXIT_TRIGGER` exactly 5 ticks before the absolute final frame to ensure the highlight is captured without crashing the batch.
2. **Config Injection:** Ensure the engine dynamically injects `+exec movie.cfg` into the HLAE command line arguments to preserve custom capture framerates and HUD settings.
3. **Long Demo Validation:** Validate capture sequence for 30+ minute demos.
4. **Packet Audit:** Audit `.dem` packet length consistency.
5. **Packet Initialization Rule:** Formalize GoldSrc packet memory initialization rule.
6. **Meta-Task: Standardize Milestones Architecture:** Design and document a strict, immutable markdown layout for this file to completely prevent AI-driven format drift across future sessions.

## 3. Future Tech Debt & Enhancements

### Task B: Unify Killstreak Segmentation (`demo_parser.rs`)
* **Life-Bounded Streaks:** Extracted life-bounded streak definitions from the `analysis` crate. The Capture Studio now natively limits "killstreaks" to events bounded by `DeathMsg` or `ServerReset`, avoiding arbitrary temporal-gap logic that causes recording cross-talk. *(Note: Continue unification)*

### Task D: Architectural Decoupling & File Cleanup
* **Context:** Review the project to decouple logic, abstract UI components into separate modules/files, and clean up deprecated code. This is an ongoing radar item to maintain code health and readability.

### Phase 11 & 12 Enhancements (Deferred)
* **Batch Queue UI Re-integration:** Restore and integrate the `batch_queue_ui` to allow users to queue up multiple demos for continuous, unattended processing.
* **HLTV Parser Upgrade:** Reverse-engineer the GoldSrc HLTV proxy byte structure in `patch.rs` to allow the engine to parse massive server-side demos and generate player-specific queues.
* **Rendering & Finalization:** Handling the resulting `.mov` files.

### Task E: Google Takeout Gemini Logs Parsing (Knowledge Harvest)
* **Implementation:** Determine a programmatic or streamlined method to parse offline chat logs and feed them into the Knowledge Harvester to extract and deduplicate workflow protocols, constraints, and engine quirks from historical Web AI sessions.

## 4. Completed Project Phases (Historical Archive)
<details>
<summary>Click to view past phases (Phases 6-10)</summary>

### Phase 6 (The Ingestion Engine)
* **Multithreading & Memory Safety:** Replaced single-threaded blocking UI with a background worker thread (`std::thread::Builder`) using a 16MB stack. Migrated all fixed-size array byte buffers in `patch.rs` to heap-allocated `Vec<u8>` to permanently resolve `STATUS_STACK_BUFFER_OVERRUN` crashes.
* **Fault Tolerance:** Eliminated all panics (`.unwrap()`) in the binary parser. The engine now safely catches malformed or corrupted `.dem` files, logs a warning, and gracefully skips them without crashing.
* **HLTV Safeguard:** Implemented a header-inspection function (`is_hltv_demo`) to automatically detect and skip HLTV proxy network streams before they break the POV parser.
* **UI Sync:** Added live progress tracking ("Scanning demo X of Y") and disk-based crash logging (`crash_log.txt`) without choking the `egui` render loop.
* **Grouping:** Finished caching models for queue grouping (By Demo, By Player, Flat) to prepare highlights for batch processing.

### Phase 7 (The HLAE Game Capture Engine)
* **Goal:** Built the engine that physically pilots Half-Life Advanced Effects (HLAE) to record the patched demos.
* **Architecture:** Implemented a background worker thread (`capture_engine.rs`) that executes HLAE (`std::process::Command`) with AFX Hook DLLs, waits sequentially, and verifies output recording artifacts.
* **Thread-Safe State & Poison Mitigation:** Upgraded global discovery collections (`QUEUED_DEMOS`, caching layers) from basic `Box` wrappers to thread-safe `Arc<Mutex<Vec<T>>>` patterns with full poison recovery (`PoisonError::into_inner`).
* **UI Reactivity:** Decoupled rendering passes from long-lived locks (cloning state under micro-locks) and implemented a time-and-batch-gated repaint manager (>16ms/5-demo throttled `ctx.request_repaint()`).
* **Semantic Constraint:** Formally re-asserted "HLAE Game Capture" pipeline semantics (specifically hook DLL injection command lines like `-hookDllPath` and `-programPath`), reverting a semantic drift to "Native".

### Phase 8 (Async Patcher & Export Config Restoration)
* **Async Batch Patcher Migration:** Replaced the in-UI synchronous patching call with a dedicated OS thread (`patch_worker`) spawned via `std::thread::Builder`. The thread posts `GuiMessage::PatchingComplete` on the mpsc channel on completion. Main loop handles state transition to `CaptureStudioState::Capture`. UI thread blocking is fully eliminated.
* **IS_PATCHING State Guard:** Added a `static IS_PATCHING: AtomicBool` in `capture_ui.rs` with public `is_patching()` / `set_is_patching()` accessors. Gates the Proceed button and shows a live spinner + label during patching.
* **Full Export Configuration Restored:** `PatcherConfig` now carries all six timing fields (`pre_roll_seconds`, `post_roll_seconds`, `record_start_lead`, `record_stop_trail`, `initial_delay`, `fast_forward_speed`). Initialized from `settings::get_settings()`, all editable via `DragValue` sliders in the Export Configuration panel.
* **Output Directory Routing:** `build_batch_queue` routes patched demos into `{hl_exe_parent}/dod/` via an `output_dir: Option<PathBuf>` field on `PatcherConfig`. The directory is auto-created if absent.
* **POV Filter Enforcement:** Batch payload construction applies the `hide_non_pov` filter before building jobs, consistent with the UI toggle.

### Phase 9: UI Structural Refactor (`capture_ui.rs` -> `views/capture/`)
* **Dead Code Purge:** Cleaned out stale statics (`WORKER_STATE`, `PROGRESS_MSG`, `PROGRESS_PCT`) and zombie imports leftover from the pre-async patcher architecture.
* **Modularization:** Split the monolithic 891-line `capture_ui.rs` into `views/capture/mod.rs` (dispatcher), `scan.rs`, `select.rs`, and `capture.rs`.
* **State Encapsulation:** Routed all shared state (`CaptureState`, `HighlightRules`, `PatcherConfig`) into `OnceLock<Mutex<T>>` managed by the parent module.

### Phase 10: Backend Structural Refactor (`patch.rs` -> `patch/`)
* **Monolith Retirement:** Split the massive 1,197-line `patch.rs` into a structured module (`mod.rs`, `types.rs`, `highlevel.rs`, `scanner.rs`, `builder.rs`, `engine.rs`).
* **WASM Target Protection:** Meticulously applied `#[cfg(not(target_arch = "wasm32"))]` gates to ensure WASM builds (which lack disk/process IO) continue to compile against the types.

### Phase 10b/c: Capture Engine Stability Upgrades (June 2026)
* **Disk Thrashing Fix:** Prevented `settings.json` from being spammed to disk 60 times a second on text input.
* **State Machine Protection:** Prevented double-clicks on the Launch button and gated the "Proceed to Render" button to prevent user navigation mid-capture.
* **Cancellation Token:** Implemented a robust `Arc<AtomicBool>` cancellation mechanism allowing users to instantly kill the HLAE background batch via a `child.kill()` polling loop in the engine.
* **Fixes:** Dash-in-filename truncation bug, viewdemo frame_count overflow/array corruption.
* **Memory Optimizations:** Refactored StreamPatcher to use `std::io::copy` and a unified `scratch_buf` to eliminate all heap allocations within the loop.
</details>

---
# File: docs/active_context.md

## Web AI State
- **Current Goal:** Transition complete. Core protocols, root rulesets, and automated bootstrapping mechanisms are fully synchronized. Next engineering phase will target `milestones.md` formatting standardization and core `dod-tools` parser feature implementation.
- **Last Modified:** `docs/ai_rules/ai_architecture_protocols.md`, `docs/ai_rules/ai_execution_protocols.md`, `.cursorrules`, `build_bootstrap.ps1`.
- **Unresolved Bugs:** None. Core workspace is 100% clean and stabilized.

## IDE AI State
- **Current Goal:** Standardize `docs/milestones.md` markdown layout to prevent AI-driven format drift.
- **Next Action:** Execute `.\build_bootstrap.ps1` to compile `bootstrap_payload.md`, then evaluate and restructure `docs/milestones.md`.
- **Unresolved Bugs:** None.
