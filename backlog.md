# 📋 Project Backlog & Future Improvements

> **⚠️ STRATEGIC PIVOT: CAPTURE STUDIO EFFICIENCY**
> All non-critical UI and exploratory tasks have been temporarily shelved. The absolute priority is optimizing the **Capture Studio** to export perfectly clean, tick-accurate `.mov` clips for the Dice WSOD LAN movie workflow, ahead of the late July deadline in Philadelphia.

---

## 🎯 ACTIVE PRIORITY: Capture Studio Overhaul

### ➔ UP NEXT (Phase 7: HLAE Capture Integration)
* **[ ] Capture Queue State Machine:** Build a state machine that tracks the active index of the patched demo list and manages transition states.
* **[ ] IPC Process Spawning:** Implement execution of `hl.exe` via HLAE using appropriate launch command-line arguments.
* **[ ] Process Lifecycle Hooks:** Implement waiting for the engine to process the injected `quit` command, catching the exit code/signal, and automatically advancing to the next patched file.
* **[ ] Artifact Verification:** Add a directory monitoring safety scan to verify that the expected frame folders/images were successfully written to disk before advancing the queue.

---

## 🧊 THE ICEBOX: Secondary / Shelved Tasks

*These tasks are sidelined until the Dice WSOD movie pipeline is complete and the Capture Studio is operating flawlessly.*

| ID | Difficulty | Task | Area | Description | Dev Notes |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **E16** | 🟢 **Easy** | **WASM Worker Race Condition** | GUI | Fix loading stall on WebAssembly drop. | Javascript message queue buffer. |
| **E20** | 🟢 **Easy** | **Gitignore Diagnostic Files** | Project | Add `recent_commits.txt` to `.gitignore`. | Standard repository cleanup. |
| **M10** | 🟡 **Medium** | **Project Renaming** | Project | Rename to support broader HL mods. | Update workspace configurations. |
| **M11** | 🟡 **Medium** | **Server Mod Detection** | Parser | Surface AMX/Warcraft3 mods in UI. | Scan `TextMsg`/`Motd` frames. |
| **M15** | 🟡 **Medium** | **Announce Cap-Outs** | GUI | Display team cap-out in Chat Log. | Parse `#Game_Allies_Capped`. |
| **M17** | 🟡 **Medium** | **Announce Flag Captures** | GUI | Add flag capture events to Chat Log. | Match text templates. |
| **M21** | 🟡 **Medium** | **Colored eGui Icons** | GUI | Premium vector folder icons. | Style with `egui::RichText`. |
| **M22** | 🟡 **Medium** | **Auto-Detect Localizations** | Core | Load Steam/HL translation catalogs. | Dynamic path traversal. |
| **M25** | 🟡 **Medium** | **Graceful Demo Corruption** | Parser | Stop on `message_length` errors. | *Easier now due to Event Sourcing.* |
| **M30** | 🟡 **Medium** | **Centralize Cache Path** | Core | Use OS AppData for caches. | Use `dirs` crate. |

---

## ✅ COMPLETED TASKS (Recent)

*A summary of recently cleared major milestones.*

| ID | Difficulty | Task | Description |
| :--- | :--- | :--- | :--- |
| **[x] C6** | 🔴 **Hard** | **Phase 6: Standalone Ingestion & Absolute Command Scheduling** | **Standalone Ingestion Engine:** Background thread parsing of `DeathMsg` frames using sliding window heuristics (`max_time_gap`, `min_kills`) to auto-discover highlights.<br>**Absolute Tick Command Scheduling:** UI offsets are mathematically converted to absolute ticks using first/last frame timings and dynamically injected via console commands.<br>**Tickrate Delta Math:** Calculated tickrate strictly via first/last 9-byte frame header timing delta.<br>**HLTV POV Separation:** Composite key grouping `(source_demo, target_player)` to build specialized names, injecting `spec_player` and `spec_mode 4`.<br>**High-Performance Caching:** `QueueGroupingMode` (By Demo, By Player, Flat List) using cached models to protect `egui` render performance.<br>**Chronological Flat Sort:** Sorts flat list alphabetically by demo name, then ascending by `start_tick`.<br>**Analytics Isolation:** Player Details tab converted to read-only layout (removed "+" queue buttons). |
| **[x] C1** | 🔴 **Hard** | **Fast Streaming Byte-Copy Patcher** | Implemented high-performance stream copy with directory structure rewriting. |
| **[x] C2** | 🟡 **Medium** | **Tick-Based Timing Accuracy** | Shifted to absolute tick-based event matching to eliminate timing drift. |
| **[x] C3** | 🟡 **Medium** | **Multi-Streak Single-Demo Batching** | Built queue system with smart overlap merging to bundle streaks. |
| **[x] C4** | 🟢 **Easy** | **GoldSrc Decal Flush Trick** | Integrated host_framerate decoupling and decal flushing. |
| **[x] C5** | 🟡 **Medium** | **Cancellation, Cleanup & Thread Reaping** | Added atomic cancellation check, physical file cleanup, and thread handle joining. |
| **[x] H10** | 🔴 **Hard** | **Match Clustering Engine** | Grouped files via SipHash content fingerprinting, ignoring OS dates. |
| **[x] M20** | 🟡 **Medium** | **Score Reset Resiliency** | Executed Event Sourcing pivot. Parser is now stateless and survives `sv_restartround`. |
| **[x] M23** | 🟡 **Medium** | **Premium UI Cards** | Upgraded UI with custom `egui::Frame` metric cards and active tab styling. |
| **[x] M33** | 🟡 **Medium** | **Modularize GUI Entrypoint** | Broken down the monolithic `main.rs` into `tree.rs`, `browser.rs`, etc. |

*(Earlier completed tasks have been archived from this view but remain in version control).*