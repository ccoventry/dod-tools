# Project Conventions

## Naming
- **Capture Nomenclature:** The capture mechanism must strictly be referred to as "HLAE Game Capture". The term "Native Game Capture" is deprecated to prevent semantic drift.

## Code Style
- **UI Logging:** Do not use `println!` or `eprintln!` for UI diagnostic logging, as it causes I/O bottlenecks. Use the custom `log_markdown` macro.
- **WASM Gating:** All file system reads (`std::fs`) and native thread spawning must be explicitly gated behind `#[cfg(not(target_arch = "wasm32"))]`.

## Do / Don't
- **DO** use `egui_extras::TableBuilder` for lists to maintain flat hierarchies.
- **DON'T** use `unwrap()` in binary `.dem` parsers; always handle malformed bytes gracefully to prevent queue crashes.
