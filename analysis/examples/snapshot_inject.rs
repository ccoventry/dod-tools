//! Rebuild the client's entity table at a join by injecting a synthetic full
//! snapshot (#224, and the remaining jank in #15).
//!
//! A stitched demo plays, but the engine prints `WARNING: CL_FlushEntityPacket`
//! and world objects -- crates, doors, props -- go missing. The tail's deltas
//! describe changes to an entity table the client never built, so it discards
//! whole packets rather than applying them. Players keep moving because they are
//! re-sent constantly with absolute origins; a static crate gets one update,
//! dropped, and is never corrected.
//!
//! The fix is to give the client a complete table at the cut. This replays every
//! entity update in the prefix, merging each entity's fields into one accumulated
//! state, and writes that out as a full `SvcPacketEntities` at the join -- the
//! message type the demo otherwise only contains at t=0.
//!
//!     cargo run --release -p analysis --example snapshot_inject -- <in.dem> <out.dem>
use dem::open_demo_from_bytes;
use dem::types::{Delta, EngineMessage, EntityState, FrameData, MessageData, NetMessage, SvcPacketEntities};
use std::collections::BTreeMap;

#[derive(Clone, Default)]
struct EntAcc {
    fields: Delta,
    custom: bool,
}

fn main() {
    let mut a = std::env::args().skip(1);
    let input = a.next().expect("usage: snapshot_inject <in.dem> <out.dem>");
    let output = a.next().expect("out.dem");
    let bytes = std::fs::read(&input).expect("read");
    let mut demo = open_demo_from_bytes(&bytes).expect("parse");

    // Find the join by the jump in `incoming_sequence`, not in frame time.
    // Frame time is not monotonic across a whole recording -- `m1_h2` resets to
    // 0.00s inside its intact prefix -- and the largest forward time jump then
    // lands at the start of playback rather than at the cut, which replays no
    // entities at all and injects an empty snapshot over a healthy packet.
    // `incoming_sequence` advances by one per network frame and jumps by
    // thousands at a splice, so it names the join whatever the clock does.
    let mut join_idx = 0usize;
    let mut join_time = 0.0f32;
    let mut best = 0i64;
    {
        let mut idx = 0usize;
        let mut prev: Option<(i32, f32)> = None;
        for entry in demo.directory.entries.iter().skip(1) {
            for f in &entry.frames {
                if let FrameData::NetworkMessage(bt) = &f.frame_data {
                    let seq = bt.1.sequence_info.incoming_sequence;
                    if let Some((p, pt)) = prev {
                        let jump = (seq as i64 - p as i64).abs();
                        if jump > best { best = jump; join_idx = idx; join_time = pt; }
                    }
                    prev = Some((seq, f.time));
                }
                idx += 1;
            }
        }
    }
    println!("join at frame {join_idx} (t={join_time:.2}s), incoming_sequence jumps by {best}");

    // Replay the prefix, merging every entity's fields into one state.
    let mut acc: BTreeMap<u16, EntAcc> = BTreeMap::new();
    let mut updates = 0usize;
    let mut idx = 0usize;
    for entry in demo.directory.entries.iter().skip(1) {
        for f in &entry.frames {
            let here = idx;
            idx += 1;
            if here > join_idx { continue }
            let FrameData::NetworkMessage(bt) = &f.frame_data else { continue };
            let MessageData::Parsed(msgs) = &bt.1.messages else { continue };
            for m in msgs {
                let NetMessage::EngineMessage(em) = m else { continue };
                match &**em {
                    EngineMessage::SvcPacketEntities(pe) => {
                        for es in &pe.entity_states {
                            let e = acc.entry(es.entity_index).or_default();
                            e.custom = es.has_custom_delta;
                            for (k, v) in es.delta.iter() { e.fields.insert(k.clone(), v.clone()); }
                            updates += 1;
                        }
                    }
                    EngineMessage::SvcDeltaPacketEntities(pe) => {
                        for es in &pe.entity_states {
                            if es.remove_entity { acc.remove(&es.entity_index); continue }
                            let Some(d) = &es.delta else { continue };
                            let e = acc.entry(es.entity_index).or_default();
                            if let Some(c) = es.has_custom_delta { e.custom = c; }
                            for (k, v) in d.iter() { e.fields.insert(k.clone(), v.clone()); }
                            updates += 1;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    println!("replayed {updates} entity updates -> {} live entities at the join", acc.len());
    let players = acc.keys().filter(|i| **i >= 1 && **i <= 32).count();
    println!("  {players} player-slot entities, {} world entities", acc.len() - players);

    // Encode them as one full snapshot. Absolute indices throughout: unambiguous,
    // and it avoids depending on the incremental index arithmetic being right.
    let mut states: Vec<EntityState> = Vec::with_capacity(acc.len());
    for (idx, e) in &acc {
        states.push(EntityState {
            entity_index: *idx,
            increment_entity_number: false,
            is_absolute_entity_index: Some(true),
            absolute_entity_index: Some(dem::nbit_num!(*idx as u32, 11)),
            entity_index_difference: None,
            has_custom_delta: e.custom,
            has_baseline_index: false,
            baseline_index: None,
            delta: e.fields.clone(),
        });
    }
    let snapshot = SvcPacketEntities {
        entity_count: dem::nbit_num!(states.len() as u32, 16),
        entity_states: states,
    };

    // *Replace* the first post-join delta packet rather than sitting in front of
    // it. Both messages would share one frame, so they share one
    // incoming_sequence and one slot in the client's frame ring -- whichever is
    // parsed second is the state that survives. Prepending the snapshot means
    // the delta still lands on top of it, resolved against a ring slot that
    // holds a frame from the *prefix*, which is the state this whole probe
    // exists to stop the client from using.
    let mut injected = false;
    let mut idx = 0usize;
    for entry in demo.directory.entries.iter_mut().skip(1) {
        for f in entry.frames.iter_mut() {
            let here = idx;
            idx += 1;
            if injected || here <= join_idx { continue }
            let FrameData::NetworkMessage(bt) = &mut f.frame_data else { continue };
            let MessageData::Parsed(msgs) = &mut bt.1.messages else { continue };
            let Some(at) = msgs.iter().position(|m| matches!(m, NetMessage::EngineMessage(em)
                if matches!(**em, EngineMessage::SvcDeltaPacketEntities(_)))) else { continue };
            msgs[at] = NetMessage::EngineMessage(Box::new(
                EngineMessage::SvcPacketEntities(snapshot.clone())));
            println!("replaced the first post-join delta packet with the snapshot at t={:.2}s", f.time);
            injected = true;
        }
    }
    if !injected { println!("no post-join frame carried entity data; nothing injected"); return }

    let out = demo.write_to_bytes();
    std::fs::write(&output, &out).expect("write");
    println!("wrote {output} ({:.1} MB)", out.len() as f64 / 1e6);
    match open_demo_from_bytes(&out) {
        Ok(d) => {
            let fulls: usize = d.directory.entries.iter().flat_map(|e| e.frames.iter())
                .filter_map(|f| if let FrameData::NetworkMessage(bt) = &f.frame_data {
                    if let MessageData::Parsed(ms) = &bt.1.messages { Some(ms) } else { None } } else { None })
                .flatten()
                .filter(|m| matches!(m, NetMessage::EngineMessage(em)
                    if matches!(**em, EngineMessage::SvcPacketEntities(_)))).count();
            println!("re-parse OK: {} frames, {fulls} full snapshots now present",
                d.directory.entries.iter().map(|e| e.frames.len()).sum::<usize>());
        }
        Err(e) => println!("re-parse FAILED: {e}"),
    }
    match analysis::Analysis::try_from_bytes(&out) {
        Ok(an) => println!("analysis OK: map={:?} players={}", an.state.initial_map_name, an.state.players.len()),
        Err(e) => println!("analysis failed: {e}"),
    }
}
