//! What the *engine* does to the first-person viewmodel, read out of a demo.
//!
//! `goldsrc-hooks`' animation fix synthesises viewmodel animations for a
//! spectated player, because GoldSrc's weapon event scripts only drive the
//! viewmodel for the local player (`EV_IsLocal`). A POV demo is a recording of
//! that same machinery working properly, so it is the reference the fix is
//! trying to reproduce -- and it states its answer outright rather than leaving
//! it to be inferred from behaviour.
//!
//! Two carriers, both counted here:
//!
//! - **`svc_weaponanim` (35)** -- the server telling a client to play a
//!   viewmodel sequence. Sent to the weapon's owner.
//! - **`Dem_WeaponAnim` frames** -- the demo recorder's own note of the same
//!   thing, written alongside the network stream.
//!
//! Run it on a POV demo and the HLTV demo of the same half. The question it
//! answers is whether an HLTV recording carries any of this for the players it
//! is watching: if it does, that is a better trigger than anything the fix
//! currently infers, and it is exact. If it does not, that is the measurement
//! that justifies inferring at all.
//!
//!     cargo run --release -p analysis --example weapon_anim_probe -- <demo>...

use dem::open_demo_from_bytes;
use dem::types::{EngineMessage, FrameData, MessageData, NetMessage};
use std::collections::HashMap;

/// Injected director commands do not move a camera and dod-tools writes them
/// into previews, so counting them would report our own patched POV demos as
/// HLTV. See `hltv_shot_gap_probe`.
const DRC_CMD_MESSAGE: u8 = 0x06;
const DRC_CMD_STUFFTEXT: u8 = 0x0A;

struct Counts {
    director: usize,
    svc_anim: usize,
    frame_anim: usize,
    /// Sequence number -> how often the engine asked for it.
    sequences: HashMap<i32, usize>,
    /// Demo time of each animation, in order, for gap analysis. Kept per
    /// carrier: both record the same event, milliseconds apart, so pooling them
    /// reports a median gap of one frame that is really just the pairing.
    svc_times: Vec<f32>,
    frame_times: Vec<f32>,
}

fn measure(bytes: &[u8]) -> Option<Counts> {
    let demo = open_demo_from_bytes(bytes).ok()?;
    let mut c = Counts {
        director: 0,
        svc_anim: 0,
        frame_anim: 0,
        sequences: HashMap::new(),
        svc_times: Vec::new(),
        frame_times: Vec::new(),
    };

    for entry in &demo.directory.entries {
        for frame in &entry.frames {
            match &frame.frame_data {
                FrameData::WeaponAnimation(wa) => {
                    c.frame_anim += 1;
                    *c.sequences.entry(wa.anim).or_insert(0) += 1;
                    c.frame_times.push(frame.time);
                }
                FrameData::NetworkMessage(bt) => {
                    let MessageData::Parsed(msgs) = &bt.1.messages else { continue };
                    for m in msgs {
                        let NetMessage::EngineMessage(em) = m else { continue };
                        match &**em {
                            EngineMessage::SvcWeaponAnim(wa) => {
                                c.svc_anim += 1;
                                *c.sequences.entry(wa.sequence_number as i32).or_insert(0) += 1;
                                c.svc_times.push(frame.time);
                            }
                            EngineMessage::SvcDirector(d) => {
                                if d.command != DRC_CMD_MESSAGE && d.command != DRC_CMD_STUFFTEXT {
                                    c.director += 1;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Some(c)
}

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    assert!(!paths.is_empty(), "usage: weapon_anim_probe <demo>...");

    for path in &paths {
        let Ok(bytes) = std::fs::read(path) else {
            println!("{path}: unreadable\n");
            continue;
        };
        let name = std::path::Path::new(path).file_name().unwrap_or_default().to_string_lossy();
        let Some(mut c) = measure(&bytes) else {
            println!("{name}: unparseable\n");
            continue;
        };

        let kind = if c.director > 0 { "HLTV" } else { "POV" };
        let total = c.svc_anim + c.frame_anim;
        println!("=== {name} ({kind}) ===");
        println!("  svc_weaponanim messages : {}", c.svc_anim);
        println!("  Dem_WeaponAnim frames   : {}", c.frame_anim);

        if total == 0 {
            // The point of the exercise when it happens on an HLTV demo: the
            // engine never tells a spectator which viewmodel animation to play,
            // so there is nothing to read and the fix has to infer.
            println!("  -> carries NO viewmodel animation at all\n");
            continue;
        }

        // The demo-frame carrier is the fuller of the two, so time off it.
        let mut times = if c.frame_times.len() >= c.svc_times.len() { c.frame_times } else { c.svc_times };
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let span = times.last().copied().unwrap_or(0.0) - times.first().copied().unwrap_or(0.0);
        println!("  spanning {:.0}s, {:.1} per minute", span, 60.0 * times.len() as f32 / span.max(1.0));

        let mut seqs: Vec<(i32, usize)> = c.sequences.into_iter().collect();
        seqs.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let shown: Vec<String> = seqs.iter().take(12).map(|(s, n)| format!("{s}:{n}")).collect();
        println!("  sequence:count          : {}", shown.join(", "));

        // How close together the engine is willing to restart the viewmodel.
        // The fix's own de-duplication window has to sit under this or it would
        // swallow animations the engine would have played.
        let mut gaps: Vec<f32> = times.windows(2).map(|w| w[1] - w[0]).filter(|g| *g > 0.0).collect();
        gaps.sort_by(|a, b| a.partial_cmp(b).unwrap());
        if !gaps.is_empty() {
            let pick = |f: f32| gaps[((gaps.len() - 1) as f32 * f) as usize];
            println!(
                "  gap between animations  : min {:.3}s, p10 {:.3}s, median {:.3}s",
                gaps[0],
                pick(0.10),
                pick(0.50)
            );
        }
        println!();
    }
}
