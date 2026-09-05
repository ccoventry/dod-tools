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

/// Pins every function/constant above against the exact `format!`/literal it
/// replaced at its original call site (text copied verbatim from the pre-PR
/// source, not re-derived), so this refactor can't have silently changed any
/// user-visible wording.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lib_rs_messages_match_their_original_inline_text() {
        assert_eq!(
            project_session_write_failed("proj.json", "disk full"),
            format!("Failed to write {}: {}", "proj.json", "disk full")
        );
        assert_eq!(
            project_session_read_failed("proj.json", "not found"),
            format!("Failed to read {}: {}", "proj.json", "not found")
        );
        assert_eq!(HLAE_EXECUTABLE_NOT_FOUND, "HLAE executable not found at specified path.");
        assert_eq!(HL_EXECUTABLE_NOT_FOUND, "Half-Life executable not found at specified path.");
        assert_eq!(
            demo_file_not_found("demo.dem"),
            format!("Demo file not found: {}", "demo.dem")
        );
        assert_eq!(analyzer_error("bad header"), format!("Analyzer error: {}", "bad header"));
    }

    #[test]
    fn capture_manager_messages_match_their_original_inline_text() {
        assert_eq!(CAPTURE_BATCH_ALREADY_RUNNING, "Capture batch already in progress");
        assert_eq!(NO_STREAKS_IN_PAYLOAD, "No streaks in payload");
        assert_eq!(
            configure_paths_before("previewing"),
            format!("Configure the HLAE and Half-Life executable paths before {}.", "previewing")
        );
        assert_eq!(
            configure_paths_before("launching"),
            format!("Configure the HLAE and Half-Life executable paths before {}.", "launching")
        );
        assert_eq!(
            HLAE_NOT_FOUND_AT_CONFIGURED_PATH,
            "HLAE executable not found at the configured path."
        );
        assert_eq!(
            HL_NOT_FOUND_AT_CONFIGURED_PATH,
            "Half-Life executable not found at the configured path."
        );
        assert_eq!(NO_HIGHLIGHTS_TO_PREVIEW, "This demo has no highlights to preview.");
        assert_eq!(FAILED_TO_BUILD_PREVIEW_JOBS, "Failed to build any preview patch jobs.");
        assert_eq!(
            CONFIGURE_OBS_PATH_BEFORE_LAUNCHING,
            "Configure the OBS executable path before launching."
        );
        assert_eq!(
            OBS_NOT_FOUND_AT_CONFIGURED_PATH,
            "OBS executable not found at the configured path."
        );
    }

    // Every non-panicking call site's spawn_blocking wrapping was already
    // exercised implicitly by the crate's existing capture_manager/map_manager
    // tests once this PR rewired them through these two helpers — what's
    // unique to test here is the join-error text itself, which only surfaces
    // if the spawned closure panics.
    #[tokio::test]
    async fn flatten_spawn_blocking_reports_a_panic_with_the_original_wording() {
        let handle: tokio::task::JoinHandle<Result<(), String>> =
            tokio::task::spawn_blocking(|| panic!("boom"));
        let err = flatten_spawn_blocking(handle).await.unwrap_err();
        assert!(
            err.starts_with("Task join error: "),
            "expected the original \"Task join error: {{}}\" prefix, got {err:?}"
        );
    }

    #[tokio::test]
    async fn spawn_blocking_result_reports_a_panic_with_the_original_wording() {
        let handle: tokio::task::JoinHandle<()> = tokio::task::spawn_blocking(|| panic!("boom"));
        let err = spawn_blocking_result(handle).await.unwrap_err();
        assert!(
            err.starts_with("Task join error: "),
            "expected the original \"Task join error: {{}}\" prefix, got {err:?}"
        );
    }
}
