## Web AI State
- **Overarching Goal:** Stabilize the `dod-tools` demo capture pipeline by eliminating binary desyncs and engine crashes during HLAE recording.
- **Last Modified:** `native/src/patch/engine.rs` (Fixed bookmark interleaving), `native/src/patch/builder.rs` (Fixed test alignment).
- **Compiler Status:** `cargo test` and `cargo check --bin parse_demo` are passing; binary structure is validated frame-for-frame.
- **Unresolved Bugs:** None; the `svc_bad` crash is confirmed resolved.

## IDE AI State
- **Goal:** Stream patcher bookmark frame layout corrected, code verified, tests passing, and documentation updated.
- **Status:** Done. Standing by for further workspace tasks.
