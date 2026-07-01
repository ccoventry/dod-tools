# dod-tools & HLCR Workspace
> **High-performance Rust suite for parsing, analyzing, and manipulating Half-Life/Day of Defeat 1.3 `.dem` files.**

## 📋 What This Is
`dod-tools` is a modular toolkit and GUI application designed to parse raw Day of Defeat 1.3 demo files, extract match analytics, and drive the Half-Life Clip Renderer (HLCR) pipeline to automate HLAE cinematic recording.

---
**GLOBAL AI RULE:** Always reference the `.cursorrules` file in the workspace root for behavioral rules and token efficiency. Use the `docs/` folder for modular project knowledge. Always provide code as minimal diffs/blocks.
---

## 📁 Repository Structure
/
├─ .cursorrules         # 🔒 Global agent constraints (WASM, Lock-Free)
├─ docs/                # 📚 Canonical project context and decisions
│  ├─ README.md         # ← You are here
│  ├─ overview.md       # Project intent, scope, success criteria
│  ├─ architecture.md   # Rust architecture, UI loops, WASM gates
│  ├─ domain_quirks.md  # GoldSrc alignment bugs, HLAE command strictness
│  ├─ bugs.md           # Known issues and limitations
│  ├─ conventions.md    # Coding standards and patterns
│  ├─ decisions.md      # Technical decisions and rationale
│  ├─ env.md            # Environment setup and dependencies
│  ├─ milestones.md     # Roadmap and active task backlog
│  ├─ references.md     # External resources and links
│  └─ ai_rules/         # Agent-specific execution and prompt protocols
├─ dod/                 # 💻 Low-Level Parser (nom/dem)
├─ analysis/            # 💻 Metrics & Analytics generation
└─ native/              # 💻 CLI Tooling & egui GUI views

## 🎬 Starting a New AI Session
1. **Assume the agent has no memory** of previous conversations.
2. **Read `.cursorrules`** for operational constraints.
3. **Read `docs/milestones.md`** to understand the active target.
