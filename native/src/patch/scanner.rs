// patch/scanner.rs
// Life-bounded highlight scanner and HLTV detection.
// All three functions perform std::fs I/O — native-only.
// calculate_demo_tickrate is private to this module (used only by scan_demo_for_highlights).

use crate::patch::types::{HighlightRules, CaptureStreak};

// ── Tickrate auto-detection ───────────────────────────────────────────────────
// Walks the frame header stream from raw bytes to derive ticks-per-second.
// Private — only called by scan_demo_for_highlights.

fn calculate_demo_tickrate(bytes: &[u8]) -> Option<f32> {
    if bytes.len() < 544 {
        return None;
    }
    let original_offset = i32::from_le_bytes(bytes.get(540..544)?.try_into().ok()?) as usize;
    if original_offset > bytes.len() {
        return None;
    }

    let mut first_hdr = None;
    let mut last_hdr = None;
    let mut offset: usize = 544;

    while offset.checked_add(9)? <= original_offset {
        let type_byte = *bytes.get(offset)?;
        let time = f32::from_le_bytes(bytes.get(offset + 1..offset + 5)?.try_into().ok()?);
        let tick = i32::from_le_bytes(bytes.get(offset + 5..offset + 9)?.try_into().ok()?);

        if first_hdr.is_none() {
            first_hdr = Some((time, tick));
        }
        last_hdr = Some((time, tick));

        let next_offset = offset.checked_add(9)?;
        match type_byte {
            2 => {
                offset = next_offset;
            }
            3 => {
                offset = next_offset.checked_add(64)?;
            }
            4 => {
                offset = next_offset.checked_add(32)?;
            }
            5 => {
                break;
            }
            6 => {
                offset = next_offset.checked_add(84)?;
            }
            7 => {
                offset = next_offset.checked_add(8)?;
            }
            8 => {
                if next_offset.checked_add(8)? > original_offset { break; }
                let sample_len = u32::from_le_bytes(bytes.get(next_offset + 4..next_offset + 8)?.try_into().ok()?) as usize;
                offset = next_offset.checked_add(8)?.checked_add(sample_len)?.checked_add(16)?;
            }
            9 => {
                if next_offset.checked_add(4)? > original_offset { break; }
                let buffer_len = u32::from_le_bytes(bytes.get(next_offset..next_offset + 4)?.try_into().ok()?) as usize;
                offset = next_offset.checked_add(4)?.checked_add(buffer_len)?;
            }
            _ => {
                if next_offset.checked_add(468)? > original_offset { break; }
                let msg_len = u32::from_le_bytes(bytes.get(next_offset + 464..next_offset + 468)?.try_into().ok()?) as usize;
                offset = next_offset.checked_add(468)?.checked_add(msg_len)?;
            }
        }
    }

    if let (Some((t1, tk1)), Some((t2, tk2))) = (first_hdr, last_hdr) {
        if t2 > t1 && tk2 > tk1 {
            return Some((tk2 - tk1) as f32 / (t2 - t1));
        }
    }
    None
}

// ── HLTV guard ────────────────────────────────────────────────────────────────

pub fn is_hltv_demo(path: &std::path::Path) -> Result<bool, std::io::Error> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut header = vec![0u8; 512];
    file.read_exact(&mut header)?;

    let contains_hltv = header.windows(4).any(|window| window == b"HLTV");
    drop(file);
    Ok(contains_hltv)
}

// ── Life-bounded highlight scanner ───────────────────────────────────────────
// Reads player.kill_streaks directly from the completed Analysis — the authoritative,
// already-segmented output of use_kill_streak_updates (analysis/src/kill.rs).
// Segmentation boundaries (death, round reset, map change) are handled by the
// analysis crate, including the grenade kill-after-death edge case.

pub fn scan_demo_for_highlights(
    path: &std::path::Path,
    rules: &HighlightRules,
) -> Result<(f32, Vec<CaptureStreak>, bool, Option<usize>), String> {
    match is_hltv_demo(path) {
        Ok(true) => return Err("Unsupported HLTV proxy demo format".to_string()),
        Err(e) => return Err(format!("Failed to read demo header: {}", e)),
        _ => {}
    }

    let bytes = std::fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    let tickrate = calculate_demo_tickrate(&bytes).unwrap_or(100.0);

    let analysis = analysis::Analysis::try_from_bytes(&bytes)
        .map_err(|e| format!("Failed to parse demo: {}", e))?;

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
                    let tick = time.frame_index as i32;
                    let abs_time = time.real_offset.as_secs_f32();
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
            };
            streak.update_visuals();
            streaks.push(streak);
        }
    }

    let is_pov = analysis.demo_info.demo_type == "POV";
    let local_player_index = analysis.state.pov_player_index.map(|idx| idx as usize);

    Ok((tickrate, streaks, is_pov, local_player_index))
}
