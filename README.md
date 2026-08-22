# DoD Tools

Tooling for **Day of Defeat 1.3** (GoldSrc) demo files: parsing them for match
analytics, and driving the engine plus HLAE to batch-record highlight clips.

A creative fork of [cgdangelo/dod-tools](https://github.com/cgdangelo/dod-tools).

---

## Component status

The two halves of this repository are at very different levels of maturity.
Read this before judging the codebase by whichever part you happen to open.

| Area | Status | Notes |
| --- | --- | --- |
| `dod/` — DoD 1.3 message parsers | **Stable** | Hand-written parsers for the mod's user messages. No public library covers this layer. |
| `analysis/` — match analytics | **Stable** | Players, scoreboard, kills, rounds, chat, survival times. Recently validated across 624 demos; derived kill counts reconcile exactly with the game server's own frag counter for ~75% of players and within one kill for ~90%. |
| `dem-patch/` — demo reader/writer | **Stable** | Vendored fork of the [`dem`](https://github.com/khanghugo/dem) crate. See *Why the fork* below. |
| `hl-demo-auditor/` — duplicate finder | **Stable** | Small, self-contained. |
| `native/` — capture engine & patcher | **Active development** | Works, but rough edges. Under near-continuous change. |
| `desktop-studio/` — Tauri UI | **Active development** | Capture Studio and Render Studio are functional but **not polished**. Expect UI inconsistencies and in-flight refactors. |

If you are here to look at demo parsing or stats extraction, `dod/` and
`analysis/` are the parts to read, and they are the parts you can rely on.
The capture and render pipeline is a frag-movie workflow and is unrelated.

> [!NOTE]
> **The GUI was rewritten.** Earlier revisions shipped an `egui` desktop app
> with a WebAssembly target. That has been removed. The current frontend is
> Tauri v2 + Vite under `desktop-studio/`, and the old `dod-tools-gui` binary
> and `trunk serve` workflow no longer exist.

---

## Workspace layout

    dod/              nom-based parsers for DoD 1.3 network messages (no I/O)
    analysis/         match analytics built on top of dod/
    dem-patch/        vendored + patched `dem` crate (GoldSrc demo read/write)
    native/           capture engine, demo patcher, take management, FFmpeg
    hl-demo-auditor/  duplicate-demo detector
    benchmark/        parsing/patching performance harness
    desktop-studio/   Tauri v2 + Vite frontend

## Quick start

Analyse a demo and print match analytics:

    cargo run -p analysis --bin parse_demo -- path/to/demo.dem

Headless preview CLI (accepts a demo or a folder):

    cargo run -p native --bin preview_cli -- path/to/demo-or-folder

Desktop app:

    cd desktop-studio
    npm install
    npm run tauri dev

Tests:

    cargo test --workspace

> [!WARNING]
> A handful of tests in `analysis/` currently fail: three localization tests
> assert on translation keys absent from the shipped language files, and one
> developer inspection test panics when a specific demo fixture is missing from
> the working tree. None of them sit on the demo-parsing path, but the suite is
> not green on a fresh clone.

## Why the fork of `dem`

`dem-patch/` is `dem` v0.2.3 with the parse path hardened. Upstream resolves
delta-decoder tables with `.unwrap()` in seven files; when a demo does not carry
the expected delta-description table, that is a **panic rather than a parse
error**, which kills the whole process mid-batch. The fork falls back to the
library's built-in initial delta table where one exists and returns a graceful
parse failure otherwise. As of upstream v0.3.0 all 29 of those call sites are
still present, so the patch is still required.

## Documentation

Engineering notes live in `docs/` — architecture decisions, GoldSrc and DoD
engine quirks, HLAE protocol constraints, and a running bug log.
