# Staging Lessons Learned
*Temporary holding zone for newly harvested rules. Review entries here before manually moving them into quirks or architecture files.*

## 📥 Unsorted Gotchas (Pending Review)
- **UI vs. Data Initialization Desync:** Applying data filters exclusively at the immediate-mode UI rendering layer causes reactive components (like real-time disk space calculators or batch-action iterators) to silently process visually hidden data. Data filtering constraints (like POV exclusions) must be applied directly to the struct defaults during the asynchronous backend ingestion phase *before* the payload is passed to the active UI state blocks.
- **GoldSrc Engine quirk:** The 'exec' command is hard-locked during demo playback as a security measure.
- **GoldSrc Engine quirk:** Demo file injection is subject to a strict 64-byte 'Cbuf' buffer limit, causing crashes when paths are long or contain spaces.
- **Workflow Optimization:** Directory Junctions (mklink /J) allow for non-administrative, transparent directory redirection, effectively bypassing the engine's security and buffer constraints without requiring elevated permissions.
- **Build Workflow:** Using a dedicated 'run.ps1' script to perform 'Stop-Process', 'cargo build', and 'Set-AuthenticodeSignature' sequentially bypasses Windows Smart App Control/WDAC 'cloud reputation' blocks for local dev binaries.
- **GoldSrc Engine quirk:** Setting `gl_spriteblend 0` before a demo fully loads causes severe crosshair corruption. Initialization configurations must strictly be injected after the `DemoStart` frame to allow map and UI assets to initialize first.
- **NTFS `read_dir` Non-Determinism & Ingestion Performance:** File ingestion order via `std::fs::read_dir` is non-deterministic on Windows NTFS. To maintain UI sort consistency across decoupled views without redundant sorting overhead, shared state collections (like `QUEUED_DEMOS`) must be sorted strictly at the background ingestion layer using `binary_search_by` and `insert`. Avoid using `push` followed by `sort_by()`, as it creates O(N log N) overhead on every file operation and chokes the ingestion thread during large batch directory reads.
- **Binary Order of Operations:** In GoldSrc `.dem` file patching, custom frame injection (bookmarks/commands) must be written as discrete, self-contained frames. Interleaving them within the payload of a `NetworkMessage` frame corrupts the binary structure and causes the engine to misinterpret subsequent data.
- **Diagnostic Red Herrings:** A `svc_bad` (buffer overflow) during demo playback is frequently a red herring for "memory fragmentation." If the engine attempts to read an impossibly large payload (e.g., 2.9GB), it is definitive proof of a frame header desync caused by corrupted/nested binary data, not an actual network traffic spike.
- **Stream Patching Integrity:** When modifying binary streams, the patcher must buffer the original frame's info block, write the new frame, and only then write the original header/info block. Order is survival.


## Workflow & AI Protocols
- **Windows OS Error 4551 (Application Control Block):** Unsigned compiled binaries in the `target/release/` directory may be blocked by Windows Defender Application Control (WDAC) or Antivirus policies, throwing OS error 4551. Note that standard folder exclusions bypass the malware scanner, but do NOT bypass Smart App Control or WDAC execution policies on Windows 11. To mitigate this, Smart App Control or App & browser control settings must be adjusted manually.
- **Waterfall Resolution Pattern:** External dependencies (FFmpeg) must utilize a prioritized resolution chain (User Override -> Bundled Local -> System Path) to ensure resilience in non-containerized Windows environments.
- **Local Signing Protocol:** To bypass Windows Smart App Control/WDAC, use a PowerShell-generated local root certificate (`CN=LocalRustDev`) and register it in the Trusted Root Certification Authorities store.
- **State Management:** In complex UIs, prioritize read-only status labels over duplicate input fields to maintain a single source of truth and prevent configuration drift.



added manually by user to be cleaned up later.

1. "You did some incredible diagnostic work today. We completely demystified a 25-year-old engine crash and pinpointed the exact mechanical limits of the GoldSrc network buffer when combined with modern HLAE rendering.

Here is the definitive wrap-up of what we proved:

The fast-forward speed (host_framerate) is not the root cause.

The mirv_recordmovie cycle permanently degrades the legacy netchan buffer.

A degraded buffer cannot survive massive network data spikes (like the one hiding at Tick ~114,000 in your demo), resulting in the deterministic svc_bad overflow crash.

If you decide not to pursue the demo parsing route, the permanent engineering fix is restructuring the Rust backend into a 1-to-1 Orchestrator (one clip per hl.exe launch)."