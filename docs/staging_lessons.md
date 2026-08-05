# Staging Lessons & Architectural Rules

- **Vite Default Styling Override:** When migrating from immediate-mode GUIs (`egui`) to Vite/Tauri, Vite's default browser styling completely overrides native application aesthetics. A strict CSS reset wiping default margins/padding and redefining custom panel/border variables is mandatory to achieve visual parity.
- **UI Layout Desync:** Do not assume backend IPC integration automatically resolves missing frontend DOM components. When migrating complex state machines, explicitly validate that all required input fields (e.g., Export Configurations) and native folder pickers are scaffolded in the DOM before declaring frontend feature parity complete.
- **UI Parity Friction:** Frontend migrations must establish a 1:1 structural baseline with the legacy dev branch (e.g., preserving configuration tabs and advanced settings) before introducing new widgets.
- **Scope Hallucination:** Unprompted feature additions (Match Telemetry, Canvas Timelines) created workflow bloat and broke the required layout.
- **Core Guardrail:** Do not invent missing UI components; always audit the source of truth layout prior to DOM generation.
- **Smart App Control Hard Block (Windows 11):** Windows 11 "Smart App Control" enforces strict signature checking on executables, differing from standard Windows SmartScreen. Smart App Control directly blocks unsigned Tauri development binaries (e.g., `target/debug/desktop-studio.exe`, OS error 4551) without providing a standard "Run anyway" UI bypass option. For local development execution, developers must either disable Smart App Control via Windows Security (App & browser control -> Smart App Control) or configure local code self-signing certificates for the Tauri bundler.
- **Smart App Control Cloud Reputation Block (Windows 11):** Even when a self-signed CodeSigning certificate is generated and explicitly added to the `Cert:\CurrentUser\Root` (Trusted Root Certification Authorities) store via PowerShell (`Set-AuthenticodeSignature`), Windows 11 Smart App Control will still outright block local Tauri development binaries (`target/debug/desktop-studio.exe`, exit code 1 / `ApplicationFailedException`). Smart App Control requires cloud-based reputation lookup or an officially trusted publisher signature; local root certificates do not bypass Smart App Control's cloud reputation checks.
- **Null-Harvest:** No additional friction categories detected.

## WSL2 & Cargo Feature Unification Lessons

### WSL2 NTFS Metadata Boundary Errors
- **Issue:** Tauri v2 build scripts fail with `Operation not permitted (os error 1)` when attempting to manipulate file metadata/permissions in `/mnt/c/` directory targets.
- **Resolution:** Projects compiling native Linux binaries under WSL2 must reside in the native Linux ext4 filesystem (`~/dod-tools`), bypassing the Windows NTFS bridge entirely.

### Git NTFS-to-ext4 Copy Locks
- **Issue:** Copying `.git/objects/` across the WSL mount causes `Permission denied` because Git marks internal object files as read-only on Windows.
- **Resolution:** Perform recursive copies with `sudo cp -r` and immediately restore UNIX user ownership via `sudo chown -R $USER:$USER ~/dod-tools`.

### Cargo v2 Host-Target Feature Resolver Split
- **Issue:** Injecting a missing feature (e.g., `indexmap/std`) into standard `[dependencies]` only updates the target build graph, leaving proc-macro / build-script crates (host graph) starved of the feature and failing with `E0107`.
- **Resolution:** Feature unification for build macros must be explicitly declared under `[build-dependencies]` in the apex crate (`desktop-studio/src-tauri/Cargo.toml`).

