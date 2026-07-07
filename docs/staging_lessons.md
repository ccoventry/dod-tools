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


## Workflow & AI Protocols
- **Windows OS Error 4551 (Application Control Block):** Unsigned compiled binaries in the `target/release/` directory may be blocked by Windows Defender Application Control (WDAC) or Antivirus policies, throwing OS error 4551. Note that standard folder exclusions bypass the malware scanner, but do NOT bypass Smart App Control or WDAC execution policies on Windows 11. To mitigate this, Smart App Control or App & browser control settings must be adjusted manually.
- **Waterfall Resolution Pattern:** External dependencies (FFmpeg) must utilize a prioritized resolution chain (User Override -> Bundled Local -> System Path) to ensure resilience in non-containerized Windows environments.
- **Local Signing Protocol:** To bypass Windows Smart App Control/WDAC, use a PowerShell-generated local root certificate (`CN=LocalRustDev`) and register it in the Trusted Root Certification Authorities store.
- **State Management:** In complex UIs, prioritize read-only status labels over duplicate input fields to maintain a single source of truth and prevent configuration drift.

