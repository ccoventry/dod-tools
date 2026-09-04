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
// Covers every Tauri command file's error/status strings: lib.rs,
// capture_manager.rs, render_manager.rs, audit_manager.rs, map_manager.rs,
// updater_manager.rs, settings_manager.rs, and dir_browser.rs. Not yet
// covered: native/analysis errors that bubble straight through Tauri
// commands unwrapped (a separate crate, out of scope for this pass — see
// issue #33).

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

/// Generic "Failed to write <path>: <err>" — same shape independently
/// authored at least twice (lib.rs's project session save, plus whatever
/// else reads a user-given path); reused rather than re-typed per call site.
pub fn failed_to_write_file(path: &str, err: impl Display) -> String {
    format!("Failed to write {}: {}", path, err)
}

/// See [`failed_to_write_file`] — the read-side counterpart, same reuse
/// rationale (lib.rs's project session load, dir_browser.rs's directory
/// listing).
pub fn failed_to_read_file(path: &str, err: impl Display) -> String {
    format!("Failed to read {}: {}", path, err)
}

pub const HLAE_EXECUTABLE_NOT_FOUND: &str = "HLAE executable not found at specified path.";
pub const HL_EXECUTABLE_NOT_FOUND: &str = "Half-Life executable not found at specified path.";

pub fn demo_file_not_found(demo_path: &str) -> String {
    format!("Demo file not found: {}", demo_path)
}

pub fn not_a_directory(path: &str) -> String {
    format!("Not a directory: {}", path)
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

pub fn obs_connection_test_failed(err: impl Display) -> String {
    format!("OBS connection test failed to run: {}", err)
}

pub fn obs_orphan_check_failed(err: impl Display) -> String {
    format!("OBS orphan check failed to run: {}", err)
}

pub fn obs_orphan_recovery_failed(err: impl Display) -> String {
    format!("OBS orphan recovery failed to run: {}", err)
}

pub fn failed_to_create_dod_directory(err: impl Display) -> String {
    format!("Failed to create dod directory: {}", err)
}

pub fn failed_to_patch_preview_demo(source_demo: &str, err: impl Display) -> String {
    format!("Failed to patch preview demo for {}: {}", source_demo, err)
}

pub fn failed_to_write_preview_sidecar(err: impl Display) -> String {
    format!("Failed to write preview sidecar: {}", err)
}

pub fn failed_to_launch_hlae_for_preview(err: impl Display) -> String {
    format!("Failed to launch HLAE for preview: {}", err)
}

pub fn failed_to_launch_hlae(err: impl Display) -> String {
    format!("Failed to launch HLAE: {}", err)
}

pub fn failed_to_launch_obs(err: impl Display) -> String {
    format!("Failed to launch OBS: {}", err)
}

pub fn game_directory_not_found(game_dir: &str) -> String {
    format!("Game directory not found: {}", game_dir)
}

pub fn failed_to_read_dod_directory(err: impl Display) -> String {
    format!("Failed to read dod directory: {}", err)
}

// ── render_manager.rs ────────────────────────────────────────────────────

pub const RENDER_BATCH_ALREADY_RUNNING_LONG: &str = "A render batch is already running.";
pub const BATCH_ALREADY_QUEUED: &str = "A batch is already queued — start it or cancel it before scanning again.";
pub const RENDER_BATCH_ALREADY_IN_PROGRESS: &str = "Render batch already in progress";
pub const NOTHING_QUEUED_TO_RENDER: &str = "Nothing queued to render — scan for takes first.";
pub const SKIP_ONLY_FOR_OBS_TAKE: &str = "Skip (keep original) is only available for a captured OBS take (its own audio, not a HUD/alpha clip).";

pub fn no_such_job(job_id: &str) -> String {
    format!("No such job: {}", job_id)
}

pub fn job_not_queued(job_id: &str, status: impl Display) -> String {
    format!("Job {} is {} — only a Queued job's codec can be changed", job_id, status)
}

pub fn job_still_rendering(job_id: &str) -> String {
    format!("Job {} is still rendering — cancel it first", job_id)
}

// ── audit_manager.rs ─────────────────────────────────────────────────────

pub fn failed_to_delete_audit_file(path: &str, err: impl Display) -> String {
    format!("Failed to delete {}: {}", path, err)
}

pub fn path_no_longer_exists(path: &str) -> String {
    format!("Path no longer exists: {}", path)
}

pub fn failed_to_open_explorer(err: impl Display) -> String {
    format!("Failed to open explorer: {}", err)
}

pub fn failed_to_open_finder(err: impl Display) -> String {
    format!("Failed to open Finder: {}", err)
}

pub const NO_PARENT_DIRECTORY_FOR_PATH: &str = "No parent directory for path";

pub fn failed_to_open_folder(err: impl Display) -> String {
    format!("Failed to open folder: {}", err)
}

// ── map_manager.rs ───────────────────────────────────────────────────────

pub fn no_map_folder_beside_exe(game_path: &str) -> String {
    format!(
        "no map folder beside `{}` — maps are expected at `<hl.exe folder>/dod/maps`",
        game_path
    )
}

pub fn map_check_failed(err: impl Display) -> String {
    format!("map check failed: {}", err)
}

pub fn config_scan_failed(err: impl Display) -> String {
    format!("config scan failed: {}", err)
}

pub fn map_download_failed(err: impl Display) -> String {
    format!("map download failed: {}", err)
}

// ── updater_manager.rs ───────────────────────────────────────────────────

pub fn unknown_update_channel(channel: &str) -> String {
    format!("Unknown update channel: {}", channel)
}

pub fn invalid_updater_endpoint_url(err: impl Display) -> String {
    format!("Invalid updater endpoint URL: {}", err)
}

pub fn failed_to_set_updater_endpoint(err: impl Display) -> String {
    format!("Failed to set updater endpoint: {}", err)
}

pub fn failed_to_build_updater(err: impl Display) -> String {
    format!("Failed to build updater: {}", err)
}

pub const NO_UPDATE_AVAILABLE_TO_INSTALL: &str = "No update available to install — call check_for_update first";

// ── settings_manager.rs ──────────────────────────────────────────────────

pub fn failed_to_serialize_settings(err: impl Display) -> String {
    format!("Failed to serialize settings: {}", err)
}

pub fn failed_to_write_settings_file(path: impl std::fmt::Debug, err: impl Display) -> String {
    format!("Failed to write settings file {:?}: {}", path, err)
}
