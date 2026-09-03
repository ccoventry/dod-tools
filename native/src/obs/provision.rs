//! Auto-provisions and repairs dod-tools' own OBS profile/scene/sources.
//!
//! **Why a dedicated, dod-tools-owned profile and scene rather than letting
//! the user point at their own.** An earlier design had scene/profile pickers
//! — dropdowns over whatever the user already had — and switched into
//! whichever one was chosen for the batch. That still left the batch
//! rewriting *that* profile's canvas/output resolution and recording
//! directory out from under it, which is exactly the kind of clobbering
//! `cfg_scan.rs`'s "detect and warn, never write" policy exists to prevent
//! for the game's own `.cfg` files. A profile/scene dod-tools creates and
//! names itself sidesteps the whole problem: nothing it writes here can ever
//! touch a setting or source the user actually built for something else.
//!
//! **Re-verified and repaired on every connect, not created once.** Nothing
//! stops a user opening the dod-tools profile/scene in OBS's own UI and
//! poking around — a setting drifting back is corrected the next time this
//! runs rather than trusted to have stayed put.
//!
//! **The window target.** `HL_WINDOW` was read directly off a real, working
//! OBS scene collection (`%APPDATA%\obs-studio\basic\scenes\*.json`) rather
//! than guessed — the window class in particular (`SDL_app`) is not
//! discoverable from documentation. Game/Window Capture sources re-resolve
//! their target on every tick, which is why this can be set before hl.exe has
//! ever launched: it's the same mechanism that lets a source configured once
//! keep finding the game across every later relaunch.

use serde_json::json;

use super::client::{ObsClient, ObsError};
use crate::log_markdown;

/// The profile dod-tools owns. Never the user's own — see module docs.
pub const PROFILE_NAME: &str = "[DoD-Tools]";
/// The scene dod-tools owns, inside whatever scene collection is current.
/// Deliberately not a dedicated scene collection of its own — a collection is
/// the coarser "which show am I running" concept; adding one scene to
/// whatever the user already has open is the smaller, less disruptive move.
pub const SCENE_NAME: &str = "[DoD-Tools] Capture";
const GAME_CAPTURE_SOURCE: &str = "Day of Defeat Game Capture";
const GAME_AUDIO_SOURCE: &str = "Day of Defeat Game Audio";
/// `title:class:executable` — DoD 1.3's hl.exe window, confirmed empirically
/// (see module docs). If a future engine/launcher build changes any of the
/// three, this stops matching and needs re-capturing the same way.
const HL_WINDOW: &str = "Day of Defeat:SDL_app:hl.exe";

/// What was active before provisioning switched it, so a caller running a
/// real batch (as opposed to just checking the connection) can restore it
/// once the batch ends. `None` means "already on it" — nothing to restore.
pub struct ProvisionResult {
    pub previous_profile: Option<String>,
    pub previous_scene: Option<String>,
}

/// Ensures the dod-tools-owned profile, scene, sources, video settings and
/// mute state all exist and are correct — creating anything missing,
/// repairing anything drifted — then switches into the profile and scene.
///
/// `width`/`height`/`fps_num`/`fps_den` are the game's own capture
/// resolution and the OBS-mode output rate (the same "Capture FPS" field
/// frame-sequence mode uses) — canvas and output both get set to the game's
/// resolution, avoiding the double-scale `ObsClient::preflight` otherwise
/// warns about.
///
/// Callers must call `ObsClient::refuse_if_busy` first — this function
/// assumes that's already been checked and switches live state
/// unconditionally.
pub fn ensure_dod_tools_setup(
    client: &mut ObsClient,
    width: i32,
    height: i32,
    fps_num: i32,
    fps_den: i32,
) -> Result<ProvisionResult, ObsError> {
    let previous_profile = ensure_profile(client)?;
    client.set_video_settings(width, height, fps_num, fps_den)?;
    let previous_scene = ensure_scene_and_sources(client, width, height)?;
    // Best-effort: muting a renamed or removed default input should not fail
    // provisioning over — see set_input_mute's own doc comment.
    let _ = client.set_input_mute("Desktop Audio", true);
    let _ = client.set_input_mute("Mic/Aux", true);
    Ok(ProvisionResult { previous_profile, previous_scene })
}

fn ensure_profile(client: &mut ObsClient) -> Result<Option<String>, ObsError> {
    let (profiles, current) = client.profile_list()?;
    if !profiles.iter().any(|p| p == PROFILE_NAME) {
        client.create_profile(PROFILE_NAME)?;
        log_markdown(&format!(
            "🎬 **OBS** — created the `{PROFILE_NAME}` profile (first run on this OBS install)."
        ));
    }
    if current == PROFILE_NAME {
        return Ok(None);
    }
    client.set_profile(PROFILE_NAME)?;
    log_markdown(&format!(
        "🎬 **OBS** — switched to profile `{PROFILE_NAME}` (restored afterwards)."
    ));
    Ok(Some(current))
}

fn ensure_scene_and_sources(
    client: &mut ObsClient,
    width: i32,
    height: i32,
) -> Result<Option<String>, ObsError> {
    let scenes = client.scene_names()?;
    if !scenes.iter().any(|s| s == SCENE_NAME) {
        client.create_scene(SCENE_NAME)?;
        log_markdown(&format!(
            "🎬 **OBS** — created the `{SCENE_NAME}` scene (first run in this scene collection)."
        ));
    }

    ensure_game_capture_source(client, width, height)?;
    ensure_game_audio_source(client)?;

    let current = client.current_scene()?;
    if current == SCENE_NAME {
        return Ok(None);
    }
    client.set_scene(SCENE_NAME)?;
    log_markdown(&format!(
        "🎬 **OBS** — switched to scene `{SCENE_NAME}` (restored afterwards)."
    ));
    Ok(Some(current))
}

fn ensure_game_capture_source(
    client: &mut ObsClient,
    width: i32,
    height: i32,
) -> Result<(), ObsError> {
    let settings = json!({
        "window": HL_WINDOW,
        "capture_mode": "window",
        "priority": 0,
        "anti_cheat_hook": false,
    });
    if client.input_names()?.iter().any(|i| i == GAME_CAPTURE_SOURCE) {
        client.set_input_settings(GAME_CAPTURE_SOURCE, settings)?;
    } else {
        client.create_input(SCENE_NAME, GAME_CAPTURE_SOURCE, "game_capture", settings)?;
    }
    // Best-effort: a canvas already matching the game's resolution (set just
    // above, in ensure_dod_tools_setup) makes this a no-op in practice, and a
    // source that was just freshly created is worth more than failing
    // provisioning over its bounding box specifically.
    if let Ok(item_id) = client.scene_item_id(SCENE_NAME, GAME_CAPTURE_SOURCE) {
        let _ = client.set_scene_item_bounds(SCENE_NAME, item_id, width as f64, height as f64);
    }
    Ok(())
}

fn ensure_game_audio_source(client: &mut ObsClient) -> Result<(), ObsError> {
    let settings = json!({
        "window": HL_WINDOW,
        "priority": 0,
    });
    if client.input_names()?.iter().any(|i| i == GAME_AUDIO_SOURCE) {
        client.set_input_settings(GAME_AUDIO_SOURCE, settings)?;
    } else {
        client.create_input(SCENE_NAME, GAME_AUDIO_SOURCE, "wasapi_process_output_capture", settings)?;
    }
    Ok(())
}
