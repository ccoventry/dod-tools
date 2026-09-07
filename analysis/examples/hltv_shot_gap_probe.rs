//! Measures the spacing between consecutive weapon-fire events, to tell apart
//! "HLTV recorded every shot" from "HLTV dropped shots between snapshots".
//!
//! An automatic weapon fires at a fixed cyclic rate, so in a recording that
//! captured every round the gaps between its events cluster tightly around one
//! value. If a recording drops rounds, that cluster thins out and gaps appear
//! at rough multiples of it. Comparing the same weapon between an HLTV demo
//! and a POV demo of the same match is therefore a direct test of whether
//! HLTV is losing shots, and needs no per-player attribution.
//!
//!     cargo run --release -p analysis --example hltv_shot_gap_probe -- demo.dem

use dem::bit::BitSliceCast;
use dem::open_demo_from_bytes;
use dem::types::{EngineMessage, FrameData, MessageData, NetMessage};
use std::collections::HashMap;

fn main() {
    let path = std::env::args().nth(1).expect("usage: hltv_shot_gap_probe <demo>");
    let bytes = std::fs::read(&path).expect("read demo");
    let demo = open_demo_from_bytes(&bytes).expect("parse demo");

    let mut event_names: HashMap<u32, String> = HashMap::new();
    let mut fires: HashMap<u32, Vec<f32>> = HashMap::new();

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
                            fires.entry(e.event_index.to_u32()).or_default().push(frame.time);
                        }
                    }
                    EngineMessage::SvcEventReliable(ev) => {
                        fires.entry(ev.event_index.to_u32()).or_default().push(frame.time);
                    }
                    _ => {}
                }
            }
        }
    }

    println!("=== {path} ===\n");
    println!(
        "{:<14} {:>6} {:>9} {:>9} {:>9}   gap histogram (25ms buckets, <1s)",
        "weapon", "shots", "min gap", "modal gap", "in burst"
    );

    let mut rows: Vec<(String, Vec<f32>)> = fires
        .into_iter()
        .filter_map(|(idx, times)| {
            let name = event_names.get(&idx)?;
            if !name.contains("events/weapons/") {
                return None;
            }
            let short = name.trim_start_matches("events/weapons/").trim_end_matches(".sc").trim().to_string();
            Some((short, times))
        })
        .collect();
    rows.sort_by_key(|(_, t)| std::cmp::Reverse(t.len()));

    for (name, mut times) in rows {
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // Gaps within a burst only: anything over a second is a new engagement,
        // not a cyclic-rate measurement.
        let gaps: Vec<f32> = times.windows(2).map(|w| w[1] - w[0]).filter(|g| *g > 0.0 && *g < 1.0).collect();
        if gaps.len() < 10 {
            continue;
        }

        let mut buckets: HashMap<i32, usize> = HashMap::new();
        for g in &gaps {
            *buckets.entry((g / 0.025) as i32).or_insert(0) += 1;
        }
        let modal = buckets.iter().max_by_key(|(_, c)| **c).map(|(b, _)| *b as f32 * 0.025).unwrap_or(0.0);
        let min = gaps.iter().cloned().fold(f32::MAX, f32::min);

        let mut hist: Vec<(i32, usize)> = buckets.into_iter().collect();
        hist.sort();
        let bars: String = hist
            .iter()
            .take(12)
            .map(|(b, c)| format!("{:.0}ms:{} ", *b as f32 * 25.0, c))
            .collect();

        println!(
            "{name:<14} {:>6} {:>8.0}ms {:>8.0}ms {:>9}   {bars}",
            times.len(),
            min * 1000.0,
            modal * 1000.0,
            gaps.len(),
        );
    }
}
