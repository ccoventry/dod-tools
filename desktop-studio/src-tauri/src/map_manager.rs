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
use crate::capture_manager::CustomCommandPayload;
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
    map_check::maps_dir_for_exe(exe).ok_or_else(|| crate::messages::no_map_folder_beside_exe(game_path))
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
    .map_err(crate::messages::map_check_failed)
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

/// Scheduled commands in the order the engine reaches them.
///
/// Everything set before the highlight runs first — larger offsets are further
/// back, so they come earlier — then everything set after it, nearest first.
/// Which one is first matters: only the first command to touch a cvar displaces
/// what the configs and init commands left it at. The rest displace each other.
fn application_order(commands: &[CustomCommandPayload]) -> Vec<&CustomCommandPayload> {
    let is_after = |c: &CustomCommandPayload| c.relation == "After";
    let mut before: Vec<&CustomCommandPayload> =
        commands.iter().filter(|c| !is_after(c)).collect();
    before.sort_by(|a, b| {
        b.offset_seconds
            .partial_cmp(&a.offset_seconds)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut after: Vec<&CustomCommandPayload> = commands.iter().filter(|c| is_after(c)).collect();
    after.sort_by(|a, b| {
        a.offset_seconds
            .partial_cmp(&b.offset_seconds)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    before.into_iter().chain(after).collect()
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
    custom_commands: Vec<CustomCommandPayload>,
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
        let command_texts: Vec<String> =
            custom_commands.iter().map(|c| c.command.clone()).collect();
        // Every scheduled `r_decals` breaks the flush, however many there are,
        // so the hazard list is not deduplicated the way the overrides are.
        for (cvar, command) in native::patch::cfg_scan::mid_demo_hazards(&command_texts) {
            custom.push(CustomCommandWarning {
                command,
                cvar,
                kind: "hazard".to_string(),
                replaced_value: String::new(),
                source: String::new(),
            });
        }

        // Only the FIRST scheduled command to touch a cvar displaces the config
        // or init value. A paired set — `hud_deathnotice_time 555` before the
        // clip and `1` after it — is one override and one restore, and
        // reporting the restore against the config file too says the same thing
        // twice while describing the second one wrongly.
        let mut already_reported: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for payload in application_order(&custom_commands) {
            let command = &payload.command;
            let Some((cvar, _)) = native::patch::cfg_scan::assigned_cvar(command) else {
                continue;
            };
            if custom.iter().any(|w| w.command == command.trim()) {
                continue;
            }
            if !already_reported.insert(cvar.to_lowercase()) {
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
    .map_err(crate::messages::config_scan_failed)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RollFloorReport {
    pub pre_roll: f32,
    pub pre_roll_floor: f32,
    pub pre_roll_binding: String,
    pub post_roll: f32,
    pub post_roll_floor: f32,
    pub post_roll_binding: String,
    pub audio_resync: f32,
    pub sound_flush: f32,
    pub flush_lead: f32,
    pub scheduled_before: f32,
    pub scheduled_after: f32,
}

/// What the pre-roll and post-roll have to cover for this configuration.
///
/// The rolls stopped being a matter of taste once the audio resync, the sound
/// flush, the decal sweep's lead and the Scheduled Command offsets all started
/// measuring against them. Reported rather than enforced: the terms are
/// knowable, the engine's audio guidance is a 2-4s range rather than a
/// constant, and whether the decal burst itself needs real-time playback is
/// still unverified — so the number is advice, not a clamp.
#[tauri::command]
pub fn roll_floors(
    pre_roll: f32,
    post_roll: f32,
    record_start_lead: Option<f32>,
    record_stop_trail: Option<f32>,
    decal_flush: Option<bool>,
    custom_commands: Vec<CustomCommandPayload>,
) -> RollFloorReport {
    // The lead and trail matter: a scheduled offset anchors to the kill, and
    // recording starts a lead before that, so the lead already covers part of
    // the distance a Before command has to reach back.
    let mut cfg = native::patch::PatcherConfig {
        pre_roll_seconds: pre_roll,
        post_roll_seconds: post_roll,
        record_start_lead: record_start_lead.unwrap_or(0.0),
        record_stop_trail: record_stop_trail.unwrap_or(0.0),
        ..Default::default()
    };
    if let Some(v) = decal_flush {
        cfg.decal_flush = v;
    }
    cfg.custom_commands = custom_commands
        .iter()
        .map(|c| native::patch::CustomCommand {
            command: c.command.clone(),
            offset: c.offset_seconds,
            relation: match c.relation.as_str() {
                "After" => native::patch::CommandRelation::After,
                _ => native::patch::CommandRelation::Before,
            },
        })
        .collect();

    let f = native::patch::builder::roll_floors(&cfg);
    RollFloorReport {
        pre_roll,
        pre_roll_floor: f.pre_roll,
        pre_roll_binding: f.pre_roll_binding.to_string(),
        post_roll,
        post_roll_floor: f.post_roll,
        post_roll_binding: f.post_roll_binding.to_string(),
        audio_resync: f.audio_resync,
        sound_flush: f.sound_flush,
        flush_lead: f.flush_lead,
        scheduled_before: f.scheduled_before,
        scheduled_after: f.scheduled_after,
    }
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
    .map_err(crate::messages::map_download_failed)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_path_with_no_map_folder_says_so_rather_than_failing_later() {
        let err = maps_dir("Z:/nowhere/hl.exe").unwrap_err();
        assert!(err.contains("dod/maps"), "{}", err);
    }

    // ── Config warning report ────────────────────────────────────────────────
    //
    // These drive the real command rather than a extracted helper, so the wiring
    // is covered too: the scan, `final_init_commands`, and the assembly. Two
    // display bugs in this report were found by screenshot, which is the wrong
    // way to find them.

    /// A game folder as the engine expects it: `hl.exe` with `dod/` beside it,
    /// a `config.cfg` that execs `movie.cfg`, and the values that caused all
    /// this in `movie.cfg`.
    fn fake_game(tag: &str) -> String {
        let root = std::env::temp_dir().join(format!("dod_cfgrep_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dod = root.join("dod");
        std::fs::create_dir_all(&dod).unwrap();
        std::fs::write(&dod.join("config.cfg"), "bind \"F7\" \"r_decals 4000\"\nexec movie.cfg\n")
            .unwrap();
        std::fs::write(
            &dod.join("movie.cfg"),
            // hud_deathnotice_time is here because it is the cvar people
            // genuinely pair around a clip — raised before, restored after.
            "r_decals \"0\"\nmirv_movie_fps \"300\"\nhud_deathnotice_time \"10\"\nmirv_fov \"105\"\n",
        )
        .unwrap();
        let exe = root.join("hl.exe");
        std::fs::write(&exe, b"").unwrap();
        exe.to_string_lossy().to_string()
    }

    fn scheduled(command: &str, relation: &str, offset: f32) -> CustomCommandPayload {
        CustomCommandPayload {
            command: command.to_string(),
            relation: relation.to_string(),
            offset_seconds: offset,
        }
    }

    fn report_scheduled(tag: &str, custom: Vec<CustomCommandPayload>) -> CfgReport {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(scan_game_configs(
            fake_game(tag),
            Vec::new(),
            custom,
            Some(120),
            Some(false),
            Some(true),
        ))
        .unwrap()
    }

    #[test]
    fn a_set_and_restore_pair_is_one_override_not_two() {
        // The real shape: raise a cvar before the clip, put it back after.
        // Only the first one displaces what movie.cfg left it at — the second
        // displaces the first. Reporting both against the config file says the
        // same thing twice and describes the second one wrongly.
        let r = report_scheduled(
            "pair",
            vec![
                scheduled("hud_deathnotice_time 555", "Before", 10.0),
                scheduled("hud_deathnotice_time 1", "After", 5.0),
            ],
        );

        let rows: Vec<_> = r
            .custom
            .iter()
            .filter(|c| c.cvar.eq_ignore_ascii_case("hud_deathnotice_time"))
            .collect();
        assert_eq!(rows.len(), 1, "{:?}", r.custom);
        assert_eq!(rows[0].command, "hud_deathnotice_time 555", "the one that displaces");
    }

    #[test]
    fn the_earliest_before_command_is_the_one_that_displaces() {
        // Larger "Before" offsets are further back, so they run first.
        let r = report_scheduled(
            "order",
            vec![
                scheduled("hud_deathnotice_time 1", "Before", 2.0),
                scheduled("hud_deathnotice_time 555", "Before", 10.0),
            ],
        );

        let rows: Vec<_> = r
            .custom
            .iter()
            .filter(|c| c.cvar.eq_ignore_ascii_case("hud_deathnotice_time"))
            .collect();
        assert_eq!(rows.len(), 1, "{:?}", r.custom);
        assert_eq!(rows[0].command, "hud_deathnotice_time 555", "10s back runs before 2s back");
    }

    #[test]
    fn every_scheduled_r_decals_is_still_flagged_however_many_there_are() {
        // Deduplication is about which command displaces a config value. Each
        // scheduled r_decals breaks the flush on its own, so they all count.
        let r = report_scheduled(
            "hazards",
            vec![
                scheduled("r_decals 128", "Before", 5.0),
                scheduled("r_decals 4096", "After", 1.0),
            ],
        );

        assert_eq!(r.custom.iter().filter(|c| c.kind == "hazard").count(), 2, "{:?}", r.custom);
    }

    fn report(
        tag: &str,
        init: &[&str],
        custom: &[&str],
        fps: i32,
    ) -> CfgReport {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(scan_game_configs(
            fake_game(tag),
            init.iter().map(|s| s.to_string()).collect(),
            custom.iter().map(|s| scheduled(s, "Before", 2.0)).collect(),
            Some(fps),
            Some(false),
            Some(true),
        ))
        .unwrap()
    }

    #[test]
    fn a_cvar_only_a_config_sets_is_reported_as_unseen() {
        // Nothing names mirv_fov, so the engine renders at 105 and the app is
        // working from its own default. That is the silent case.
        let r = report("unseen", &[], &[], 120);

        assert!(
            r.unseen.iter().any(|u| u.cvar == "mirv_fov" && u.value == "105"),
            "{:?}",
            r.unseen
        );
        assert!(
            !r.unseen.iter().any(|u| u.cvar == "r_decals"),
            "the decal pin names r_decals, so it is not unseen: {:?}",
            r.unseen
        );
    }

    #[test]
    fn naming_a_cvar_at_the_config_s_own_value_still_counts_as_seeing_it() {
        // Regression: matching on differing values put a cvar the user HAD
        // typed into the "the app cannot see these" list.
        let r = report("agrees", &["mirv_fov 105"], &[], 120);

        assert!(
            !r.unseen.iter().any(|u| u.cvar == "mirv_fov"),
            "it is in Init Commands — it is seen: {:?}",
            r.unseen
        );
    }

    #[test]
    fn one_override_row_per_cvar_even_when_two_commands_set_it() {
        // Regression: typing the value the app also appends produced two rows
        // saying the identical thing, which reads as a bug rather than as two
        // facts.
        let r = report("dupe", &["mirv_movie_fps 120"], &[], 120);

        let fps_rows: Vec<_> = r
            .overrides
            .iter()
            .filter(|o| o.cvar.eq_ignore_ascii_case("mirv_movie_fps"))
            .collect();
        assert_eq!(fps_rows.len(), 1, "{:?}", r.overrides);
        assert_eq!(fps_rows[0].cfg_value, "300");
    }

    #[test]
    fn a_typed_command_the_app_overrides_is_reported_against_the_setting() {
        // The screenshot case: mirv_movie_fps 500 typed by hand, Capture FPS at
        // 120 appended after it. The typed value never applies.
        let r = report("shadow", &["mirv_movie_fps 500"], &[], 120);

        assert_eq!(r.shadowed.len(), 1, "{:?}", r.shadowed);
        assert_eq!(r.shadowed[0].shadowed_value, "500");
        assert_eq!(r.shadowed[0].winner_value, "120");
        assert!(r.shadowed[0].winner_from_app, "the app appended the winner");

        assert!(
            !r.overrides.iter().any(|o| o.command == "mirv_movie_fps 500"),
            "a command that never applies overrides nothing: {:?}",
            r.overrides
        );
    }

    #[test]
    fn a_scheduled_r_decals_is_reported_as_breaking_the_flush() {
        let r = report("hazard", &[], &["r_decals 128"], 120);

        let hazards: Vec<_> = r.custom.iter().filter(|c| c.kind == "hazard").collect();
        assert_eq!(hazards.len(), 1, "{:?}", r.custom);
        assert_eq!(hazards[0].cvar, "r_decals");
        assert_eq!(
            r.custom.iter().filter(|c| c.command == "r_decals 128").count(),
            1,
            "the hazard must not also be listed as an ordinary override"
        );
    }

    #[test]
    fn a_scheduled_command_is_reported_against_whatever_it_displaces() {
        // Scheduled commands run last of all, so they beat the init commands as
        // well as the configs.
        let r = report("custom", &["mirv_fov 90"], &["mirv_fov 130"], 120);

        let row = r
            .custom
            .iter()
            .find(|c| c.cvar.eq_ignore_ascii_case("mirv_fov"))
            .expect("reported");
        assert_eq!(row.kind, "overridesInit");
        assert_eq!(row.replaced_value, "90");
    }

    #[test]
    fn a_bound_key_in_a_config_is_never_treated_as_a_setting() {
        // config.cfg here binds F7 to `r_decals 4000`. Nothing happens until
        // someone presses F7, and warning about it would be noise.
        let r = report("bind", &[], &[], 120);

        assert!(
            !r.overrides.iter().any(|o| o.cfg_value == "4000"),
            "{:?}",
            r.overrides
        );
    }
}
