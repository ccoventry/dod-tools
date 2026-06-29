use native::patch::scanner::scan_demo_for_highlights;
use native::patch::types::{HighlightRules, PatcherConfig};

fn main() {
    let path = std::path::Path::new("demos/ktps8w3_gorilla_dicE_____G.dem");
    let rules = HighlightRules {
        min_kills: Some(1),
        target_players: vec![],
        max_time_gap: None,
    };
    match scan_demo_for_highlights(path, &rules) {
        Ok((_tickrate, mut streaks, _is_pov, _pov_idx, _frames)) => {
            streaks.retain(|s| s.target_player.as_deref().unwrap_or("") == "dicE ::: [Gorilla]x");
            for (i, streak) in streaks.iter().enumerate() {
                println!("Streak {}: start_tick = {}, end_tick = {}, kills = {}", i, streak.start_tick, streak.end_tick, streak.kill_count);
            }
        },
        Err(e) => println!("Error: {}", e),
    }
}
