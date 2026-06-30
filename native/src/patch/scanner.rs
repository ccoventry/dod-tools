// patch/scanner.rs
// Life-bounded highlight scanner and HLTV detection.
// All three functions perform std::fs I/O — native-only.

use crate::patch::types::{HighlightRules, CaptureStreak};

// ── HLTV guard ────────────────────────────────────────────────────────────────

pub fn is_hltv_demo(path: &std::path::Path) -> Result<bool, std::io::Error> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut header = vec![0u8; 512];
    file.read_exact(&mut header)?;

    if header.len() >= 512 {
        let hltv_proxy_name = b"HLTV Proxy";
        if header.windows(hltv_proxy_name.len()).any(|window| window == hltv_proxy_name) {
            return Ok(true);
        }
    }
    Ok(false)
}

// ── Life-bounded highlight scanner ───────────────────────────────────────────
// Reads player.kill_streaks directly from the completed Analysis — the authoritative,
// already-segmented output of use_kill_streak_updates (analysis/src/kill.rs).
// Segmentation boundaries (death, round reset, map change) are handled by the
// analysis crate, including the grenade kill-after-death edge case.

pub fn scan_demo_for_highlights(
    path: &std::path::Path,
    rules: &HighlightRules,
) -> Result<(f32, Vec<CaptureStreak>, bool, Option<usize>, i32), String> {
    match is_hltv_demo(path) {
        Ok(true) => return Err("Unsupported HLTV proxy demo format".to_string()),
        Err(e) => return Err(format!("Failed to read demo header: {}", e)),
        _ => {}
    }

    let bytes = std::fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;

    let analysis = analysis::Analysis::try_from_bytes(&bytes)
        .map_err(|e| format!("Failed to parse demo: {}", e))?;

    let mut frame_times: Vec<f32> = Vec::with_capacity(analysis.demo_info.playback_frames as usize);
    if bytes.len() >= 544 {
        let directory_offset = i32::from_le_bytes(bytes[540..544].try_into().unwrap()) as usize;
        let mut pos = 544;
        let end = if directory_offset > 0 && directory_offset <= bytes.len() { directory_offset } else { bytes.len() };
        while pos + 9 <= end {
            let type_byte = bytes[pos];
            if type_byte > 9 && type_byte != 255 {
                break;
            }
            if type_byte != 5 && type_byte != 255 {
                let time = f32::from_le_bytes(bytes[pos+1..pos+5].try_into().unwrap());
                frame_times.push(time);
            }
            pos += 9;
            match type_byte {
                0 | 1 => {
                    if pos + 468 > end { break; }
                    let len = i32::from_le_bytes(bytes[pos+464..pos+468].try_into().unwrap()) as usize;
                    pos += 468 + len;
                },
                2 | 5 | 255 => {},
                3 => pos += 64,
                4 => pos += 32,
                6 => pos += 84,
                7 => pos += 8,
                8 => {
                    if pos + 8 > end { break; }
                    let len = u32::from_le_bytes(bytes[pos+4..pos+8].try_into().unwrap()) as usize;
                    pos += 24 + len;
                },
                9 => {
                    if pos + 4 > end { break; }
                    let len = u32::from_le_bytes(bytes[pos..pos+4].try_into().unwrap()) as usize;
                    pos += 4 + len;
                },
                _ => break,
            }
        }
    }
    let frame_times_arc = std::sync::Arc::new(frame_times);

    let mut tickrate = if analysis.demo_info.playback_time > 0.0 {
        analysis.demo_info.playback_frames as f32 / analysis.demo_info.playback_time
    } else {
        100.0
    };
    
    // Fallback if the demo header has garbage values
    if !tickrate.is_normal() || tickrate < 10.0 || tickrate > 1000.0 {
        tickrate = 100.0;
    }

    let min_kills = rules.min_kills.unwrap_or(1);
    let mut streaks: Vec<CaptureStreak> = Vec::new();
    let target_players_lower: Vec<String> = rules.target_players.iter()
        .map(|s| s.to_lowercase())
        .collect();

    // ── Per-player life-bounded streak iteration ──────────────────────────────
    for player in &analysis.state.players {
        // Phase D — target player filter applied at the player level, not per-kill.
        if !target_players_lower.is_empty() {
            let name_lower = player.name.to_lowercase();
            if !target_players_lower.iter().any(|t| name_lower.contains(t)) {
                continue;
            }
        }

        // Skip players that are not (or are no longer) in a connected slot.
        // Disconnected entries have no valid client_id to anchor the patcher.
        let player_index = match player.connection {
            analysis::Connection::Connected { client_id } => client_id as usize,
            _ => continue,
        };

        for kill_streak in &player.kill_streaks {
            if kill_streak.kills.len() < min_kills {
                continue;
            }

            // Phase B — resolve tick and abs_time directly from GameTime.
            // frame_index: the 1-based frame counter captured during use_timing_updates.
            // real_offset: wall-clock Duration used by update_visuals for timeline strings.
            let kills_raw: Vec<(i32, f32, String)> = kill_streak.kills.iter()
                .map(|(time, weapon, _victim)| {
                    let abs_time = time.real_offset.as_secs_f32();
                    let tick = time.frame_index as i32;
                    (tick, abs_time, format!("{:?}", weapon))
                })
                .collect();

            let end_index = kills_raw.len().saturating_sub(1);
            let mut streak = CaptureStreak {
                start_tick: kills_raw[0].0,
                end_tick: kills_raw[end_index].0,
                source_demo: path.to_string_lossy().to_string(),
                target_player: Some(player.name.clone()),
                kill_count: kills_raw.len(),
                timeline_string: String::new(),
                duration_string: String::new(),
                player_index,
                kills: kills_raw,
                start_index: 0,
                end_index,
                total_demo_frames: analysis.demo_info.playback_frames,
                demo_fps: tickrate,
                frame_times: frame_times_arc.clone(),
            };
            streak.update_visuals();
            streaks.push(streak);
        }
    }

    let local_player_index = analysis.state.pov_player_index.map(|idx| idx as usize);

    Ok((
        tickrate,
        streaks,
        analysis.demo_info.demo_type == "POV",
        local_player_index,
        analysis.demo_info.playback_frames,
    ))
}
