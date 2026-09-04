// messages.rs
// native's own error-string catalog — the same idea as
// desktop-studio/src-tauri/src/messages.rs (itself the counterpart to the
// frontend's strings.js), but crate-local: native has no direct UI of its
// own, but many of its `Result<T, String>` errors bubble straight through a
// Tauri command (desktop-studio's messages.rs wraps them with extra context,
// e.g. `failed_to_patch_preview_demo`, but the inner text — what actually
// went wrong — is authored here). Scoped to the modules desktop-studio's
// Tauri layer actually calls (patch::map_fetch, shared::hlae_ffmpeg,
// obs::session) — native's CLI probe binaries (native/src/bin/*) are never
// reachable from the app and are out of scope. Issue #33.

use std::fmt::Display;

// ── patch/map_fetch.rs ───────────────────────────────────────────────────

pub fn not_a_usable_map_name(map_name: &str) -> String {
    format!("`{}` is not a usable map name", map_name)
}

pub const MAP_MIRRORS_MUST_BE_HTTPS: &str = "map mirrors must be https";

/// Generic "<path or URL>: <underlying error>" — the shape every I/O/network
/// failure in this file reduces to once the specific operation is already
/// clear from surrounding context (a scratch-file write, a download, a
/// directory create). Six independently-authored copies of this exact
/// `format!("{}: {}", ...)` collapsed into one call.
pub fn labeled(label: impl Display, err: impl Display) -> String {
    format!("{}: {}", label, err)
}

pub fn served_unreadable_bsp(url: &str, err: impl Display) -> String {
    format!("what {} served is not a readable BSP: {}", url, err)
}

pub fn served_unparseable_map(url: &str, err: impl Display) -> String {
    format!("what {} served does not parse as a map: {}", url, err)
}

pub fn served_wrong_build(url: &str, got: u32, want: u32) -> String {
    format!(
        "{} served build {:08x}, but the demo needs {:08x} — not installing it",
        url, got, want
    )
}

pub fn could_not_move_existing_aside(target: impl Display, err: impl Display) -> String {
    format!("could not move the existing {} aside: {}", target, err)
}

pub fn url_returned_status(url: &str, status: impl Display) -> String {
    format!("{} returned {}", url, status)
}

// ── shared/hlae_ffmpeg.rs ────────────────────────────────────────────────

pub fn not_a_file(path: impl Display) -> String {
    format!("{} is not a file", path)
}

pub fn could_not_run(path: impl Display, err: impl Display) -> String {
    format!("could not run {}: {}", path, err)
}

pub fn did_not_respond_to_version_flag(path: impl Display) -> String {
    format!("{} did not respond to -version", path)
}

pub fn did_not_identify_as_ffmpeg(path: impl Display) -> String {
    format!("{} did not identify itself as FFmpeg", path)
}

pub fn reports_itself_as(path: impl Display, banner: &str) -> String {
    format!("{} is not FFmpeg — it reports itself as \"{}\"", path, banner)
}

// ── obs/session.rs ───────────────────────────────────────────────────────

/// Independently authored twice (`start_record`/`stop_record`'s
/// `ObsError::Transport` guard) for the same "no client, never connected"
/// state.
pub const NOT_CONNECTED_TO_OBS: &str = "not connected to OBS";

pub fn obs_reported_file_missing(recorded: impl Display) -> String {
    format!("OBS reported {} but no file is there", recorded)
}

pub fn could_not_move_after_retries(
    recorded: impl Display,
    target: impl Display,
    retry_secs: f64,
    last_err: impl Display,
) -> String {
    format!(
        "could not move {} to {} after {:.1}s of retries: {}",
        recorded, target, retry_secs, last_err
    )
}
