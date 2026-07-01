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
