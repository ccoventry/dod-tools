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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfgOverrideRow {
    pub command: String,
    pub cvar: String,
    pub init_value: String,
    pub cfg_value: String,
    pub file: String,
    pub line: usize,
    /// True when the command comes from the pipeline itself rather than from
    /// something the user typed — those override a config too, and the user has
    /// no other way to find out.
    pub from_app: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfgShadowRow {
    pub cvar: String,
    /// The command that will not take effect.
    pub shadowed: String,
    pub shadowed_value: String,
    pub winner_value: String,
    /// True when the winning command is one the pipeline appends for itself, in
    /// which case the fix is a setting rather than an edit to this list.
    pub winner_from_app: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomCommandWarning {
    pub command: String,
    pub cvar: String,
    /// `hazard` | `overridesInit` | `overridesConfig`
    pub kind: String,
    /// What this displaces, and where that came from.
    pub replaced_value: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CfgReport {
    /// Values the pipeline reads that a config sets and no init command names.
    pub unseen: Vec<CfgWarningRow>,
    /// Init commands that will win over a config's value.
    pub overrides: Vec<CfgOverrideRow>,
    /// Init commands beaten by a later entry in the same list.
    pub shadowed: Vec<CfgShadowRow>,
    /// Scheduled commands that displace something, or that must not run mid-demo.
    pub custom: Vec<CustomCommandWarning>,
}

/// What the game's own config files set, and what the app's init commands will
/// override.
///
/// Read-only, and advisory. Nothing in this app writes, edits or removes a
/// config file — the fix is always the user's to make.
#[tauri::command]
pub async fn scan_game_configs(
    game_path: String,
    init_commands: Vec<String>,
    custom_commands: Vec<String>,
    capture_fps: Option<i32>,
    separate_hud: Option<bool>,
    decal_flush: Option<bool>,
) -> Result<CfgReport, String> {
    let exe = PathBuf::from(&game_path);
    let Some(dir) = exe.parent().map(|p| p.join("dod")) else {
        return Ok(CfgReport::default());
    };
    if !dir.is_dir() {
        return Ok(CfgReport::default());
    }

    tokio::task::spawn_blocking(move || {
        let scan = native::patch::cfg_scan::scan(&dir);

        // The list the engine will actually receive, so the app's own additions
        // — the movie fps, the decal pin — are checked too.
        let mut cfg = native::patch::PatcherConfig {
            init_commands: init_commands.clone(),
            ..Default::default()
        };
        if let Some(v) = capture_fps {
            cfg.capture_fps = v;
        }
        if let Some(v) = separate_hud {
            cfg.separate_hud = v;
        }
        if let Some(v) = decal_flush {
            cfg.decal_flush = v;
        }
        let effective_commands = native::patch::final_init_commands(&cfg);
        let user_typed: std::collections::HashSet<String> =
            init_commands.iter().map(|c| c.trim().to_string()).collect();

        // A command that never applies cannot override anything, so the ones
        // beaten later in the list are dropped rather than reported twice with
        // opposite implications.
        let dead: std::collections::HashSet<String> =
            native::patch::cfg_scan::self_overrides(&effective_commands)
                .into_iter()
                .map(|s| s.shadowed)
                .collect();

        // One row per cvar. Typing `mirv_movie_fps 120` when the app appends the
        // same value produces two commands that both override movie.cfg, and
        // listing the identical consequence twice reads as a bug rather than as
        // two facts. Only the last one applies, so that is the one reported.
        let mut by_cvar: indexmap::IndexMap<String, CfgOverrideRow> = indexmap::IndexMap::new();
        for o in scan.overrides_in(&effective_commands) {
            if dead.contains(&o.command) {
                continue;
            }
            by_cvar.insert(
                o.cvar.to_lowercase(),
                CfgOverrideRow {
                    from_app: !user_typed.contains(&o.command),
                    file: o.file_name(),
                    command: o.command,
                    cvar: o.cvar,
                    init_value: o.init_value,
                    cfg_value: o.cfg_value,
                    line: o.line,
                },
            );
        }
        let overrides: Vec<CfgOverrideRow> = by_cvar.into_iter().map(|(_, v)| v).collect();

        // An init command later in the list beats an earlier one, so a value the
        // user typed can be dead on arrival without anything on screen saying
        // so — the pipeline appends its own commands after theirs.
        let user_count = init_commands.len();
        let shadowed = native::patch::cfg_scan::self_overrides(&effective_commands)
            .into_iter()
            .filter(|s| user_typed.contains(&s.shadowed))
            .map(|s| CfgShadowRow {
                winner_from_app: s.winner_index >= user_count,
                cvar: s.cvar,
                shadowed: s.shadowed,
                shadowed_value: s.shadowed_value,
                winner_value: s.winner_value,
            })
            .collect::<Vec<_>>();

        // Anything the pipeline reads that a config sets and no init command
        // even names — the genuinely silent case. Naming it at the same value
        // still counts as seeing it, so that is not reported as unseen.
        let named: std::collections::HashSet<String> = effective_commands
            .iter()
            .filter_map(|c| c.trim().split_whitespace().next().map(str::to_lowercase))
            .collect();
        let unseen = scan
            .effective_settings()
            .into_iter()
            .filter(|s| !named.contains(&s.cvar.to_lowercase()))
            .map(|s| CfgWarningRow {
                cvar: s.cvar.clone(),
                value: s.value.clone(),
                file: s.file_name(),
                line: s.line,
            })
            .collect();

        // Custom commands are scheduled into playback, so they run after the
        // configs AND after the init commands — they are the last word on any
        // cvar they touch, and the only place a value can change mid-demo.
        let mut custom = Vec::new();
        for (cvar, command) in native::patch::cfg_scan::mid_demo_hazards(&custom_commands) {
            custom.push(CustomCommandWarning {
                command,
                cvar,
                kind: "hazard".to_string(),
                replaced_value: String::new(),
                source: String::new(),
            });
        }
        for command in &custom_commands {
            let Some((cvar, _)) = native::patch::cfg_scan::assigned_cvar(command) else {
                continue;
            };
            if custom.iter().any(|w| w.command == command.trim()) {
                continue;
            }
            if let Some(existing) =
                native::patch::cfg_scan::effective_in(&effective_commands, &cvar)
            {
                custom.push(CustomCommandWarning {
                    command: command.trim().to_string(),
                    cvar,
                    kind: "overridesInit".to_string(),
                    replaced_value: existing,
                    source: String::new(),
                });
            } else if let Some(setting) = scan.effective(&cvar) {
                custom.push(CustomCommandWarning {
                    command: command.trim().to_string(),
                    cvar,
                    kind: "overridesConfig".to_string(),
                    replaced_value: setting.value.clone(),
                    source: format!("{}, line {}", setting.file_name(), setting.line),
                });
            }
        }

        CfgReport { unseen, overrides, shadowed, custom }
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
