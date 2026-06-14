# DoD Demo Analyzer & Tools

🎮 A creative fork of the original [cgdangelo/dod-tools](https://github.com/cgdangelo/dod-tools) project, putting a fresh spin on Day of Defeat v1.3 demo analysis. 

## 💡 About this Fork
This is a passionate, work-in-progress playground. It is heavily inspired by classic tools like **Complexity Demo Player**, aiming to bring detailed game states, team comparisons, and visual overviews into a modern, native desktop dashboard.

## 🛠️ In-Progress Features
Some of the features currently being developed and refined:
* **Summary Section:** Comprehensive overview of demo, server, and game metadata.
* **Individual Player Stats:** Deep-dive performance details for each player, with options to compare players 1:1.
* **Advanced Chat Log:** Full chat log extraction categorized and filterable by channels, states, and factions.
* **Updated Kill Streaks UI:** A redesigned kill streaks section for clearer player and interval breakdowns.

## 🌐 WebAssembly (WASM) Deployment Plan
The project is architected to compile for both native desktop and web page targets.

> [!WARNING]
> The WebAssembly (WASM) target is a work in progress and changes are not always developed in parallel with the native GUI. The web assembly side may lag behind the desktop app in features and functionality.

* **Browser Runtime:** Compiles to WebAssembly via `trunk`, running the parser and analyzer client-side directly in the browser using HTML5 `<canvas>` and `egui`.
* **Zero Server Overhead:** Demos will be processed locally in-browser via drag-and-drop or file upload selectors—no remote servers or databases required.
* **WASM Feature Parity:** Work is in progress to align the web interface with the native build, including embedding translation resources and handling web-safe storage.

## 🚀 Quick Start
* **GUI Desktop Mode:**
  ```powershell
  cargo run -p native --bin dod-tools-gui
  ```
* **GUI Web Mode (Development Server):**
  ```powershell
  trunk serve
  ```
* **CLI Analysis Mode:**
  ```powershell
  cargo run -p native --bin dod-tools-cli -- "path/to/demo.dem"
  ```

*Stay tuned—features and experimental tools are being added dynamically for fun and testing!*



