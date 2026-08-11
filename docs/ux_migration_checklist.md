# UX Migration Checklist: Legacy egui → Tauri/Vite
> Extracted from the `dod-tools-gui` legacy audit. Framework-specific egui issues
> (layout primitives, color constants) are excluded. Only logical UX flaws and
> missing feedback loops are listed here, mapped to their web-frontend solutions.

---

## Legend
- `[ ]` Not started
- `[/]` In progress
- `[x]` Done / already handled in Tauri branch

---

## Category A — Feedback Loops & Notifications

| # | Legacy Flaw | Web-Frontend Solution | Status |
|---|---|---|---|
| A1 | **Silent notification drop** — `self.notification` set but never rendered; project save/load gave zero user feedback | `showToast()` in `toast.js` must be called on every save, load, scan-complete, and cancel event. Currently wired for scan and save; verify it fires on all code paths | `[/]` |
| A2 | **Success hacked into error field** — clipboard/copy success detected by checking `error_message.contains("copied to clipboard")` | Use a dedicated `showToast(msg, 'success')` call at every copy-to-clipboard action; never route success through the error display channel | `[ ]` |
| A3 | **Toast has no fade** — 3-second hard cutoff, no animation | CSS `@keyframes` fade-out in the final 0.5s; `toast.js` should set `opacity: 0` before removing the DOM node | `[ ]` |
| A4 | **No ingestion progress** — during scan, UI showed static text with no cancel affordance | `triggerAutoScan()` disables buttons and shows toast. **Missing:** indeterminate progress bar or spinner visible in `#master-demo-table-body` | `[/]` |

---

## Category B — Global Shortcut Leakage

| # | Legacy Flaw | Web-Frontend Solution | Status |
|---|---|---|---|
| B1 | **Capture Studio shortcuts fired on all tabs** — `Ctrl+S/N/O/W` were always active regardless of active view | Scope `keydown` listeners to the active wizard step: check `document.querySelector('.nav-tab-btn.active')?.dataset.nav` before acting. Never attach global `document` keyboard handlers for destructive actions | `[ ]` |
| B2 | **`Ctrl+O` opened a project, not a demo** — mismatch on Workspace tab | On `nav === 'workspace'`: `Ctrl+O` → `#add-files-btn` flow. On `nav === 'export-config'`: `Ctrl+O` → load project dialog. Route by active nav key | `[ ]` |

---

## Category C — Recovery & Blocking Modals

| # | Legacy Flaw | Web-Frontend Solution | Status |
|---|---|---|---|
| C1 | **Recovery modal blocked entire UI** — `return` after drawing modal prevented all other rendering | Web modals are DOM overlays; they don't block the render loop. Ensure any recovery overlay does NOT `display: none` the entire `<body>` | `[ ]` |
| C2 | **Window-close without Recover/Discard left stuck state** | On Tauri `CloseRequested` event: if a recovery JSON exists, prompt via `dialog.confirm()`. Use `appWindow.onCloseRequested()` to intercept | `[ ]` |
| C3 | **Dead double-assignment after recovery** | Ensure project-load callback sets exactly one tab active via `activateWizardStep()` — no double-call on load path | `[x]` |

---

## Category D — State Persistence & Settings

| # | Legacy Flaw | Web-Frontend Solution | Status |
|---|---|---|---|
| D1 | **Language preview applied before save** | When added: apply to DOM immediately, but persist to disk only on explicit Save | `[ ]` |
| D2 | **No unsaved-changes indicator in Settings** | Track a `settingsDirty` boolean; show badge on Settings nav button; enable Save CTA only when dirty | `[ ]` |
| D3 | **"Bookmarks" vs "Pinned Folders" naming inconsistency** | Standardize on "Demo Folders" in UI labels, `"scan_paths"` internally | `[ ]` |

---

## Category E — Demo List & Browser

| # | Legacy Flaw | Web-Frontend Solution | Status |
|---|---|---|---|
| E1 | **Keyboard navigation stub was empty** — arrow-key handler never implemented | Add `keydown` listener on the demo table container: `ArrowUp`/`ArrowDown` shift `selectedDemoIdx`, `Enter` triggers select. Maintain focus ring via CSS `:focus-within` | `[ ]` |
| E2 | **Filter bar overflowed at narrow widths** | If additional filters are added: use CSS `flex-wrap: wrap` on the filter toolbar container | `[ ]` |
| E3 | **Date filter had no format hint or validation** | Use `<input type="date">` or `placeholder="YYYY-MM-DD"` with live regex validation | `[ ]` |
| E4 | **Demo list footer showed hardcoded 0** | Wire `currentScannedDemos.length` into a status bar element after each scan. Sum `demo.streaks.length` for total streak count | `[ ]` |
| E5 | **Draw shown as loss** — tie case displayed `<` | In scoreboard renderer: `a > b ? '>' : a < b ? '<' : '='` | `[ ]` |

---

## Category F — Capture Studio & Export

| # | Legacy Flaw | Web-Frontend Solution | Status |
|---|---|---|---|
| F1 | **No cancel during ingestion** — UI froze with no cancel affordance | Add `cancel_scan` Tauri command backed by `AtomicBool` in a `ScanManager` state; wire frontend "Cancel Scan" button | `[ ]` |
| F2 | **Export Manager footer hardcoded stubs** | `render_pane.js` wires to `render_status` event. Verify status text element updates on every event emission | `[/]` |
| F3 | **Batch export: no per-streak progress** | `execute_render_batch` emits `current_frame`/`total_frames`. Wire into a `"Rendering X/Y"` visible label | `[/]` |
| F4 | **Capture status polling instead of push** — `getCaptureStatus()` polled every 500ms | Add `app.emit("capture_status", ...)` calls inside the `EngineEvent` listener thread; switch frontend to `listen('capture_status', ...)` | `[ ]` |

---

## Category G — Analysis / Telemetry View

| # | Legacy Flaw | Web-Frontend Solution | Status |
|---|---|---|---|
| G1 | **Analysis fields returned as Null** — key-guessing in `analyze_demo` may miss actual field names | Audit the JSON shape produced by `serde_json::to_value(&analysis)` for a real demo. Confirm key names match what `lib.rs` probes for. Add smoke test | `[ ]` |
| G2 | **No loading state for analyzeDemo()** — inline auto-analyze errors silently swallowed | `loadAndShowTelemetry()` shows spinner in modal path. Inline auto-analyze on selection should show brief spinner in `#telemetry-container` | `[/]` |
| G3 | **No search/filter on player dropdown** | Use `<datalist>` or searchable `<select>` for demos with 30+ players | `[ ]` |

---

## IPC Safety Note
All `invoke()` calls in `ipc_bridge.js` chain `.catch()`. **One gap:** `analyzeDemo()` catches the error and `throw`s it but does NOT call `showToast`. The call-site in `main.js` also silently catches. Ensure all IPC failure paths surface a user-visible error message.

---
*Generated during Tauri Migration State Audit — branch: feature/tauri-migration*
