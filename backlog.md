# 📋 Project Backlog & Future Improvements

> **⚠️ STRATEGIC PIVOT: CAPTURE STUDIO EFFICIENCY**
> All non-critical UI and exploratory tasks have been temporarily shelved. The absolute priority is optimizing the **Capture Studio** to export perfectly clean, tick-accurate `.mov` clips for the Dice WSOD LAN movie workflow, ahead of the late July deadline in Philadelphia.

---

## 🎯 ACTIVE PRIORITY: Capture Studio Overhaul

These tasks must be executed next to finalize the movie production pipeline and ensure frame-perfect exports for Vegas Pro.

| ID | Difficulty | Task | Area | Description | Dev Notes |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **C1** | 🔴 **Hard** | **Fast Streaming Byte-Copy Patcher** | Capture | Rewrite the patcher to bypass the deep parser. Read/write raw frame bytes to maximize file I/O speed while maintaining file integrity. | Turn a 10s parse into a 0.1s copy. **CRITICAL:** Must track `injected_byte_count` and rewrite the `.dem` header's `directory_offset` at EOF to prevent file corruption. |
| **C2** | 🟡 **Medium** | **Tick-Based Timing Accuracy** | Capture | Abandon floating-point `time` matching to prevent timing drift and missed clips. | Target absolute `tick` values for console command injection. e.g., `start_record_tick = kill_tick - buffer_ticks`. |
| **C3** | 🟡 **Medium** | **Multi-Streak Single-Demo Batching** | Capture | Group multiple killstreaks by their source `.dem` file, outputting one patched demo per match. | **CRITICAL:** Implement `merge_overlapping_streaks`. If the gap between streaks is smaller than the pre-buffer, merge them into one continuous recording block to prevent command crossover. |
| **C4** | 🟢 **Easy** | **GoldSrc Decal Flush Trick** | Capture | Wipe the world clean of blood/bullets between streaks without reloading the map. | During the pre-record buffer, inject `host_framerate 0`, then `r_decals 0` (flushes engine memory), followed instantly by `r_decals 5555`. |

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
| **M31** | 🟡 **Medium** | **Extract POV Stats** | Parser | Move POV logic out of `lib.rs`. | *Prerequisite for H1.* |
| **M32** | 🟡 **Medium** | **Deconstruct Player Tab** | GUI | Componentize `player_details.rs`. | *Prerequisite for H1.* |
| **H1** | 🔴 **Hard** | **Combine Weapon/POV Tabs** | GUI | Merge layouts. | *Must complete M31/M32 first.* |
| **H2** | 🔴 **Hard** | **Objective Capture Timelines**| Parser | Track flags captured (`CapMsg`). | Build timeline widget. |
| **H3** | 🔴 **Hard** | **POV Ammo Box Tracking** | Parser | Track `w_ammobox.mdl` timelines. | *Renamed to fix H2 collision.* |
| **H4** | 🔴 **Hard** | **Zero-Copy String Decoding** | Parser | Use `Cow<'a, str>` for speed. | *Conflicts with H5. Must choose one.* |
| **H5** | 🔴 **Hard** | **Streaming Demo Parser** | Parser | Drop buffer for $O(1)$ memory. | *Conflicts with H4. Must choose one.* |
| **H6** | 🔴 **Hard** | **Vec Capacity Pre-alloc** | Parser | Size heuristics for memory. | Use demo file size scale factors. |
| **H7** | 🔴 **Hard** | **"Trim Demo" Tool** | CLI | Slice out warmup frames. | *Requires String Table Rebuilder.* |
| **H9** | 🔴 **Hard** | **Interactive Minimap** | GUI | 2D player positions on map. | Requires 3D coordinate parsing. |

---

## ✅ COMPLETED TASKS (Recent)

*A summary of recently cleared major milestones.*

| ID | Difficulty | Task | Description |
| :--- | :--- | :--- | :--- |
| **[x] H10** | 🔴 **Hard** | **Match Clustering Engine** | Grouped files via SipHash content fingerprinting, ignoring OS dates. |
| **[x] M20** | 🟡 **Medium** | **Score Reset Resiliency** | Executed Event Sourcing pivot. Parser is now stateless and survives `sv_restartround`. |
| **[x] M23** | 🟡 **Medium** | **Premium UI Cards** | Upgraded UI with custom `egui::Frame` metric cards and active tab styling. |
| **[x] M33** | 🟡 **Medium** | **Modularize GUI Entrypoint** | Broken down the monolithic `main.rs` into `tree.rs`, `browser.rs`, etc. |

*(Earlier completed tasks have been archived from this view but remain in version control).*