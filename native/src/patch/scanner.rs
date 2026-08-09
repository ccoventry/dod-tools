// patch/scanner.rs
// Life-bounded highlight scanner and HLTV detection.
// All three functions perform std::fs I/O — native-only.

use crate::patch::types::{CaptureStreak, HighlightStatus};
use crate::patch::{MAX_PAYLOAD_LIMIT_BYTES, NETWORK_HEADER_ALIGNMENT, SCANNER_SECTION_BOUNDARY};

// ── HLTV guard ────────────────────────────────────────────────────────────────

pub fn is_hltv_demo(path: &std::path::Path) -> Result<bool, std::io::Error> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut header = [0_u8; crate::patch::HLTV_HEADER_SIZE];
    file.read_exact(&mut header)?;

    if header.len() >= crate::patch::HLTV_HEADER_SIZE {
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
) -> Result<(f32, Vec<CaptureStreak>, bool, Option<usize>, i32, Option<i32>, std::sync::Arc<Vec<f32>>), String> {
    match is_hltv_demo(path) {
        Ok(true) => return Err("Unsupported HLTV proxy demo format".to_string()),
        Err(e) => return Err(format!("Failed to read demo header: {}", e)),
        _ => {}
    }

    let bytes = std::fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;

    let analysis = analysis::Analysis::try_from_bytes(&bytes)
        .map_err(|e| format!("Failed to parse demo: {}", e))?;

    let mut frame_times: Vec<f32> = Vec::with_capacity(analysis.demo_info.playback_frames as usize);
    if bytes.len() >= crate::patch::DEMO_HEADER_SIZE {
        let directory_offset = i32::from_le_bytes(bytes[crate::patch::DIRECTORY_OFFSET_POS..crate::patch::DEMO_HEADER_SIZE].try_into().unwrap()) as usize;
        let mut pos = crate::patch::DEMO_HEADER_SIZE;
        let end = if directory_offset > 0 && directory_offset <= bytes.len() { directory_offset } else { bytes.len() };
        while pos + crate::patch::FRAME_HEADER_SIZE <= end {
            let type_byte = bytes[pos];
            if type_byte > 9 && type_byte != 255 {
                break;
            }
            if type_byte == SCANNER_SECTION_BOUNDARY {
                break;
            }
            if type_byte != 255 {
                let time = f32::from_le_bytes(bytes[pos+1..pos+5].try_into().unwrap());
                frame_times.push(time);
            }
            pos += crate::patch::FRAME_HEADER_SIZE;
            match type_byte {
                0 | 1 => {
                    let total_fixed_size = NETWORK_HEADER_ALIGNMENT;
                    if pos + total_fixed_size > end { break; }
                    let len = i32::from_le_bytes(bytes[pos+crate::patch::NETMSG_INFO_SIZE..pos+total_fixed_size].try_into().unwrap()) as usize;
                    if len > MAX_PAYLOAD_LIMIT_BYTES {
                        return Err(format!("Scanner alignment lost! Read impossible packet size: {} bytes at pos {}", len, pos));
                    }
                    pos += total_fixed_size + len;
                },
                2 | 255 => {},
                3 => pos += crate::patch::CMD_FRAME_SIZE,
                4 => pos += crate::patch::CLIENT_DATA_FRAME_SIZE,
                6 => pos += crate::patch::EVENT_FRAME_SIZE,
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
    let final_demo_frames = if analysis.demo_info.playback_frames > 0 {
        analysis.demo_info.playback_frames
    } else if !frame_times.is_empty() {
        frame_times.len() as i32
    } else {
        0
    };
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

    let mut streaks: Vec<CaptureStreak> = Vec::new();

    // ── Per-player life-bounded streak iteration ────────────────────────────────────────────
    for player in &analysis.state.players {

        // Skip players that are not (or are no longer) in a connected slot.
        // Disconnected entries have no valid client_id to anchor the patcher.
        let player_index = match player.connection {
            analysis::Connection::Connected { client_id } => client_id as usize,
            _ => continue,
        };

        for kill_streak in &player.kill_streaks {

            let kills_raw: Vec<(i32, f32, String)> = kill_streak.kills.iter()
                .map(|(time, weapon, _victim)| {
                    let abs_time = time.real_offset.as_secs_f32();
                    let tick = time.frame_index as i32;
                    let weapon_str = match weapon {
                        analysis::Weapon::Unknown => crate::analysis::translate_key("weapon.unknown"),
                        analysis::Weapon::Kabar => crate::analysis::translate_key("weapon.kabar"),
                        analysis::Weapon::GermanKnife => crate::analysis::translate_key("weapon.germanknife"),
                        analysis::Weapon::M1911 => crate::analysis::translate_key("weapon.m1911"),
                        analysis::Weapon::Luger => crate::analysis::translate_key("weapon.luger"),
                        analysis::Weapon::Garand => crate::analysis::translate_key("weapon.garand"),
                        analysis::Weapon::ScopedK98 => crate::analysis::translate_key("weapon.scopedk98"),
                        analysis::Weapon::Thompson => crate::analysis::translate_key("weapon.thompson"),
                        analysis::Weapon::Stg44 => crate::analysis::translate_key("weapon.stg44"),
                        analysis::Weapon::Springfield => crate::analysis::translate_key("weapon.springfield"),
                        analysis::Weapon::K98 => crate::analysis::translate_key("weapon.k98"),
                        analysis::Weapon::Bar => crate::analysis::translate_key("weapon.bar"),
                        analysis::Weapon::Mp40 => crate::analysis::translate_key("weapon.mp40"),
                        analysis::Weapon::Mk2Grenade => crate::analysis::translate_key("weapon.mk2grenade"),
                        analysis::Weapon::StickGrenade => crate::analysis::translate_key("weapon.stickgrenade"),
                        analysis::Weapon::Mg42 => crate::analysis::translate_key("weapon.mg42"),
                        analysis::Weapon::Browning30Cal => crate::analysis::translate_key("weapon.browning30cal"),
                        analysis::Weapon::Spade => crate::analysis::translate_key("weapon.spade"),
                        analysis::Weapon::M1Carbine => crate::analysis::translate_key("weapon.m1carbine"),
                        analysis::Weapon::Mg34 => crate::analysis::translate_key("weapon.mg34"),
                        analysis::Weapon::GreaseGun => crate::analysis::translate_key("weapon.greasegun"),
                        analysis::Weapon::Fg42 => crate::analysis::translate_key("weapon.fg42"),
                        analysis::Weapon::K43 => crate::analysis::translate_key("weapon.k43"),
                        analysis::Weapon::LeeEnfield => crate::analysis::translate_key("weapon.leeenfield"),
                        analysis::Weapon::Sten => crate::analysis::translate_key("weapon.sten"),
                        analysis::Weapon::Bren => crate::analysis::translate_key("weapon.bren"),
                        analysis::Weapon::Webley => crate::analysis::translate_key("weapon.webley"),
                        analysis::Weapon::Bazooka => crate::analysis::translate_key("weapon.bazooka"),
                        analysis::Weapon::Panzerschreck => crate::analysis::translate_key("weapon.panzerschreck"),
                        analysis::Weapon::Piat => crate::analysis::translate_key("weapon.piat"),
                        analysis::Weapon::Mortar => crate::analysis::translate_key("weapon.mortar"),
                        analysis::Weapon::ScopedFg42 => crate::analysis::translate_key("weapon.scopedfg42"),
                        analysis::Weapon::M1A1Carbine => crate::analysis::translate_key("weapon.m1a1carbine"),
                        analysis::Weapon::K98Bayonet => crate::analysis::translate_key("weapon.k98bayonet"),
                        analysis::Weapon::ScopedLeeEnfield => crate::analysis::translate_key("weapon.scopedleeenfield"),
                        analysis::Weapon::MillsBomb => crate::analysis::translate_key("weapon.millsbomb"),
                        analysis::Weapon::BritishKnife => crate::analysis::translate_key("weapon.britishknife"),
                        analysis::Weapon::ButtStock => crate::analysis::translate_key("weapon.buttstock"),
                        analysis::Weapon::EnfieldBayonet => crate::analysis::translate_key("weapon.enfieldbayonet"),
                    };
                    (tick, abs_time, weapon_str)
                })
                .collect();

            if kills_raw.is_empty() {
                continue;
            }

            let viewdemo_times: Vec<f32> = kill_streak.kills.iter()
                .map(|(time, _, _)| time.viewdemo_offset.as_secs_f32())
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
                viewdemo_times,
                start_index: 0,
                end_index,
                total_demo_frames: final_demo_frames,
                demo_fps: tickrate,
                frame_times: frame_times_arc.clone(),
                status: HighlightStatus::None,
                match_start_tick: analysis.state.match_start_tick,
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
        analysis.state.match_start_tick,
        frame_times_arc,
    ))
}
