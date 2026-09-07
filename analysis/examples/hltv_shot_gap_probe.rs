//! Detects weapon-fire events that a recording *dropped*, using only that
//! recording -- no second demo to compare against.
//!
//! An automatic weapon fires at a fixed cyclic rate, so within a held burst
//! the gap between consecutive events is that rate. If a recording loses
//! rounds, the surviving gaps land on near-exact integer multiples of it. A
//! human releasing and re-pressing the trigger produces gaps too, but has no
//! reason to land on an exact multiple -- so "gap is within tolerance of n
//! times the cyclic rate, n >= 2" separates dropped rounds from deliberate
//! pauses.
//!
//! The cyclic rate is measured from the demo itself (the modal gap for that
//! weapon), so there is no hardcoded weapon table and it adapts to whatever
//! rate the recording actually ran at.
//!
//! Events are grouped per (weapon, shooter), because two players firing the
//! same weapon interleave into one stream otherwise and manufacture gaps that
//! were never one player's burst.
//!
//! **Validating the heuristic:** run it on a POV demo. Those record their own
//! player's fire essentially completely, so whatever this reports there is the
//! false-positive rate -- pauses being misread as drops. Compare that against
//! what it reports for an HLTV demo of the same match.
//!
//!     cargo run --release -p analysis --example hltv_shot_gap_probe -- demo.dem

use dem::bit::BitSliceCast;
use dem::open_demo_from_bytes;
use dem::types::{EngineMessage, FrameData, MessageData, NetMessage};
use std::collections::HashMap;

/// A gap counts as dropped rounds only if it lands this close to an exact
/// multiple of the cyclic rate (as a fraction of that rate).
const MULTIPLE_TOLERANCE: f32 = 0.25;
/// Gaps longer than this are a new engagement, not a burst with holes in it.
const MAX_BURST_GAP: f32 = 1.0;

/// Only full-automatic weapons have a cyclic rate for this to key off. On a
/// semi-automatic the gap between shots is just how fast the player clicked,
/// so "gap is twice the usual gap" carries no information at all and the
/// heuristic reports noise -- measured at 27.8% on the Luger in an HLTV demo
/// and 7.5% on the Garand, against 1.2% and 2.8% for the same weapons in a POV
/// demo, which is the tell that those numbers are the method failing rather
/// than rounds going missing.
const AUTOMATIC_WEAPONS: &[&str] =
    &["bar", "mp44", "mp40", "thompson", "sten", "greasegun", "bren", "mg42", "mg34", "30cal"];

fn main() {
    let path = std::env::args().nth(1).expect("usage: hltv_shot_gap_probe <demo>");
    let bytes = std::fs::read(&path).expect("read demo");
    let demo = open_demo_from_bytes(&bytes).expect("parse demo");

    let mut event_names: HashMap<u32, String> = HashMap::new();
    // (event index, shooter) -> fire times
    let mut fires: HashMap<(u32, u32), Vec<f32>> = HashMap::new();

    for entry in &demo.directory.entries {
        for frame in &entry.frames {
            let FrameData::NetworkMessage(bt) = &frame.frame_data else { continue };
            let MessageData::Parsed(msgs) = &bt.1.messages else { continue };
            for m in msgs {
                let NetMessage::EngineMessage(em) = m else { continue };
                match &**em {
                    EngineMessage::SvcResourceList(rl) => {
                        for r in &rl.resources {
                            if r.type_.to_u8() == 5 {
                                event_names.insert(r.index.to_u32(), r.name.get_string());
                            }
                        }
                    }
                    EngineMessage::SvcEvent(ev) => {
                        for e in &ev.events {
                            let shooter = e.packet_index.as_ref().map(|p| p.to_u32()).unwrap_or(u32::MAX);
                            fires.entry((e.event_index.to_u32(), shooter)).or_default().push(frame.time);
                        }
                    }
                    EngineMessage::SvcEventReliable(ev) => {
                        fires.entry((ev.event_index.to_u32(), u32::MAX)).or_default().push(frame.time);
                    }
                    _ => {}
                }
            }
        }
    }

    // Per weapon: every (shooter, times) stream, and every in-burst gap
    // pooled across shooters so the cyclic rate has enough samples to be
    // stable even when one player only fired a few rounds.
    let mut per_weapon: HashMap<String, Vec<Vec<f32>>> = HashMap::new();
    for ((idx, _shooter), mut times) in fires {
        let Some(name) = event_names.get(&idx) else { continue };
        if !name.contains("events/weapons/") {
            continue;
        }
        // Resource names arrive NUL-terminated, so strip that (and any
        // whitespace) before the ".sc" suffix, or the suffix never matches.
        let short = name
            .trim_matches(|c: char| c == '\0' || c.is_whitespace())
            .trim_start_matches("events/weapons/")
            .trim_end_matches(".sc")
            .to_string();
        if !AUTOMATIC_WEAPONS.contains(&short.as_str()) {
            continue;
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        per_weapon.entry(short).or_default().push(times);
    }

    println!("=== {path} ===\n");
    println!(
        "{:<12} {:>7} {:>9} {:>8} {:>9} {:>9} {:>8}",
        "weapon", "shots", "shooters", "cyclic", "in burst", "dropped", "loss"
    );

    let mut rows: Vec<(String, Vec<Vec<f32>>)> = per_weapon.into_iter().collect();
    rows.sort_by_key(|(_, s)| std::cmp::Reverse(s.iter().map(|t| t.len()).sum::<usize>()));

    let (mut total_shots, mut total_dropped) = (0usize, 0usize);

    for (name, streams) in rows {
        let shots: usize = streams.iter().map(|t| t.len()).sum();
        if shots < 30 {
            continue;
        }

        let gaps: Vec<f32> = streams
            .iter()
            .flat_map(|t| t.windows(2).map(|w| w[1] - w[0]))
            .filter(|g| *g > 0.0 && *g < MAX_BURST_GAP)
            .collect();
        if gaps.len() < 10 {
            continue;
        }

        // Cyclic rate = modal gap, in 5ms buckets for a finer estimate than
        // the histogram this tool used to print.
        let mut buckets: HashMap<i32, usize> = HashMap::new();
        for g in &gaps {
            *buckets.entry((g / 0.005) as i32).or_insert(0) += 1;
        }
        let cyclic = buckets.iter().max_by_key(|(_, c)| **c).map(|(b, _)| (*b as f32 + 0.5) * 0.005).unwrap_or(0.0);
        if cyclic <= 0.0 {
            continue;
        }

        let mut dropped = 0usize;
        for gap in &gaps {
            let n = (gap / cyclic).round();
            if n >= 2.0 && (gap - n * cyclic).abs() <= MULTIPLE_TOLERANCE * cyclic {
                dropped += (n as usize) - 1;
            }
        }

        let recorded_plus_lost = shots + dropped;
        println!(
            "{name:<12} {shots:>7} {:>9} {:>7.0}ms {:>9} {dropped:>9} {:>7.1}%",
            streams.len(),
            cyclic * 1000.0,
            gaps.len(),
            100.0 * dropped as f32 / recorded_plus_lost as f32,
        );
        total_shots += shots;
        total_dropped += dropped;
    }

    if total_shots > 0 {
        println!(
            "\ntotal: {total_shots} recorded, {total_dropped} estimated dropped ({:.1}% loss)",
            100.0 * total_dropped as f32 / (total_shots + total_dropped) as f32
        );
    }
}
