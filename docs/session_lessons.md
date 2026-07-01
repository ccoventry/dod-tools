# 📝 Draft Rules (Pending Review)

*If you uncover a new rule, bug, or workaround during a session, log it here. The user will periodically review this section and promote validated rules to the permanent canonical documentation.*

### Agent Protocols & Workflow Optimizations
* **IDE Sandbox Limitations:** The IDE AI cannot autonomously clone or browse arbitrary GitHub repositories. Always manually feed raw template files to the Web AI first to bake in project constraints before passing them to the IDE AI.
* **The Zombie File Hazard:** When migrating to a modular architecture, legacy monolithic rule files must be explicitly deleted from the filesystem (including backup folders like `local/`). If they remain, the IDE AI will index them and suffer catastrophic context confusion.
* **Readme Tree Drift:** Generic templates must have their directory tree maps manually updated to reflect the actual workspace files, otherwise the AI will not know the newly seeded files exist or what they are used for.
* **PowerShell Git Commit Chaining:** When generating `git commit` commands with multi-line messages in PowerShell, avoid using newline escape characters (`` `n ``). Instead, chain multiple `-m` flags within a single-line command to safely assemble multi-paragraph commit bodies without breaking the terminal.