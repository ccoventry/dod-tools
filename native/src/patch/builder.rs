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
    let mut grouped: std::collections::HashMap<(String, Option<String>), Vec<CaptureStreak>> = std::collections::HashMap::new();
    for streak in raw_streaks {
        grouped.entry((streak.source_demo.clone(), streak.target_player.clone())).or_default().push(streak);
    }

    let mut jobs = Vec::new();

    for ((source_demo, target_player), mut streaks) in grouped {
        // Sort by start_tick in ascending order
        streaks.sort_by_key(|s| s.start_tick);

        // Overlap Merge Logic
        let mut merged_streaks: Vec<CaptureStreak> = Vec::new();
        for current in streaks {
            if merged_streaks.is_empty() {
                merged_streaks.push(current);
            } else {
                let adjusted_start = (current.start_tick - config.pre_roll_ticks).max(0);
                let last = merged_streaks.last_mut().unwrap();
                if adjusted_start <= last.end_tick + config.post_roll_ticks {
                    last.end_tick = last.end_tick.max(current.end_tick);
                } else {
                    merged_streaks.push(current);
                }
            }
        }

        // Safe output path manipulation
        let path = std::path::Path::new(&source_demo);
        let base_name = path.file_stem().unwrap().to_str().unwrap();
        let output_name = if let Some(ref player_name) = target_player {
            let sanitized: String = player_name.chars()
                .map(|c| if c.is_alphanumeric() { c } else { '_' })
                .collect();
            format!("{}_{}_patched.dem", base_name, sanitized)
        } else {
            format!("{}_patched.dem", base_name)
        };
        let mut output_demo = path.with_file_name(&output_name);

        if let Some(ref out_dir) = config.output_dir {
            if !out_dir.exists() {
                let _ = std::fs::create_dir_all(out_dir);
            }
            output_demo = out_dir.join(&output_name);
        }

        // Generate scheduled commands
        let mut scheduled_commands = Vec::new();
        for streak in &merged_streaks {
            let pre_roll_tick = (streak.start_tick - (config.pre_roll_seconds * config.tickrate) as i32).max(0);
            let record_start_tick = (streak.start_tick - (config.record_start_lead * config.tickrate) as i32).max(0);
            let record_stop_tick = streak.end_tick + (config.record_stop_trail * config.tickrate) as i32;
            let post_roll_tick = streak.end_tick + (config.post_roll_seconds * config.tickrate) as i32;

            // Preroll commands
            scheduled_commands.push((pre_roll_tick, format!("host_framerate {}", config.capture_fps)));
            scheduled_commands.push((pre_roll_tick, "r_decals 0".to_string()));
            scheduled_commands.push((pre_roll_tick, "r_decals 5555".to_string()));

            // Amendment 1:
            if let Some(ref player_name) = target_player {
                scheduled_commands.push((pre_roll_tick, format!("spec_player \"{}\"", player_name)));
                scheduled_commands.push((pre_roll_tick, "spec_mode 4".to_string()));
            }

            // Custom command overrides
            for custom in &config.custom_commands {
                let target_tick = match custom.relation {
                    CommandRelation::Before => streak.start_tick - (custom.offset * config.tickrate) as i32,
                    CommandRelation::After => streak.end_tick + (custom.offset * config.tickrate) as i32,
                };
                scheduled_commands.push((target_tick.max(0), custom.command.clone()));
            }

            // Record start
            scheduled_commands.push((record_start_tick, format!("startmovie cap_ {}", config.capture_fps)));

            // Record stop
            scheduled_commands.push((record_stop_tick, "endmovie".to_string()));

            // Post roll end
            scheduled_commands.push((post_roll_tick, "host_framerate 0".to_string()));
        }

        // Exit on finish
        if config.exit_on_finish {
            if let Some(last_streak) = merged_streaks.last() {
                let post_roll_tick = last_streak.end_tick + (config.post_roll_seconds * config.tickrate) as i32;
                scheduled_commands.push((post_roll_tick + 50, "quit".to_string()));
            }
        }

        // Sort scheduled_commands by tick
        scheduled_commands.sort_by_key(|(tick, _)| *tick);

        jobs.push(PatchJob {
            source_demo,
            output_demo,
            streaks: merged_streaks,
            target_player: target_player.clone(),
            init_commands: config.init_commands.clone(),
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
