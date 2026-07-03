# System Architecture Decisions

## 🛠️ Tech Stack Core
- **Language/Runtime:** Rust targeting native desktop and `wasm32-unknown-unknown` GUI compilation.
- **UI Framework:** `egui` (Zero-allocation immediate-mode rendering loop).

## 📦 Workspace Module Boundaries
- `native/`: Handles direct GoldSrc hooks, binary stream patching, and memory injection layers.
- `src/ui/`: Contains the immediate-mode GUI modules. Standard native threading and direct `std::fs` operations are strictly forbidden inside this module.
- `shared/`: Holds immutable type definitions, message schemas, and atomic communication channels passing events between native loops and the UI thread.

## 🔒 State, Memory, & Concurrency Rules
- **State Ingestion:** Immediate-mode execution. Telemetry data feeds sequentially from native processes into active UI state tracking blocks via bounded sync channels.
- **Micro-Stutter Protection:** Highly accessed internal asset catalogs are wrapped in `std::sync::RwLock`. Under no circumstances should blocking mutex synchronization primitives be introduced directly on the UI frame loop thread.
- **Network Header Alignment:** Network messages expect exactly 468 bytes of header data before reading payload lengths. Ensure byte streams slice exactly on this structural boundary before passing to structural deserializers.
- **Memory Safety Guardrails:** Never use fixed-size stack buffers (`[u8; N]`) for binary stream slicing inside the engine memory layer. Always use heap-allocated `Vec<u8>` gated by explicit 2MB payload limits to avoid stack overruns during memory injection.