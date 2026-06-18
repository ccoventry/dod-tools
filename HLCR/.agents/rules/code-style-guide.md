---
trigger: always_on
---

# EXECUTION & OUTPUT PROTOCOLS
- **File Editing:** Do not output manual diffs, `@@` syntax, or full file reprints in the chat. You must use the IDE's built-in file editing tool to apply changes directly.
- **Edit Scope:** Restrict file edits strictly to the lines requiring changes. Do not reformat or overwrite untouched functions in the same tool call.
- **Terminal Errors:** When running terminal commands, do not echo the raw compiler output back to the chat. Provide a one-sentence summary of the failure and immediately propose the fix.
- **Internal Reasoning:** Keep internal CoT (Chain of Thought) focused exclusively on technical logic. Skip all conversational filler, apologies, and summary introductions.

# Terminal & Context Interaction Rules
1. I have an automated profile script that saves terminal error/success output to unique files inside the root 'scratch/' folder.
2. ONLY when a command is meant to capture complex logs, compiler errors, or diagnostics for you to analyze, format it as: `Your-Command 2>&1 | tuf`.
3. For simple administrative or deployment actions that do not require your review (e.g., git push, git commit, cd, mkdir), provide the plain standard command directly without piping to 'tuf'.
4. When a 'tuf' command is used, instruct me: "Run the command. Once it finishes, type 'done' and I will automatically analyze the newest file generated in your scratch directory."
5. On the turn immediately following a 'tuf' execution confirmation, look inside the 'scratch/' folder, sort by creation date, and pull the single newest 'output_*.txt' file into your context window.
6. Always format terminal or PowerShell commands in a standard Markdown code block (e.g. using triple backticks) so they can be easily copied or run. For administrative/deployment actions that the user will execute via the paste/run button, always format them as a single-line command (using semicolons `;` to chain them if necessary) as the UI's quick integration button only appears for single-line blocks.