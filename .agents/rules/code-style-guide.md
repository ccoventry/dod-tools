---
trigger: always_on
---

# === dod-tools Project Cursor Rules & Guidelines ===

This workspace is a Rust-based toolkit for parsing, analyzing, and visualizing Day of Defeat (DoD) demo files (`.dem`) and logs.

## 1. PROJECT ARCHITECTURE & CRATE RESPONSIBILITIES
Maintain strict separation of concerns across workspace crates:
- **`dod` (Low-Level Parser)**: Parses raw `.dem` and logs via `nom`/`dem`. Keep data structures strictly close to the raw protocol. Do not introduce UI or aggregate analysis states here.
- **`analysis` (Metrics & Analytics)**: Consumes raw structures from `dod` to compute aggregate game states (scoreboards, weapon stats, killstreaks). Pure analytical library independent of rendering frameworks.
- **`native` (CLI & GUI App)**: The delivery layer. 
  - CLI Tools (`cli.rs`, `dump.rs`, `inspect*.rs`): Executables for dumping specific demo details.
  - GUI App (`native/src/bin/gui/`): Desktop & WASM interface using `egui`. Views inside `views/` must handle layout and UI rendering only.

## 2. COMPILATION & WASM TARGETS
- **Rust 2024**: Adhere strictly to modern Rust 2024 idioms.
- **WASM Constraints**: The GUI compiles to `wasm32-unknown-unknown`. Never use non-WASM libraries (like direct file IO or native threads) inside the GUI application without explicit conditional checks (`#[cfg(target_arch = "wasm32")]`).

## 3. WORKSPACE EXCLUSIONS (TOKEN PROTECTION)
- **Massive Directories**: Never search, scan, or read files inside the `target/` directory or the `demos/` folder.
- **Ignore Lock Files**: Exclude `Cargo.lock` from workspace-wide searches.
- **Game Localizations**: The `localizations/` directory contains massive text files. Never read them in full. You must use explicit line limits (e.g., `grep -m 5` or piping to `head`/`tail`) to strictly constrain search outputs when hunting for translation keys.

## 4. THE "BLIND ARCHITECT" PROTOCOL (DEMO PARSING)
- **Binary Ban**: Never open, read, or print raw `.dem` binary files into this chat.
- **Summary-Driven Debugging**: To isolate parsing bugs, write a dedicated debugging script inside a single, recycled scratch file (`scratch/debug.rs`). You must only append to or minimally edit this file; do not rewrite the entire script from scratch for minor changes.
- **Data Air-Gap**: Program the debugging script to output data in a strict, parsable JSON format, or write a targeted frame slice to `debug_slice.txt`. 
- **Minified Output Only**: Ensure all JSON output from debug scripts is strictly minified. Remove all whitespace, indentation, and unnecessary keys from the terminal output to reduce token ingestion.
- **Execution Hand-off**: Write the script code, then immediately stop tool execution and prompt the user to run it and report the isolated JSON findings back to you.

## 5. EXECUTION & OUTPUT PROTOCOLS
- **File Editing:** Do not output manual diffs, `@@` syntax, or full file reprints in the chat. You must use the IDE's built-in file editing tool to apply changes directly.
- **Edit Scope:** Restrict file edits strictly to the lines requiring changes. Do not reformat or overwrite untouched functions in the same tool call.
- **Terminal Errors:** When running terminal commands, do not echo the raw compiler output back to the chat. Provide a one-sentence summary of the failure and immediately propose the fix.
- **Internal Reasoning:** Keep internal CoT (Chain of Thought) focused exclusively on technical logic. Skip all conversational filler, apologies, and summary introductions.
- **CRITICAL SCOPE HALT:** If my prompt asks you to edit a crate or feature outside the current workspace focus (e.g., switching abruptly from `dod` low-level parsing to `native` HLAE rendering), you MUST refuse to write code. Output exactly: "⚠️ **Scope Change:** Please accept pending changes and open a new chat to preserve context." Do not attempt to fulfill the prompt.

## 6. BATCH PROCESSING & QUEUE RULES (TOKEN PROTECTION)
- **Dry-Run Default:** When building or modifying batch jobs (e.g., processing multiple `.dem` files), always implement and test a dry-run mode first (printing target files and calculated timestamps to the terminal) before enabling heavy HLAE execution commands.
- **Fail-Safe Iteration:** Ensure batch loops include error bypassing (using `match` or `if let`). If a single `.dem` file fails to parse, log the error and continue to the next file. Do not allow a single file failure to crash the loop.
- **Settings Buffer logic:** Any configuration UI edits (general preferences, game/HLAE executable directories) must be loaded into a temporary/draft buffer (`draft_settings`) in the GUI state and only persisted to disk (`settings.json`) upon explicit confirmation (e.g., a "Save Settings" click) to prevent excessive disk I/O.

## 7. TERMINAL & CONTEXT INTERACTION RULES
1. I have an automated profile script that saves terminal error/success output to unique files inside the root `scratch/` folder.
2. ONLY when a command is meant to capture complex logs, compiler errors, or diagnostics for you to analyze, format it as: `Your-Command 2>&1 | tuf`.
3. For simple administrative or deployment actions that do not require your review (e.g., git push, git commit, cd, mkdir), provide the plain standard command directly without piping to `tuf`.
4. When a `tuf` command is used, instruct me: "Run the command. Once it finishes, type 'done' and I will automatically analyze the newest file generated in your scratch directory."
5. **Strict File Ingestion:** On the turn immediately following a `tuf` execution confirmation, look inside the `scratch/` folder, sort by creation date, and pull the single newest `output_*.txt` file. **Crucial Limit:** Do NOT read the entire file if it is a compiler error. You must explicitly limit your read to the final 30 lines, or specifically grep/extract only the blocks containing the string `error[`.
6. Always format terminal or PowerShell commands in a standard Markdown code block (e.g. using triple backticks) so they can be easily copied or run. For administrative/deployment actions that the user will execute via the paste/run button, always format them as a single-line command (using semicolons `;` to chain them if necessary).
7. **Terminal Integration Single-Line Rule:** Multi-step sequences or task pipelines executed via terminal commands must be collapsed into a single, chained line (using semicolons `;` as separators in PowerShell) to ensure one-click execution capability for the user.

## 8. CONTEXT HANDOFF PROTOCOL
If the user prompts with exactly `Initiate Context Handoff`, you must immediately stop all current tasks and generate a minified context payload designed to seed a new chat window. 
1. Output the payload inside a single markdown code block.
2. The payload MUST include:
   - **CRITICAL BOOTSTRAP INSTRUCTION:** A prominent instruction telling the new chat instance to immediately read and adhere to the project rules in `.agents\rules\code-style-guide.md` before processing any other context.
   - The current overarching goal.
   - The specific crate, file, and function last edited.
   - Any unresolved compiler errors or bugs currently being investigated.
   - The exact next step or terminal command to execute.
3. Format this payload as concisely as possible (bullet points, zero conversational filler) to consume the absolute minimum amount of tokens in the next chat.


## 9. EGUI & PERFORMANCE RULES (CRITICAL)
1. **Zero-Allocation Render Loops:** `egui` is an immediate-mode GUI. Never use `format!()`, string concatenation, or heavy `.clone()` operations inside the `update` or rendering loops. Pre-calculate all complex strings (like timelines or durations) in background workers and pass them to the UI as static/reference structs.
2. **Anti-Recursion / Layout Rules:** Do not nest dynamically expanding containers (like `CollapsingHeader`) inside virtualized row loops (`show_rows`). If rendering a list of structured data, strictly use `egui_extras::TableBuilder` to prevent 1MB Windows stack overflows (`0xc0000409`).
3. **Data Retention over Parser Filtration:** Never drop or filter out parsed data during the background ingestion phase (e.g., trying to filter non-POV players at the parser level). Always retain all data in the state machine and handle filtration visually at the UI layer. This prevents blank screens if the parser encounters edge cases.
4. **DRY Architecture (Don't Repeat Yourself):** Before implementing new data segmentation or UI tables in the Capture Studio, explicitly audit the `analysis` crate and existing views (like `player_details_ui.rs`) to re-use established logic.