// map_manager.rs
// Whether the demos on screen can actually be played, and on the right map.
//
// Kept out of the scan pipeline deliberately. Scanning parses every frame of
// every demo and is the slow, threaded part of loading a folder; this reads 544
// bytes per demo and one map file per distinct map, so it runs after a scan
// without slowing it, and a failure here costs a badge rather than the folder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use native::patch::map_check::{self, MapStatus};
use native::patch::map_fetch;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapCheckRow {
    pub demo_path: String,
    pub demo_name: String,
    pub map_name: String,
    /// The build the demo was recorded on. Absent for HLTV demos, which do not
    /// record one.
    pub expected_checksum: Option<u32>,
    /// `ok` | `wrongBuild` | `missing` | `unverifiable` | `unreadableMap` |
    /// `unreadableDemo`
    pub state: String,
    pub detail: String,
    /// Whether the demo can be played at all.
    pub playable: bool,
}

/// Where maps live for a configured `hl.exe`, or an error a person can act on.
fn maps_dir(game_path: &str) -> Result<PathBuf, String> {
    let exe = Path::new(game_path);
    map_check::maps_dir_for_exe(exe).ok_or_else(|| {
        format!(
            "no map folder beside `{}` — maps are expected at `<hl.exe folder>/dod/maps`",
            game_path
        )
    })
}

/// Check a list of demos against the map library.
///
/// Map files are read once each however many demos want them, because the
/// checksum walks the whole file and a folder of 400 demos covers maybe twenty
/// maps.
#[tauri::command]
pub async fn check_demo_maps(
    demo_paths: Vec<String>,
    game_path: String,
) -> Result<Vec<MapCheckRow>, String> {
    let dir = maps_dir(&game_path)?;

    tokio::task::spawn_blocking(move || {
        let mut seen: HashMap<String, MapStatus> = HashMap::new();
        let mut rows = Vec::with_capacity(demo_paths.len());

        for path in demo_paths {
            let p = PathBuf::from(&path);
            let demo_name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());

            let reference = match map_check::map_reference(&p) {
                Ok(r) => r,
                Err(e) => {
                    rows.push(MapCheckRow {
                        demo_path: path,
                        demo_name,
                        map_name: String::new(),
                        expected_checksum: None,
                        state: "unreadableDemo".to_string(),
                        detail: e,
                        playable: false,
                    });
                    continue;
                }
            };

            // Keyed on name AND wanted build: two demos of the same map name
            // wanting different builds are genuinely different questions.
            let cache_key = format!(
                "{}:{}",
                reference.map_name,
                reference.expected_checksum.unwrap_or(0)
            );
            let status = seen
                .entry(cache_key)
                .or_insert_with(|| map_check::status_of(&reference, &dir))
                .clone();

            let state = match &status {
                MapStatus::Ok { .. } => "ok",
                MapStatus::WrongBuild { .. } => "wrongBuild",
                MapStatus::Missing => "missing",
                MapStatus::Unverifiable => "unverifiable",
                MapStatus::Unreadable { .. } => "unreadableMap",
            };

            rows.push(MapCheckRow {
                demo_path: path,
                demo_name,
                detail: status.summary(&reference.map_name),
                playable: status.is_playable(),
                map_name: reference.map_name,
                expected_checksum: reference.expected_checksum,
                state: state.to_string(),
            });
        }

        rows
    })
    .await
    .map_err(|e| format!("map check failed: {}", e))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfgWarningRow {
    pub cvar: String,
    pub value: String,
    pub file: String,
    pub line: usize,
}

/// What the game's own config files set that this pipeline reads.
///
/// Read-only, and advisory. Nothing in this app writes, edits or removes a
/// config file — the fix is for the user to make, either in their config or by
/// stating the value in Init Commands where the pipeline can see it.
#[tauri::command]
pub async fn scan_game_configs(game_path: String) -> Result<Vec<CfgWarningRow>, String> {
    let exe = PathBuf::from(&game_path);
    let Some(dir) = exe.parent().map(|p| p.join("dod")) else {
        return Ok(Vec::new());
    };
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    tokio::task::spawn_blocking(move || {
        native::patch::cfg_scan::scan(&dir)
            .effective_settings()
            .into_iter()
            .map(|s| CfgWarningRow {
                cvar: s.cvar.clone(),
                value: s.value.clone(),
                file: s.file_name(),
                line: s.line,
            })
            .collect()
    })
    .await
    .map_err(|e| format!("config scan failed: {}", e))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapFetchResult {
    pub map_name: String,
    pub installed_path: String,
    pub checksum: u32,
    pub bytes: u64,
    pub already_correct: bool,
    /// Where an existing file was moved to, when one was in the way. Nothing is
    /// ever overwritten in place.
    pub replaced_path: Option<String>,
}

/// The URL a map would be fetched from, so a prompt can show it before anything
/// reaches the network.
#[tauri::command]
pub fn map_download_url(map_name: String) -> Result<String, String> {
    map_fetch::map_url(map_fetch::DEFAULT_MIRROR, &map_name)
}

/// Download one map and install it, verified against the build the demo wants.
///
/// This writes into the user's game folder and talks to the network, so it is
/// only ever reached from an explicit action — never from a scan, and never as
/// a side effect of starting a capture.
#[tauri::command]
pub async fn download_map(
    map_name: String,
    expected_checksum: Option<u32>,
    game_path: String,
) -> Result<MapFetchResult, String> {
    let dir = maps_dir(&game_path)?;

    tokio::task::spawn_blocking(move || {
        map_fetch::fetch_map(
            &map_name,
            expected_checksum,
            &dir,
            map_fetch::DEFAULT_MIRROR,
        )
        .map(|o| MapFetchResult {
            map_name: o.map_name,
            installed_path: o.installed.to_string_lossy().to_string(),
            checksum: o.checksum,
            bytes: o.bytes,
            already_correct: o.already_correct,
            replaced_path: o.replaced.map(|p| p.to_string_lossy().to_string()),
        })
    })
    .await
    .map_err(|e| format!("map download failed: {}", e))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_path_with_no_map_folder_says_so_rather_than_failing_later() {
        let err = maps_dir("Z:/nowhere/hl.exe").unwrap_err();
        assert!(err.contains("dod/maps"), "{}", err);
    }
}
