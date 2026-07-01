# 🛠️ AI Execution & IDE Protocols

## 1. IDE File Editing & Code Spills
- **No Code Spills:** Only rewrite the specific lines or functions requiring modifications. Do not drop, modify, or aggressively reformat unrelated logic blocks within the same file.
- **File Editing:** Do not output manual diffs, `@@` syntax, or full file reprints in the chat for standard project code. **EXCEPTION:** If you edit a file inside the `.agents\rules\` directory (or similar config), you MUST output a markdown `diff` block in the chat showing exactly what lines you added/removed, because the IDE does not provide visual diffs for these configuration files.
- **Edit Scope:** Restrict file edits strictly to the lines requiring changes. Do not reformat or overwrite untouched functions in the same tool call.
- **Workspace Search Exclusions:** System searches and file queries are strictly prohibited from indexing the target/ directory, the demos/ folder, and the Cargo.lock file. Massive asset maps inside the localizations/ folder must never be read in full; utilize targeted, line-clamped lookups (e.g., grep -m 5).
- **The Table Delete-and-Recreate Rule:** When moving items across layout-critical markdown tables, you must delete the source row entirely before generating the destination row to prevent text string fragments and visual duplication.

## 2. Terminal Output & `tuf` Pipeline
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

## 3. Internal Reasoning & Protocol
- **Internal Reasoning:** Keep internal CoT (Chain of Thought) focused exclusively on technical logic. Skip all conversational filler, apologies, and summary introductions.

## 4. Context Handoff Protocol
Whenever the user types `Initiate Context Handoff`, `wrap up`, or `session over`, you must immediately halt execution and generate a dual-part exit package:

**Part 1: 🧠 Session Lessons Learned (Knowledge Extraction)**
You must act as a forensic knowledge extractor. Scan the entire conversation history of THIS chat:
- Ignore standard syntax or UI changes that worked perfectly. Focus EXCLUSIVELY on "gotchas" (e.g., engine quirks, threading deadlocks, hallucinated commands).
- If we encountered an error and fixed it, extract the underlying rule that prevents that error.
- Capture any workflow friction points: if the AI read too many tokens, hallucinated a tool, or misunderstood a command, extract the protocol needed to prevent it in the future.
Output the results as a raw, bulleted Markdown list grouped into `GoldSrc/HLAE Engine Quirks`, `Rust/Architecture Constraints`, and `Agent Protocols & Workflow Optimizations`. You must use your file editing tools to AUTOMATICALLY APPEND these harvested rules to `docs/session_lessons.md`. Do not rely on the user to copy-paste them.

**Part 2: 📦 Context Payload (State Transfer)**
Output a minified markdown code block designed to seed the new chat window. It MUST include:
- **CRITICAL BOOTSTRAP INSTRUCTION:** Read `docs/ai_rules/ai_execution_protocols.md` before processing context.
- The current overarching goal.
- The specific crate, file, and function last edited.
- Any unresolved compiler errors or bugs.
- The exact next step or terminal command to execute.
Keep this payload as concise as possible to consume minimum tokens.
