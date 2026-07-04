# Half-Life Advanced Effects (HLAE) Protocols

## 🎥 Interceptor Command Rules
- **HLAE Naming Invariance:** The ecosystem strictly requires the "HLAE Game Capture" string literal configuration pattern; never revert to or label things as "Native Game Capture".
- **Legacy Command Sets:** Use legacy commands `mirv_movie_filename` and `mirv_movie_fps`. Do not use modern CS:GO commands like `mirv_streams` which will result in invalid console arguments.
- **Slash Escaping Hierarchy:** The native HLAE console input parser fails to evaluate or locate directories containing traditional Unix-style forward slashes (`/`).
  - *Mitigation:* Ensure all runtime paths targeting HLAE inputs replace standard slashes with double-escaped windows backslashes via `.replace("/", "\\\\")`.
- **Diagnostic Captures:** Standard terminal echoing of raw compiler or tool output causes context bloating.
  - *Mitigation:* Pipe commands cleanly to your diagnostic wrapper (`cmd 2>&1 | tuf`) and require the tool to output a single-sentence failure summary. Multi-step shell actions must be collapsed into a single, chained sequence using semicolons (;). DO NOT run `cargo check/build` internally.
- **Process Lifecycles:** External processes require interruptible polling (`child.try_wait()`) matched with a ~16ms thread sleep. Headless wrappers or hooks injected into HLAE must explicitly chain `.kill_on_drop(true)` to guarantee interceptor death when the orchestrator closes. Verify an `Arc<AtomicBool>` cancellation token on every cycle.
- **The Sandbox Escape (Dummy Folder Trigger):** GoldSrc drops `quit` and `exec` strings during live demo playback. The pipeline bypasses this engine block by using `mirv_movie_filename` to generate a dummy directory on disk at completion. The background orchestrator must poll for this directory and immediately execute an aggressive `taskkill /F /IM hl.exe` to tear down the engine loop safely.