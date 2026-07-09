// patch/builder.rs
// Batch job construction and the legacy channel-based worker spawner.
// Calls std::fs::create_dir_all and std::thread::spawn — native-only.

use std::sync::{Arc, atomic::AtomicBool};
use crate::patch::types::{
    CaptureStreak, PatchJob, PatcherConfig, CommandRelation,
    CaptureWorker, PatchEvent,
};
use crate::patch::engine::StreamPatcher;

// ── Frame-time helpers ───────────────────────────────────────────────────────

/// Walk backwards from `start_frame` (0-indexed) through `frame_times` until
/// `gap_seconds` of real demo time has been accumulated. Returns the 0-indexed
/// frame where that time boundary is reached. Clamps to frame 0 if the gap
/// exceeds the available history before the start frame.
fn find_tick_backwards(start_frame: usize, gap_seconds: f32, frame_times: &[f32]) -> i32 {
    if frame_times.is_empty() || gap_seconds <= 0.0 {
        return start_frame as i32;
    }
    let anchor_time = frame_times.get(start_frame).copied().unwrap_or(0.0);
    let target_time = anchor_time - gap_seconds;
    // Walk backwards until we cross target_time
    let mut frame = start_frame;
    while frame > 0 {
        frame -= 1;
        if frame_times[frame] <= target_time {
            return frame as i32;
        }
    }
    0
}

/// Walk forwards from `start_frame` (0-indexed) through `frame_times` until
/// `gap_seconds` of real demo time has accumulated. Returns the 0-indexed
/// frame where that time boundary is reached. Clamps to the last valid frame
/// if the end of the array is reached before the gap is satisfied.
fn find_tick_forwards(start_frame: usize, gap_seconds: f32, frame_times: &[f32]) -> i32 {
    if frame_times.is_empty() || gap_seconds <= 0.0 {
        return start_frame as i32;
    }
    let anchor_time = frame_times.get(start_frame).copied().unwrap_or(0.0);
    let target_time = anchor_time + gap_seconds;
    let last = frame_times.len().saturating_sub(1);
    let mut frame = start_frame;
    while frame < last {
        frame += 1;
        if frame_times[frame] >= target_time {
            return frame as i32;
        }
    }
    last as i32
}

const LOG_TAG: &str = "[dod-tools]";

fn build_safe_echos(tick: i32, message: &str) -> Vec<(i32, String)> {
    let mut result = Vec::new();
    let mut current_tick = tick;
    
    let mut words: Vec<&str> = message.split(' ').collect();
    if words.is_empty() {
        return result;
    }
    
    let mut current_chunk = String::new();
    let mut is_first = true;
    
    let mut i = 0;
    while i < words.len() {
        let word = words[i];
        let prefix = if is_first {
            format!("{} ", LOG_TAG)
        } else {
            "[dodtools] ->".to_string()
        };
        
        let test_message = if current_chunk.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", current_chunk, word)
        };
        
        let full_string = format!("{}{}", prefix, test_message);
        
        if full_string.len() > 55 {
            if current_chunk.is_empty() {
                let limit = 55_usize.saturating_sub(prefix.len());
                let (part1, part2) = word.split_at(limit.min(word.len()));
                
                let cmd = format!("echo \"{}{}\"", prefix, part1);
                result.push((current_tick, cmd));
                current_tick += 1;
                
                is_first = false;
                words[i] = part2;
                continue;
            } else {
                let cmd = format!("echo \"{}{}\"", prefix, current_chunk);
                result.push((current_tick, cmd));
                current_tick += 1;
                
                current_chunk.clear();
                is_first = false;
                continue;
            }
        } else {
            current_chunk = test_message;
            i += 1;
        }
    }
    
    if !current_chunk.is_empty() {
        let prefix = if is_first {
            format!("{} ", LOG_TAG)
        } else {
            "[dodtools] ->".to_string()
        };
        let cmd = format!("echo \"{}{}\"", prefix, current_chunk);
        result.push((current_tick, cmd));
    }
    
    result
}


// ── Batch queue builder ───────────────────────────────────────────────────────

pub fn build_batch_queue(raw_streaks: Vec<CaptureStreak>, config: &PatcherConfig) -> Result<Vec<PatchJob>, std::io::Error> {
    // tickrate is extracted dynamically from streaks per-demo
    let mut grouped: std::collections::HashMap<(String, Option<String>), Vec<CaptureStreak>> = std::collections::HashMap::new();
    for streak in raw_streaks {
        grouped.entry((streak.source_demo.clone(), streak.target_player.clone())).or_default().push(streak);
    }

    // Sort grouped chronologically by the start_tick of their first streak
    let mut sorted_groups: Vec<_> = grouped.into_iter().collect();
    sorted_groups.sort_by_key(|(_, streaks)| streaks.iter().map(|s| s.start_tick).min().unwrap_or(0));

    let mut jobs = Vec::new();
    let total_jobs = sorted_groups.len();
    
    let date_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut helper_cfg_content = String::new();

    let game_path_buf = std::path::PathBuf::from(&config.game_path);
    let dod_dir = match game_path_buf.parent() {
        Some(parent) => parent.join("dod"),
        None => std::path::PathBuf::from("dod"),
    };

    // Remove stale config from dod_dir
    let _ = std::fs::remove_file(dod_dir.join("dodtools_helper.cfg"));
    let _ = std::fs::remove_file(dod_dir.join("dodtools_capture_done.cfg"));
    let _ = std::fs::remove_file(dod_dir.join("dod_quit.cfg"));
    if let Ok(entries) = std::fs::read_dir(&dod_dir) {
        for entry in entries.flatten() {
            let filename = entry.file_name().to_string_lossy().to_string();
            if filename.starts_with("dodtools_chain_") && filename.ends_with(".cfg") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    
    // Remove stale config from output_dir if configured
    if let Some(ref out_dir) = config.output_dir {
        let _ = std::fs::remove_file(out_dir.join("dodtools_helper.cfg"));
        let _ = std::fs::remove_file(out_dir.join("dodtools_capture_done.cfg"));
        let _ = std::fs::remove_file(out_dir.join("dod_quit.cfg"));
        
        if let Ok(entries) = std::fs::read_dir(out_dir) {
            for entry in entries.flatten() {
                let filename = entry.file_name().to_string_lossy().to_string();
                if filename.starts_with("dodtools_chain_") && filename.ends_with(".cfg") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
    
    let active_export_dir = config.primary_media_dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let session_dir = if !config.session_id.is_empty() {
        active_export_dir.join(&config.session_id)
    } else {
        active_export_dir
    };
    if !session_dir.exists() {
        let _ = std::fs::create_dir_all(&session_dir);
    }
    
    helper_cfg_content.push_str(&format!(
        "# dodtools_helper.cfg\n# Created by: dod_tools.exe v{}\n# Date: {}\n\n",
        crate::VERSION,
        date_time
    ));
    
    helper_cfg_content.push_str("# Global aliases\n");
    helper_cfg_content.push_str("alias sys_normal_speed \"fps_max 100; host_framerate 0\"\n");
    let safe_ff_speed = config.fast_forward_speed.min(0.05);
    helper_cfg_content.push_str(&format!("alias sys_fast_forward \"fps_override 1; fps_max 1000; host_framerate {}\"\n", safe_ff_speed));
    helper_cfg_content.push_str("alias sys_sound \"stopsound\"\n");
    helper_cfg_content.push_str("alias sys_record_start \"mirv_recordmovie_start; stopsound\"\n");
    helper_cfg_content.push_str("alias sys_record_stop \"mirv_recordmovie_stop\"\n");
    helper_cfg_content.push_str("alias sys_capture_done_path \"mirv_movie_filename DOD_TOOLS_EXIT_TRIGGER; mirv_recordmovie_start; mirv_recordmovie_stop\"\n");



    // 1. Primer Job
    if total_jobs > 0 {
        let first_source = sorted_groups[0].0.0.clone();
        let mut primer_init = config.init_commands.clone();
        
        let separate_hud_str = if config.separate_hud { "1" } else { "0" };
        primer_init.push(format!("mirv_movie_separate_hud {}", separate_hud_str));

        let primer_out = if let Some(ref out_dir) = config.output_dir {
            out_dir.join("primer.dem")
        } else {
            std::path::PathBuf::from("primer.dem")
        };

        // Delay playdemo chain_01 to tick 500 (~5 seconds) to allow the engine to fully finish the 
        // 2-second GoldSrc server handshake without buffer overflows before jumping to the first real chain.
        let mut primer_scheduled = Vec::new();
        helper_cfg_content.push_str("# Demo specific next demos\n");
        helper_cfg_content.push_str("alias primer_next \"playdemo chain_01\"\n");
        primer_scheduled.push((500, "primer_next".to_string()));

        jobs.push(PatchJob {
            source_demo: first_source.clone(),
            output_demo: primer_out,
            streaks: Vec::new(),
            target_player: None,
            init_commands: primer_init,
            scheduled_commands: primer_scheduled,
            bookmarks: Vec::new(),
        });
    }

    // 2. Chained Jobs
    for (job_idx, ((source_demo, target_player), mut streaks)) in sorted_groups.into_iter().enumerate() {
        // Sort by start_tick in ascending order
        streaks.sort_by_key(|s| s.start_tick);

        let demo_bookmarks: Vec<i32> = streaks.iter().map(|s| s.start_tick).collect();

        let demo_fps = streaks.first().map(|s| s.demo_fps).filter(|&fps| fps > 0.0).unwrap_or(30.0);

        // Overlap Merge Logic
        let mut merged_streaks: Vec<CaptureStreak> = Vec::new();
        for current in streaks {
            if merged_streaks.is_empty() {
                merged_streaks.push(current);
            } else {
                let dynamic_pre_roll_ticks = (config.pre_roll_seconds * demo_fps) as i32;
                let dynamic_post_roll_ticks = (config.post_roll_seconds * demo_fps) as i32;
                
                let adjusted_start = (current.start_tick - dynamic_pre_roll_ticks).max(0);
                let last = merged_streaks.last_mut().unwrap();
                if adjusted_start <= last.end_tick + dynamic_post_roll_ticks {
                    last.end_tick = last.end_tick.max(current.end_tick);
                } else {
                    merged_streaks.push(current);
                }
            }
        }

        let demo_name = format!("chain_{:02}", job_idx + 1);
        let next_demo_name = format!("chain_{:02}", job_idx + 2);
        let output_name = format!("{}.dem", demo_name);
        let path = std::path::Path::new(&source_demo);
        let mut output_demo = path.with_file_name(&output_name);

        if let Some(ref out_dir) = config.output_dir {
            if !out_dir.exists() {
                let _ = std::fs::create_dir_all(out_dir);
            }
            output_demo = out_dir.join(&output_name);
        }


        if job_idx < total_jobs - 1 {
            helper_cfg_content.push_str(&format!("alias {}_next \"playdemo {}\"\n", demo_name, next_demo_name));
        } else {
            helper_cfg_content.push_str(&format!("alias {}_next \"sys_capture_done_path\"\n", demo_name));
        }
        
        helper_cfg_content.push_str(&format!("alias {}_path \"mirv_movie_filename dodtools_session/{}\"\n", demo_name, demo_name));

        // Generate scheduled commands
        let mut scheduled_commands = Vec::new();
        scheduled_commands.push((0, format!("{}_path", demo_name)));
        
        // Initialize Engine Speed after Initial Load Delay
        let initial_delay_ticks = (config.initial_delay * demo_fps) as i32;
        scheduled_commands.push((initial_delay_ticks, "sys_fast_forward".to_string()));

        for (i, streak) in merged_streaks.iter().enumerate() {


            let frame_times = &streak.frame_times;
            let absolute_final_frame = frame_times.len() as i32;
            let exit_frame = absolute_final_frame.saturating_sub(5);
            let danger_zone = absolute_final_frame.saturating_sub(10);

            let record_start_tick = find_tick_backwards(streak.start_tick as usize, config.record_start_lead, frame_times);
            let s_speed_tick = find_tick_backwards(record_start_tick.max(0) as usize, 3.0, frame_times);
            let s_sound_tick = find_tick_backwards(record_start_tick.max(0) as usize, 1.0, frame_times);
            let mut r_stop = find_tick_forwards(streak.end_tick as usize, config.record_stop_trail, frame_times);
            let mut s_end = find_tick_forwards(r_stop.max(0) as usize, config.post_roll_seconds, frame_times);

            let mut is_clutch = false;
            if s_end >= danger_zone {
                crate::log_markdown("⚠️ **Clutch Clip Detected:** Post-roll truncated to save batch near EOF.");
                is_clutch = true;
                r_stop = r_stop.min(exit_frame);
                s_end = exit_frame;
            } else {
                r_stop = r_stop.min(exit_frame);
                s_end = s_end.min(exit_frame);
            }

            // Custom command overrides
            for (idx, custom) in config.custom_commands.iter().enumerate() {
                let relation_str = match custom.relation {
                    CommandRelation::Before => "BEFORE",
                    CommandRelation::After => "AFTER",
                };
                let target_tick = match custom.relation {
                    CommandRelation::Before => {
                        let mut t = find_tick_backwards(streak.start_tick as usize, custom.offset, frame_times);
                        if t == s_speed_tick || t == s_sound_tick || t == record_start_tick {
                            t += 1;
                        }
                        t
                    }
                    CommandRelation::After => {
                        let mut t = find_tick_forwards(streak.end_tick as usize, custom.offset, frame_times);
                        if t == r_stop {
                            t += 1;
                        }
                        t
                    }
                };

                for (t, echo_cmd) in build_safe_echos(target_tick, &format!("CUSTOM_CMD{}_{} - Tick {}", idx + 1, relation_str, target_tick)) {
                    scheduled_commands.push((t, echo_cmd));
                }
                let cmd_len = custom.command.len();
                if cmd_len > 60 {
                    crate::log_markdown(&format!("⚠️ **WARNING:** Custom command exceeds 60 bytes and will likely be dropped by the GoldSrc Cbuf: {}", custom.command));
                }
                scheduled_commands.push((target_tick, custom.command.clone()));
            }

            // At Speed Flush (Stage 1)
            scheduled_commands.push((s_speed_tick, "sys_normal_speed".to_string()));
            for (t, echo_cmd) in build_safe_echos(s_speed_tick, &format!("SPEED_FLUSH - Tick {}", s_speed_tick)) {
                scheduled_commands.push((t, echo_cmd));
            }

            // At Sound Flush (Stage 1.5)
            scheduled_commands.push((s_sound_tick, "sys_sound".to_string()));
            for (t, echo_cmd) in build_safe_echos(s_sound_tick, &format!("AUDIO_SYNC - Tick {}", s_sound_tick)) {
                scheduled_commands.push((t, echo_cmd));
            }

            // At Start Frame (Stage 2)
            scheduled_commands.push((record_start_tick, "sys_record_start".to_string()));
            for (t, echo_cmd) in build_safe_echos(record_start_tick, &format!("START_RECORD - Tick {}", record_start_tick)) {
                scheduled_commands.push((t, echo_cmd));
            }

            // At End Frame (Stage 3)
            scheduled_commands.push((r_stop, "sys_record_stop".to_string()));
            for (t, echo_cmd) in build_safe_echos(r_stop, &format!("STOP_RECORD - Tick {}", r_stop)) {
                scheduled_commands.push((t, echo_cmd));
            }

            // At Post-Roll End (Stage 4)
            scheduled_commands.push((s_end, "sys_fast_forward".to_string()));
            for (t, echo_cmd) in build_safe_echos(s_end, &format!("FAST_FORWARD - Tick {}", s_end)) {
                scheduled_commands.push((t, echo_cmd));
            }

            if i == merged_streaks.len() - 1 {
                // At Absolute EOF
                if job_idx == total_jobs - 1 {
                    let echos = build_safe_echos(s_end, "BATCH_COMPLETE");
                    let echos_len = echos.len() as i32;
                    for (t, echo_cmd) in echos {
                        scheduled_commands.push((t, echo_cmd));
                    }
                    let final_tick = if is_clutch { exit_frame } else { s_end + echos_len };
                    scheduled_commands.push((final_tick, format!("{}_next", demo_name)));
                } else {
                    let final_tick = if is_clutch { exit_frame } else { s_end };
                    scheduled_commands.push((final_tick, format!("{}_next", demo_name)));
                }
            }
        }

        // Implement Global Breadcrumb Loop
        let total_demo_frames = merged_streaks.first().map(|s| s.frame_times.len()).unwrap_or(0) as i32;
        let mut step = 0;
        while step < total_demo_frames {
            scheduled_commands.push((
                step, 
                format!("echo \"[dod-tools] BREADCRUMB - Tick {}\"", step)
            ));
            step += 5000;
        }

        // Sort scheduled_commands by tick
        scheduled_commands.sort_by_key(|(tick, _)| *tick);

        let mut final_init_commands = config.init_commands.clone();

        final_init_commands.push(format!("mirv_movie_fps {}", config.capture_fps));

        let separate_hud_str = if config.separate_hud { "1" } else { "0" };
        final_init_commands.push(format!("mirv_movie_separate_hud {}", separate_hud_str));

        jobs.push(PatchJob {
            source_demo,
            output_demo,
            streaks: merged_streaks,
            target_player: target_player.clone(),
            init_commands: final_init_commands,
            scheduled_commands,
            bookmarks: demo_bookmarks,
        });
    }
    
    // Write dodtools_helper.cfg to dod_dir
    if !dod_dir.exists() {
        std::fs::create_dir_all(&dod_dir)?;
    }
    let cfg_path = dod_dir.join("dodtools_helper.cfg");
    std::fs::write(&cfg_path, helper_cfg_content)?;

    Ok(jobs)
}

pub struct WorkspaceGuard {
    pub session_junction: std::path::PathBuf,
    pub exit_trigger: std::path::PathBuf,
}

impl Drop for WorkspaceGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.session_junction);
        let _ = std::fs::remove_file(&self.exit_trigger);
        let _ = std::fs::remove_dir_all(&self.exit_trigger);
        if let Some(parent) = self.exit_trigger.parent() {
            let dod_dir = parent.join("dod");
            let _ = std::fs::remove_file(dod_dir.join("dodtools_helper.cfg"));
            let _ = std::fs::remove_file(dod_dir.join("dodtools_capture_done.cfg"));
            let _ = std::fs::remove_file(dod_dir.join("dod_quit.cfg"));
            if let Ok(entries) = std::fs::read_dir(&dod_dir) {
                for entry in entries.flatten() {
                    let filename = entry.file_name().to_string_lossy().to_string();
                    if filename.starts_with("dodtools_chain_") && filename.ends_with(".cfg") {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
}

// ── Channel-based worker spawner ──────────────────────────────────────────────
// Retained for the cancellation test and any callers that still reference it.
// The primary patch path now uses the inline patch_worker in capture/select.rs.

pub fn spawn_patch_batch(
    jobs: Vec<PatchJob>,
    config: PatcherConfig,
    cancel_token: Arc<AtomicBool>,
) -> CaptureWorker {
    let (tx, rx) = std::sync::mpsc::channel();
    let cancel_token_clone = cancel_token.clone();

    let handle = std::thread::spawn(move || {
        let total_jobs = jobs.len();
        if tx.send(PatchEvent::Starting(total_jobs)).is_err() {
            return;
        }

        let mut cancelled = false;
        for (idx, job) in jobs.iter().enumerate() {
            let start_pct = (idx as f32 / total_jobs as f32) * 100.0;
            if tx.send(PatchEvent::Progress(job.source_demo.clone(), start_pct)).is_err() {
                return;
            }

            let patcher = StreamPatcher::new(&job.source_demo, &job.output_demo);
            match patcher.patch(job, &config, &cancel_token_clone) {
                Ok(()) => {
                    let end_pct = ((idx + 1) as f32 / total_jobs as f32) * 100.0;
                    if tx.send(PatchEvent::Progress(job.source_demo.clone(), end_pct)).is_err() {
                        return;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    std::fs::remove_file(&job.output_demo).ok();
                    let _ = tx.send(PatchEvent::Cancelled);
                    cancelled = true;
                    break;
                }
                Err(e) => {
                    if tx.send(PatchEvent::Error(format!("Failed to patch {}: {}", job.source_demo, e))).is_err() {
                        return;
                    }
                }
            }
        }

        if !cancelled {
            let _ = tx.send(PatchEvent::Completed);
        }
    });

    CaptureWorker {
        receiver: rx,
        is_running: true,
        cancel_token,
        handle: Some(handle),
    }
}

// ── svc_director payload builder ──────────────────────────────────────────────

/// Build a GoldSrc `svc_director` (OpCode 0x33) net-message payload for a
/// `DRC_CMD_MESSAGE` (sub-command 0x06) HLTV title card.
///
/// The returned `Vec<u8>` is a self-contained net-message body ready to be
/// embedded inside a `Dem_NetworkBuffer` (frame type 0x00 / 0x01) payload.
///
/// Fixed wire layout (30 bytes before the text):
///
/// | Offset | Size | Value       | Meaning                 |
/// |--------|------|-------------|-------------------------|
/// | 0      | 1    | 0x33        | svc_director opcode     |
/// | 1      | 1    | payload_len | total bytes after opcode|
/// | 2      | 1    | 0x06        | DRC_CMD_MESSAGE         |
/// | 3      | 1    | 0x00        | effect (none)           |
/// | 4      | 4    | FF A0 00 00 | RGBA colour #FFA000FF   |
/// | 8      | 4    | -1.0 f32 LE | position X (centered)   |
/// | 12     | 4    | 0.85 f32 LE | position Y              |
/// | 16     | 4    | 0.5  f32 LE | fade-in  (seconds)      |
/// | 20     | 4    | 0.5  f32 LE | fade-out (seconds)      |
/// | 24     | 4    | 3.0  f32 LE | hold time (seconds)     |
/// | 28     | 4    | 0.0  f32 LE | FX time                 |
/// | 32     | N+1  | text + \0   | null-terminated string  |
///
/// `payload_len` = 30 (fields 2-31) + text_len + 1 (null), capped at 255.
pub fn build_director_message(text: &str) -> Vec<u8> {
    // Null-terminate and clamp so payload_len fits in one byte.
    // payload_len covers everything from the sub-command byte (offset 2) to the
    // end of the null-terminated string, i.e. 30 fixed bytes + string + NUL.
    // Maximum payload_len = 255, so maximum text bytes = 255 - 30 - 1 = 224.
    const FIXED_OVERHEAD: usize = 30; // bytes 2..31 (sub-cmd through FX time)
    const MAX_TEXT_BYTES: usize = 255 - FIXED_OVERHEAD - 1; // 224

    let raw = text.as_bytes();
    let text_len = raw.len().min(MAX_TEXT_BYTES);
    let text_bytes = &raw[..text_len];

    // payload_len is everything after the opcode and length byte itself.
    let payload_len: u8 = (FIXED_OVERHEAD + text_len + 1) as u8;

    let mut msg: Vec<u8> = Vec::with_capacity(2 + FIXED_OVERHEAD + text_len + 1);

    // Opcode + payload length
    msg.push(0x33);          // svc_director
    msg.push(payload_len);

    // Sub-command and effect
    msg.push(0x06);          // DRC_CMD_MESSAGE
    msg.push(0x00);          // effect: none

    // RGBA colour #FFA000FF
    msg.extend_from_slice(&[0xFF, 0xA0, 0x00, 0x00]);

    // Position (X = -1.0 → engine centers horizontally; Y = 0.85)
    msg.extend_from_slice(&(-1.0f32).to_le_bytes());
    msg.extend_from_slice(&(0.85f32).to_le_bytes());

    // Timing
    msg.extend_from_slice(&(0.5f32).to_le_bytes());  // fade in
    msg.extend_from_slice(&(0.5f32).to_le_bytes());  // fade out
    msg.extend_from_slice(&(3.0f32).to_le_bytes());  // hold time
    msg.extend_from_slice(&(0.0f32).to_le_bytes());  // FX time

    // Null-terminated text payload
    msg.extend_from_slice(text_bytes);
    msg.push(0x00);

    msg
}

/// Build a GoldSrc `svc_director` (OpCode 0x33) net-message for a
/// `DRC_CMD_STUFFTEXT` (sub-command 0x0A) executable command.
///
/// The engine executes `command` on the client console when the event fires
/// in the `viewdemo` event list. The returned `Vec<u8>` is a self-contained
/// net-message body ready to embed inside a `Dem_NetworkBuffer` frame.
///
/// Wire layout:
///
/// | Offset | Size | Value        | Meaning              |
/// |--------|------|--------------|----------------------|
/// | 0      | 1    | 0x33         | svc_director opcode  |
/// | 1      | 1    | payload_len  | 1 + text_len + 1     |
/// | 2      | 1    | 0x0A         | DRC_CMD_STUFFTEXT    |
/// | 3      | N    | command      | raw command string   |
/// | 3+N    | 1    | 0x00         | null terminator      |
///
/// Maximum `command` length is 253 bytes (keeps `payload_len` ≤ 255).
pub fn build_director_stufftext(command: &str) -> Vec<u8> {
    const MAX_TEXT_BYTES: usize = 253; // keeps payload_len <= 255

    let raw      = command.as_bytes();
    let text_len = raw.len().min(MAX_TEXT_BYTES);
    let text_bytes = &raw[..text_len];

    // payload_len = sub-command byte (1) + text + NUL
    let payload_len: u8 = (1 + text_len + 1) as u8;

    let mut msg: Vec<u8> = Vec::with_capacity(2 + 1 + text_len + 1);
    msg.push(0x33);           // svc_director
    msg.push(payload_len);
    msg.push(0x0A);           // DRC_CMD_STUFFTEXT
    msg.extend_from_slice(text_bytes);
    msg.push(0x00);           // null terminator
    msg
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_batch_queue_merging() {
        let config = PatcherConfig::default(); // pre = 200, post = 60
        let raw_streaks = vec![
            CaptureStreak {
                start_tick: 1000,
                end_tick: 1200,
                source_demo: "demo1.dem".to_string(),
                target_player: None,
                kill_count: 3,
                timeline_string: String::new(),
                duration_string: String::new(),
                player_index: 0,
                kills: Vec::new(),
                start_index: 0,
                end_index: 2,
                total_demo_frames: 3000,
                demo_fps: 100.0,
                viewdemo_times: Vec::new(),
                frame_times: std::sync::Arc::new(Vec::new()),
            },
            CaptureStreak {
                start_tick: 1300,
                end_tick: 1500,
                source_demo: "demo1.dem".to_string(),
                target_player: None,
                kill_count: 3,
                timeline_string: String::new(),
                duration_string: String::new(),
                player_index: 0,
                kills: Vec::new(),
                start_index: 0,
                end_index: 2,
                total_demo_frames: 3000,
                demo_fps: 100.0,
                viewdemo_times: Vec::new(),
                frame_times: std::sync::Arc::new(Vec::new()),
            },
            CaptureStreak {
                start_tick: 2000,
                end_tick: 2200,
                source_demo: "demo1.dem".to_string(),
                target_player: None,
                kill_count: 3,
                timeline_string: String::new(),
                duration_string: String::new(),
                player_index: 0,
                kills: Vec::new(),
                start_index: 0,
                end_index: 2,
                total_demo_frames: 3000,
                demo_fps: 100.0,
                viewdemo_times: Vec::new(),
                frame_times: std::sync::Arc::new(Vec::new()),
            },
        ];

        let jobs = build_batch_queue(raw_streaks, &config).unwrap();
        assert_eq!(jobs.len(), 2);
        
        let primer = &jobs[0];
        assert_eq!(primer.output_demo, std::path::PathBuf::from("primer.dem"));
        assert_eq!(primer.streaks.len(), 0);

        let job = &jobs[1];
        assert_eq!(job.source_demo, "demo1.dem");
        assert_eq!(job.output_demo, std::path::PathBuf::from("chain_01.dem"));
        assert_eq!(job.streaks.len(), 2);
        assert_eq!(job.streaks[0].start_tick, 1000);
        assert_eq!(job.streaks[0].end_tick, 1500); // Merged 1000-1200 and 1300-1500
        assert_eq!(job.streaks[1].start_tick, 2000);
        assert_eq!(job.streaks[1].end_tick, 2200);
    }
}
