use native::patch::CaptureStreak;
use crate::types::DemoData;

pub struct StreakFilter {
    pub selected_only: bool,
    pub pov_local_only: bool,
}

pub fn build_capture_streak_payload(
    demos: &[DemoData],
    filter: StreakFilter,
) -> Vec<CaptureStreak> {
    let mut payload = Vec::new();
    for demo in demos {
        let demo_path_str = demo.path.to_string_lossy().to_string();
        for streak in &demo.streaks {
            if filter.selected_only && !streak.is_selected {
                continue;
            }
            if filter.pov_local_only && demo.is_pov && Some(streak.player_index) != demo.local_player_index {
                continue;
            }
            payload.push(CaptureStreak {
                start_tick: streak.start_tick,
                end_tick: streak.end_tick,
                source_demo: demo_path_str.clone(),
                target_player: Some(streak.target_player.clone()),
                kill_count: streak.kill_count,
                timeline_string: streak.timeline_string.clone(),
                duration_string: streak.duration_string.clone(),
                player_index: streak.player_index,
                kills: streak.kills.clone(),
                start_index: streak.start_index,
                end_index: streak.end_index,
                total_demo_frames: demo.playback_frames,
                demo_fps: demo.tickrate,
                viewdemo_times: streak.viewdemo_times.clone(),
                frame_times: streak.frame_times.clone(),
                status: streak.status,
            });
        }
    }
    payload
}
