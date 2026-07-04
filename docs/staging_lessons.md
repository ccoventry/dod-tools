# Staging Lessons Learned
*Temporary holding zone for newly harvested rules. Review entries here before manually moving them into quirks or architecture files.*

## 📥 Unsorted Gotchas (Pending Review)
- **GoldSrc Engine quirk:** The 'exec' command is hard-locked during demo playback as a security measure.
- **GoldSrc Engine quirk:** Demo file injection is subject to a strict 64-byte 'Cbuf' buffer limit, causing crashes when paths are long or contain spaces.
- **Workflow Optimization:** Directory Junctions (mklink /J) allow for non-administrative, transparent directory redirection, effectively bypassing the engine's security and buffer constraints without requiring elevated permissions.
- **Build Workflow:** Using a dedicated 'run.ps1' script to perform 'Stop-Process', 'cargo build', and 'Set-AuthenticodeSignature' sequentially bypasses Windows Smart App Control/WDAC 'cloud reputation' blocks for local dev binaries.


## Workflow & AI Protocols
- **Windows OS Error 4551 (Application Control Block):** Unsigned compiled binaries in the `target/release/` directory may be blocked by Windows Defender Application Control (WDAC) or Antivirus policies, throwing OS error 4551. Note that standard folder exclusions bypass the malware scanner, but do NOT bypass Smart App Control or WDAC execution policies on Windows 11. To mitigate this, Smart App Control or App & browser control settings must be adjusted manually.
