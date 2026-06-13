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
- **Execution Hand-off**: Write the script code, then immediately stop tool execution and prompt the user to run it and report the isolated JSON findings back to you.

## 5. EXECUTION & OUTPUT PROTOCOLS
- **File Editing:** Do not output manual diffs, `@@` syntax, or full file reprints in the chat. You must use the IDE's built-in file editing tool to apply changes directly.
- **Edit Scope:** Restrict file edits strictly to the lines requiring changes. Do not reformat or overwrite untouched functions in the same tool call.
- **Terminal Errors:** When running terminal commands, do not echo the raw compiler output back to the chat. Provide a one-sentence summary of the failure and immediately propose the fix.
- **Internal Reasoning:** Keep internal CoT (Chain of Thought) focused exclusively on technical logic. Skip all conversational filler, apologies, and summary introductions.
- **Hard Pivot Halt:** If my prompt asks you to edit a crate or feature outside the current workspace focus (e.g., switching abruptly from `dod` low-level parsing to `native` GUI layout), stop all code generation immediately. Output exactly: "⚠️ **Scope Change:** Please accept pending changes and open a new chat to preserve context."

# Terminal & Context Interaction Rules
1. I have an automated profile script that saves terminal error/success output to unique files inside the root 'scratch/' folder.
2. ONLY when a command is meant to capture complex logs, compiler errors, or diagnostics for you to analyze, format it as: `Your-Command 2>&1 | tuf`.
3. For simple administrative or deployment actions that do not require your review (e.g., git push, git commit, cd, mkdir), provide the plain standard command directly without piping to 'tuf'.
4. When a 'tuf' command is used, instruct me: "Run the command. Once it finishes, type 'done' and I will automatically analyze the newest file generated in your scratch directory."
5. On the turn immediately following a 'tuf' execution confirmation, look inside the 'scratch/' folder, sort by creation date, and pull the single newest 'output_*.txt' file into your context window.
6. Always format terminal or PowerShell commands in a standard Markdown code block (e.g. using triple backticks) so they can be easily copied or run.