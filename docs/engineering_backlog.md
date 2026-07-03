# Engineering Backlog

## 🚀 Active Sprint Priorities
- [ ] Task: Audit and strip lingering template text lines out of `docs/app_architecture.md`.
- [ ] Task: Ensure all local terminal pipelines cleanly integrate the `.cursorrules` `cmd 2>&1 | tuf` parameter sequence.
- [ ] Task: Verify that the `build_bootstrap.ps1` dynamic file utility executes targeted laser payloads under 50 lines.

## 📋 General Backlog (Future Roadmap Items)
- [ ] Feature: Set up an automated directory watch mechanism looking out for local `DOD_BATCH_DONE` signals.
- [ ] Refactor: Isolate native engine stream slicing mechanisms behind explicit `target_arch` macro controls.
- [ ] Chore: Build absolute workspace path anchors derived natively from `std::env::current_exe()`.

## 🛑 Non-Goals (Out of Scope)
- No support for native engine multi-threading layers operating inside immediate-mode UI thread blocks.
- No direct architectural support for alternative Source/GoldSrc modifications outside of Day of Defeat 1.3.
