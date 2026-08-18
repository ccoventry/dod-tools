// find_overlaps.rs
//
// Scans a folder of .dem files and reports which ones contain highlights that
// would merge into a single recording block during a capture batch.
//
// Why this exists: build_batch_queue collapses highlights that overlap (once
// pre/post-roll is applied) into one continuous take, so one take folder can be
// the source of several highlight rows. That fan-out is hard to exercise by
// hand, and finding a demo that actually triggers it by clicking through the UI
// is tedious. This reproduces the merge decision exactly, without capturing.
//
//   cargo run -p native --bin find_overlaps -- <folder> [options]
//
//   --pre-roll <secs>    default 2.0   (PatcherConfig::default)
//   --post-roll <secs>   default 0.6
//   --all-players        don't filter to the recording player
//   --show-all           list every demo, not just ones with merges
//
// Note: record_start_lead / record_stop_trail deliberately play no part here.
// They shift the scheduled record commands and the disk estimate, but the merge
// decision in builder.rs reads pre_roll_seconds/post_roll_seconds only.

use native::patch::scanner::scan_demo_for_highlights;
use native::patch::types::CaptureStreak;

struct Options {
    folder: std::path::PathBuf,
    pre_roll: f32,
    post_roll: f32,
    all_players: bool,
    show_all: bool,
}

fn print_usage() {
    eprintln!("usage: find_overlaps <folder> [--pre-roll <secs>] [--post-roll <secs>] [--all-players] [--show-all]");
}

fn parse_args() -> Option<Options> {
    let mut args = std::env::args().skip(1);
    let mut folder: Option<std::path::PathBuf> = None;
    // Defaults mirror PatcherConfig::default() so a bare run matches the app's
    // out-of-the-box behaviour.
    let mut pre_roll = 2.0_f32;
    let mut post_roll = 0.6_f32;
    let mut all_players = false;
    let mut show_all = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pre-roll" => pre_roll = args.next()?.parse().ok()?,
            "--post-roll" => post_roll = args.next()?.parse().ok()?,
            "--all-players" => all_players = true,
            "--show-all" => show_all = true,
            "-h" | "--help" => return None,
            other if other.starts_with("--") => {
                eprintln!("unknown option: {}", other);
                return None;
            }
            other => folder = Some(std::path::PathBuf::from(other.trim_matches(['"', '\'']))),
        }
    }

    Some(Options { folder: folder?, pre_roll, post_roll, all_players, show_all })
}

/// Mirrors the Overlap Merge Logic in `build_batch_queue` exactly: streaks are
/// sorted by start_tick, then each is folded into the previous block when its
/// pre-roll-adjusted start reaches back into the previous block's post-roll.
/// Returns one Vec of streak indices per resulting recording block.
fn simulate_merge(sorted: &[&CaptureStreak], pre_roll: f32, post_roll: f32) -> Vec<Vec<usize>> {
    if sorted.is_empty() {
        return Vec::new();
    }

    // Same fps resolution builder.rs uses: the first streak in the group, with a
    // 30.0 fallback when it's missing or zero.
    let demo_fps = sorted.first().map(|s| s.demo_fps).filter(|&f| f > 0.0).unwrap_or(30.0);
    let pre_ticks = (pre_roll * demo_fps) as i32;
    let post_ticks = (post_roll * demo_fps) as i32;

    let mut blocks: Vec<Vec<usize>> = Vec::new();
    let mut block_ends: Vec<i32> = Vec::new();

    for (i, streak) in sorted.iter().enumerate() {
        if blocks.is_empty() {
            blocks.push(vec![i]);
            block_ends.push(streak.end_tick);
            continue;
        }

        let last_end = *block_ends.last().unwrap();
        if native::patch::builder::blocks_merge(last_end, streak.start_tick, pre_ticks, post_ticks) {
            let end = block_ends.last_mut().unwrap();
            *end = (*end).max(streak.end_tick);
            blocks.last_mut().unwrap().push(i);
        } else {
            blocks.push(vec![i]);
            block_ends.push(streak.end_tick);
        }
    }

    blocks
}

fn secs(ticks: i32, fps: f32) -> f32 {
    if fps > 0.0 { ticks as f32 / fps } else { 0.0 }
}

struct DemoReport {
    name: String,
    total_highlights: usize,
    blocks: usize,
    merged_groups: Vec<Vec<usize>>,
    detail: Vec<String>,
    /// Tightest gap between consecutive highlights in this demo, and the rows
    /// either side of it. Drives the "how much roll would you need" hint.
    closest_gap: Option<(i32, f32, usize, usize)>,
}

fn main() {
    let opts = match parse_args() {
        Some(o) => o,
        None => {
            print_usage();
            std::process::exit(2);
        }
    };

    if !opts.folder.is_dir() {
        eprintln!("not a directory: {}", opts.folder.display());
        std::process::exit(1);
    }

    let mut demo_files: Vec<std::path::PathBuf> = match std::fs::read_dir(&opts.folder) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .map(|e| e.to_string_lossy().to_lowercase() == "dem")
                        .unwrap_or(false)
            })
            .collect(),
        Err(e) => {
            eprintln!("failed to read {}: {}", opts.folder.display(), e);
            std::process::exit(1);
        }
    };
    demo_files.sort();

    if demo_files.is_empty() {
        eprintln!("no .dem files found in {}", opts.folder.display());
        std::process::exit(1);
    }

    println!(
        "Scanning {} demo(s) in {}\npre-roll {:.2}s, post-roll {:.2}s, {}\n",
        demo_files.len(),
        opts.folder.display(),
        opts.pre_roll,
        opts.post_roll,
        if opts.all_players { "all players" } else { "recording player only" }
    );

    let mut reports: Vec<DemoReport> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut no_highlights: Vec<String> = Vec::new();

    for (idx, path) in demo_files.iter().enumerate() {
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        eprint!("\r[{}/{}] {:<60}", idx + 1, demo_files.len(), name);

        let (_tickrate, streaks, _is_pov, local_player_index, _frames, _match_start, _times) =
            match scan_demo_for_highlights(path) {
                Ok(v) => v,
                Err(e) => {
                    skipped.push(format!("{} ({})", name, e));
                    continue;
                }
            };

        // Match the app's POV filter: gate on whether local_player_index actually
        // resolved, not on is_pov (which also fires when an HLTV caster merely
        // spectated the match). None means no resolvable owner, so keep everything.
        let owned: Vec<&CaptureStreak> = match (opts.all_players, local_player_index) {
            (false, Some(rec)) => streaks.iter().filter(|s| s.player_index == rec).collect(),
            _ => streaks.iter().collect(),
        };

        if owned.is_empty() {
            no_highlights.push(name);
            continue;
        }

        // build_batch_queue groups by (source_demo, target_player) before merging.
        // Within a single file that reduces to grouping by target_player.
        let mut by_player: std::collections::BTreeMap<String, Vec<&CaptureStreak>> =
            std::collections::BTreeMap::new();
        for streak in &owned {
            by_player
                .entry(streak.target_player.clone().unwrap_or_else(|| "<none>".to_string()))
                .or_default()
                .push(streak);
        }

        let mut blocks_total = 0_usize;
        let mut merged_groups: Vec<Vec<usize>> = Vec::new();
        let mut detail: Vec<String> = Vec::new();
        let mut closest_gap: Option<(i32, f32, usize, usize)> = None;

        for (player, mut group) in by_player {
            group.sort_by_key(|s| s.start_tick);
            let fps = group.first().map(|s| s.demo_fps).filter(|&f| f > 0.0).unwrap_or(30.0);

            // Tightest consecutive gap, regardless of whether it merges — this
            // is what tells you how much roll padding it would take.
            for pair in group.windows(2) {
                let gap = pair[1].start_tick - pair[0].end_tick;
                let idx = group.iter().position(|s| std::ptr::eq(*s, pair[0])).unwrap_or(0);
                if closest_gap.map(|(g, _, _, _)| gap < g).unwrap_or(true) {
                    closest_gap = Some((gap, fps, idx + 1, idx + 2));
                }
            }

            let blocks = simulate_merge(&group, opts.pre_roll, opts.post_roll);
            blocks_total += blocks.len();

            for block in &blocks {
                if block.len() < 2 {
                    continue;
                }
                merged_groups.push(block.clone());

                let rows: Vec<String> = block.iter().map(|&i| format!("#{}", i + 1)).collect();
                let first = group[block[0]];
                let last_end = block.iter().map(|&i| group[i].end_tick).max().unwrap_or(0);
                detail.push(format!(
                    "      {} rows {} -> one take  (ticks {}-{}, ~{:.1}s @ {:.0}fps, player {})",
                    if player == "<none>" { "".to_string() } else { format!("[{}]", player) },
                    rows.join("+"),
                    first.start_tick,
                    last_end,
                    secs(last_end - first.start_tick, fps),
                    fps,
                    first.player_index
                ));

                // Gap between each consecutive pair, so it's obvious how close
                // they were and how much the rolls had to stretch to join them.
                for pair in block.windows(2) {
                    let a = group[pair[0]];
                    let b = group[pair[1]];
                    let gap = b.start_tick - a.end_tick;
                    detail.push(format!(
                        "        gap #{}->#{}: {} ticks (~{:.2}s)",
                        pair[0] + 1,
                        pair[1] + 1,
                        gap,
                        secs(gap, fps)
                    ));
                }
            }
        }

        reports.push(DemoReport {
            name,
            total_highlights: owned.len(),
            blocks: blocks_total,
            merged_groups,
            detail,
            closest_gap,
        });
    }

    eprintln!("\r{:<70}\r", "");

    // Most merges first — the best test candidate is the one that exercises the
    // fan-out hardest.
    reports.sort_by(|a, b| {
        let a_merged: usize = a.merged_groups.iter().map(|g| g.len()).sum();
        let b_merged: usize = b.merged_groups.iter().map(|g| g.len()).sum();
        b_merged.cmp(&a_merged).then_with(|| a.name.cmp(&b.name))
    });

    let with_overlaps: Vec<&DemoReport> = reports.iter().filter(|r| !r.merged_groups.is_empty()).collect();

    if with_overlaps.is_empty() {
        println!("No overlapping highlights found at pre-roll {:.2}s / post-roll {:.2}s.\n", opts.pre_roll, opts.post_roll);

        // The nearest miss across every demo, and what it would take to close it.
        let nearest = reports
            .iter()
            .filter_map(|r| r.closest_gap.map(|g| (r, g)))
            .min_by_key(|(_, (gap, _, _, _))| *gap);

        if let Some((demo, (gap, fps, row_a, row_b))) = nearest {
            let gap_secs = secs(gap, fps);
            let have = opts.pre_roll + opts.post_roll;
            println!("Closest pair anywhere: {} rows #{} and #{}", demo.name, row_a, row_b);
            println!("  {} ticks apart (~{:.2}s @ {:.0}fps)", gap, gap_secs, fps);
            println!(
                "  Current pre+post roll covers {:.2}s — needs > {:.2}s to merge.",
                have, gap_secs
            );
            println!(
                "\n  To force a merge for testing, re-run with e.g.:\n    --pre-roll {:.1} --post-roll {:.1}\n  and use those same values in Capture Studio's Timing Options.",
                (gap_secs + 0.5).ceil(),
                opts.post_roll
            );
        } else {
            println!("No demo had two or more highlights to compare.");
        }
        println!("\nAlso worth trying: --all-players to include non-recording players' streaks.\n");
    } else {
        println!("=== Demos with merging highlights ({} of {}) ===\n", with_overlaps.len(), reports.len());
        for r in &with_overlaps {
            println!(
                "  {}\n    {} highlight(s) -> {} recording block(s), {} merge group(s)",
                r.name,
                r.total_highlights,
                r.blocks,
                r.merged_groups.len()
            );
            for line in &r.detail {
                println!("{}", line);
            }
            println!();
        }
        let best = with_overlaps[0];
        println!(
            "Best test candidate: {}\n  Select its highlight rows {} in Highlight Details --\n  they should capture as a single take and verify as one block.\n",
            best.name,
            best.merged_groups[0].iter().map(|&i| format!("#{}", i + 1)).collect::<Vec<_>>().join(" and ")
        );
    }

    if opts.show_all {
        println!("=== All demos ===");
        let mut by_name: Vec<&DemoReport> = reports.iter().collect();
        by_name.sort_by(|a, b| a.name.cmp(&b.name));
        for r in by_name {
            println!(
                "  {:<50} {:>3} highlight(s) -> {:>3} block(s){}",
                r.name,
                r.total_highlights,
                r.blocks,
                if r.merged_groups.is_empty() { "" } else { "  <-- has merges" }
            );
        }
        println!();
    }

    println!("Scanned {} demo(s) with highlights.", reports.len());
    if !no_highlights.is_empty() {
        println!("  {} had no highlights: {}", no_highlights.len(), no_highlights.join(", "));
    }
    if !skipped.is_empty() {
        println!("  {} skipped (HLTV/unreadable):", skipped.len());
        for s in &skipped {
            println!("    {}", s);
        }
    }
}
