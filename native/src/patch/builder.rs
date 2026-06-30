// patch/builder.rs
// Batch job construction and the legacy channel-based worker spawner.
// Calls std::fs::create_dir_all and std::thread::spawn — native-only.

use std::sync::{Arc, atomic::AtomicBool};
use crate::patch::types::{
    CaptureStreak, PatchJob, PatcherConfig, CommandRelation,
    CaptureWorker, PatchEvent,
};
use crate::patch::engine::StreamPatcher;

// ── Batch queue builder ───────────────────────────────────────────────────────

pub fn build_batch_queue(raw_streaks: Vec<CaptureStreak>, config: &PatcherConfig) -> Vec<PatchJob> {
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

    // 1. Primer Job
    if total_jobs > 0 {
        let first_source = sorted_groups[0].0.0.clone();
        let mut primer_init = config.init_commands.clone();
        
        let active_export_dir = config.primary_media_dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let active_export_dir_str = active_export_dir.to_string_lossy().to_string();
        primer_init.push(format!("mirv_movie_filename \"{}\"", active_export_dir_str));
        
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
        primer_scheduled.push((500, "playdemo chain_01".to_string()));

        jobs.push(PatchJob {
            source_demo: first_source.clone(),
            output_demo: primer_out,
            streaks: Vec::new(),
            target_player: None,
            init_commands: primer_init,
            scheduled_commands: primer_scheduled,
        });
    }

    // 2. Chained Jobs
    for (job_idx, ((source_demo, target_player), mut streaks)) in sorted_groups.into_iter().enumerate() {
        // Sort by start_tick in ascending order
        streaks.sort_by_key(|s| s.start_tick);

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

        let output_name = format!("chain_{:02}.dem", job_idx + 1);
        let path = std::path::Path::new(&source_demo);
        let mut output_demo = path.with_file_name(&output_name);

        if let Some(ref out_dir) = config.output_dir {
            if !out_dir.exists() {
                let _ = std::fs::create_dir_all(out_dir);
            }
            output_demo = out_dir.join(&output_name);
        }

        // Generate scheduled commands
        let mut scheduled_commands = Vec::new();
        
        // Initialize Engine Speed after Initial Load Delay
        let initial_delay_ticks = (config.initial_delay * demo_fps) as i32;
        scheduled_commands.push((initial_delay_ticks, format!("host_framerate {}", config.fast_forward_speed)));

        for (i, streak) in merged_streaks.iter().enumerate() {
            let pre_roll_ticks = (config.pre_roll_seconds * demo_fps) as i32;
            let record_lead_ticks = (config.record_start_lead * demo_fps) as i32;
            let record_trail_ticks = (config.record_stop_trail * demo_fps) as i32;
            let post_roll_ticks = (config.post_roll_seconds * demo_fps) as i32;

            let stabilize_start_tick = streak.start_tick.saturating_sub(record_lead_ticks + pre_roll_ticks).max(0);
            let record_start_tick = streak.start_tick.saturating_sub(record_lead_ticks).max(0);
            let record_stop_tick = streak.end_tick.saturating_add(record_trail_ticks);
            let post_roll_end_tick = record_stop_tick.saturating_add(post_roll_ticks);

            // Preroll commands
            scheduled_commands.push((stabilize_start_tick, "host_framerate 0".to_string()));
            scheduled_commands.push((stabilize_start_tick, "echo \"[dod-tools] host_framerate 0\"".to_string()));

            scheduled_commands.push((stabilize_start_tick + 5, "stopsound".to_string()));
            scheduled_commands.push((stabilize_start_tick + 5, "echo \"[dod-tools] stopsound\"".to_string()));

            // Custom command overrides
            for custom in &config.custom_commands {
                let target_tick = match custom.relation {
                    CommandRelation::Before => streak.start_tick - (custom.offset * demo_fps) as i32,
                    CommandRelation::After => streak.end_tick + (custom.offset * demo_fps) as i32,
                };
                scheduled_commands.push((target_tick.max(0), custom.command.clone()));
            }

            // Record start (Atomic execution to prevent delta frame rendering)
            let fps_cmd = format!("mirv_movie_fps {}; mirv_recordmovie_start", config.capture_fps);
            scheduled_commands.push((record_start_tick, fps_cmd.clone()));
            scheduled_commands.push((record_start_tick, format!("echo \"[dod-tools] Start Frame {}\"", record_start_tick)));

            // Record stop & post roll
            if i < merged_streaks.len() - 1 {
                // Not the last streak, resume fast forward
                scheduled_commands.push((record_stop_tick, "mirv_recordmovie_stop".to_string()));
                scheduled_commands.push((record_stop_tick, format!("echo \"[dod-tools] Stop Frame {}\"", record_stop_tick)));
                scheduled_commands.push((post_roll_end_tick, format!("host_framerate {}", config.fast_forward_speed)));
            } else if job_idx < total_jobs - 1 {
                // Last streak, but NOT the last demo in the batch
                let next_chain = format!("chain_{:02}", job_idx + 2);
                scheduled_commands.push((record_stop_tick, "mirv_recordmovie_stop".to_string()));
                scheduled_commands.push((record_stop_tick, format!("echo \"[dod-tools] Stop Frame {}\"", record_stop_tick)));
                scheduled_commands.push((record_stop_tick + 2, format!("playdemo {}", next_chain)));
            } else {
                // Last streak of the final demo
                scheduled_commands.push((record_stop_tick, "mirv_recordmovie_stop".to_string()));
                scheduled_commands.push((record_stop_tick + 1, "mirv_movie_filename DOD_BATCH_DONE".to_string()));
                scheduled_commands.push((record_stop_tick + 2, "mirv_movie_fps 1; mirv_recordmovie_start".to_string()));
            }
        }

        // Sort scheduled_commands by tick
        scheduled_commands.sort_by_key(|(tick, _)| *tick);

        let mut final_init_commands = config.init_commands.clone();

        let active_export_dir = config.primary_media_dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let active_export_dir_str = active_export_dir.to_string_lossy().to_string();
        final_init_commands.push(format!("mirv_movie_filename \"{}\"", active_export_dir_str));
        
        let separate_hud_str = if config.separate_hud { "1" } else { "0" };
        final_init_commands.push(format!("mirv_movie_separate_hud {}", separate_hud_str));

        jobs.push(PatchJob {
            source_demo,
            output_demo,
            streaks: merged_streaks,
            target_player: target_player.clone(),
            init_commands: final_init_commands,
            scheduled_commands,
        });
    }

    jobs
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
            },
        ];

        let jobs = build_batch_queue(raw_streaks, &config);
        assert_eq!(jobs.len(), 1);
        let job = &jobs[0];
        assert_eq!(job.source_demo, "demo1.dem");
        assert_eq!(job.output_demo, std::path::PathBuf::from("demo1_patched.dem"));
        assert_eq!(job.streaks.len(), 2);
        assert_eq!(job.streaks[0].start_tick, 1000);
        assert_eq!(job.streaks[0].end_tick, 1500); // Merged 1000-1200 and 1300-1500
        assert_eq!(job.streaks[1].start_tick, 2000);
        assert_eq!(job.streaks[1].end_tick, 2200);
    }
}
