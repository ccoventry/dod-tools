// messages.rs
// Rust-side counterpart to desktop-studio/src/strings.js: the Tauri command
// layer's own authored user-facing error/status text, centralized so the
// same message can't drift into two different wordings at two call sites
// (see the "Task join error: {}" cluster below, previously duplicated
// verbatim 13 times across 4 files) and shared boilerplate lives in exactly
// one place. Scope matches strings.js's own: text that reaches the frontend
// as a toast/dialog string. Developer-only log::error!/eprintln!
// diagnostics stay inline at their call site, same as strings.js leaves
// console.log/console.error alone. Issue #33, track 2 — a parallel catalog
// to strings.js, not a merge into it (Rust and JS strings are never shared
// cross-language here).
//
// This module doesn't yet cover every hardcoded string in the Tauri command
// layer — it starts with the highest-value, safest-to-extract cluster (the
// spawn_blocking join-error boilerplate) plus a handful of fully-audited
// call sites in lib.rs and capture_manager.rs. See issue #33 for the
// remaining file-by-file punch list.

use std::fmt::Display;

// ── spawn_blocking join-error flattening ────────────────────────────────────
//
// Two closure shapes appear at Tauri command call sites: one whose closure
// itself returns `Result<T, String>` (needs the outer JoinError flattened
// into that same Result via `?`), and one whose closure returns a plain `T`
// (the JoinError becomes the whole function's error case directly, no `?`
// needed). Each gets its own helper rather than forcing one shape to fit
// the other.

/// For a `spawn_blocking` closure returning `Result<T, String>`. Awaits the
/// handle and flattens `Result<Result<T, String>, JoinError>` into
/// `Result<T, String>`, converting a join failure (the blocking task itself
/// panicked) into the same error-string shape as any other command failure.
pub async fn flatten_spawn_blocking<T>(
    handle: tokio::task::JoinHandle<Result<T, String>>,
) -> Result<T, String> {
    handle.await.map_err(|e| format!("Task join error: {}", e))?
}

/// For a `spawn_blocking` closure returning a plain `T` (no inner Result).
/// Awaits the handle and turns a join failure into the same error-string
/// shape the rest of the command layer uses.
pub async fn spawn_blocking_result<T>(handle: tokio::task::JoinHandle<T>) -> Result<T, String> {
    handle.await.map_err(|e| format!("Task join error: {}", e))
}

// ── lib.rs ───────────────────────────────────────────────────────────────

pub fn project_session_write_failed(path: &str, err: impl Display) -> String {
    format!("Failed to write {}: {}", path, err)
}

pub fn project_session_read_failed(path: &str, err: impl Display) -> String {
    format!("Failed to read {}: {}", path, err)
}

pub const HLAE_EXECUTABLE_NOT_FOUND: &str = "HLAE executable not found at specified path.";
pub const HL_EXECUTABLE_NOT_FOUND: &str = "Half-Life executable not found at specified path.";

pub fn demo_file_not_found(demo_path: &str) -> String {
    format!("Demo file not found: {}", demo_path)
}

pub fn analyzer_error(err: impl Display) -> String {
    format!("Analyzer error: {}", err)
}

// ── capture_manager.rs ──────────────────────────────────────────────────────

pub const CAPTURE_BATCH_ALREADY_RUNNING: &str = "Capture batch already in progress";
pub const NO_STREAKS_IN_PAYLOAD: &str = "No streaks in payload";

pub fn configure_paths_before(action: &str) -> String {
    format!("Configure the HLAE and Half-Life executable paths before {}.", action)
}

pub const HLAE_NOT_FOUND_AT_CONFIGURED_PATH: &str = "HLAE executable not found at the configured path.";
pub const HL_NOT_FOUND_AT_CONFIGURED_PATH: &str = "Half-Life executable not found at the configured path.";
pub const NO_HIGHLIGHTS_TO_PREVIEW: &str = "This demo has no highlights to preview.";
pub const FAILED_TO_BUILD_PREVIEW_JOBS: &str = "Failed to build any preview patch jobs.";
pub const CONFIGURE_OBS_PATH_BEFORE_LAUNCHING: &str = "Configure the OBS executable path before launching.";
pub const OBS_NOT_FOUND_AT_CONFIGURED_PATH: &str = "OBS executable not found at the configured path.";
