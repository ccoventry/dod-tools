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
//! entity update in the prefix -- seeded from entry 0's `SvcSpawnBaseline`, so a
//! field set only at spawn and genuinely never resent survives too -- merging
//! each entity's fields into one accumulated state, and writes that out as a
//! full `SvcPacketEntities` at the join -- the message type the demo otherwise
//! only contains at t=0.
//!
//! **Two live-crash fixes are baked into how that packet is encoded**, both
//! found by testing in-engine rather than by offline checks (which all still
//! pass on the broken versions): entity 0 is dropped from the list entirely
//! rather than restated, and its entities are encoded with increment/difference
//! indexing rather than absolute, matching what real `SvcPacketEntities`
//! traffic actually does. See the comment just above where the snapshot is
//! built for the measurements behind both.
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
    // And find *every* join, not just the largest: `multi_bridge` splices a demo
    // once per hole, so `m1_h1` carries two and `m1_h2` three. A snapshot at only
    // one of them leaves the others resolving deltas against entity state the
    // client never built.
    let mut joins: Vec<(usize, f32, i64)> = Vec::new();
    {
        let mut idx = 0usize;
        let mut prev: Option<(i32, f32)> = None;
        for entry in demo.directory.entries.iter().skip(1) {
            for f in &entry.frames {
                if let FrameData::NetworkMessage(bt) = &f.frame_data {
                    let seq = bt.1.sequence_info.incoming_sequence;
                    if let Some((p, pt)) = prev {
                        // A network frame advances the sequence by one; a splice
                        // moves it by thousands. Anything past a handful is a cut.
                        let jump = (seq as i64 - p as i64).abs();
                        if jump > 100 { joins.push((idx, pt, jump)) }
                    }
                    prev = Some((seq, f.time));
                }
                idx += 1;
            }
        }
    }
    if joins.is_empty() { println!("no join found -- nothing to inject"); return }
    println!("{} join(s) found:", joins.len());
    for (i, t, j) in &joins { println!("  frame {i} (t={t:.2}s), incoming_sequence jumps by {j}") }
    // Replay the whole demo once, merging every entity's fields into one state,
    // and take a copy of that state as each join goes past. Replaying up to each
    // join separately would be the same work over again per join.
    //
    // Seeded from entry 0's SvcSpawnBaseline first, not left empty. A field set
    // only at spawn and genuinely never resent -- worldspawn's modelindex is the
    // case that surfaced this: entity 0 moves and changes for nobody, so no
    // delta in the whole recording ever touches it again -- would otherwise be
    // absent from every injected snapshot. The client does not re-seed a full
    // update from the baseline on its own; it instantiates the entity from
    // exactly the fields the packet lists. Missing modelindex on entity 0 is
    // `SV_LinkEdict`'s "Tried to link edict 0 without model", spammed once per
    // tick from the join onward -- measured live: 1,658 occurrences, then the
    // process gone with no crash dialog.
    let mut acc: BTreeMap<u16, EntAcc> = BTreeMap::new();
    if let Some(entry0) = demo.directory.entries.first() {
        for f in &entry0.frames {
            let FrameData::NetworkMessage(bt) = &f.frame_data else { continue };
            let MessageData::Parsed(msgs) = &bt.1.messages else { continue };
            for m in msgs {
                let NetMessage::EngineMessage(em) = m else { continue };
                let EngineMessage::SvcSpawnBaseline(sb) = &**em else { continue };
                for es in &sb.entities {
                    let e = acc.entry(es.entity_index).or_default();
                    for (k, v) in es.delta.iter() { e.fields.insert(k.clone(), v.clone()); }
                }
            }
        }
    }
    let baseline_seeded = acc.len();
    let mut at_join: Vec<BTreeMap<u16, EntAcc>> = Vec::new();
    let mut next_join = 0usize;
    let mut updates = 0usize;
    let mut idx = 0usize;
    'replay: for entry in demo.directory.entries.iter().skip(1) {
        for f in &entry.frames {
            let here = idx;
            idx += 1;
            while next_join < joins.len() && here > joins[next_join].0 {
                at_join.push(acc.clone());
                next_join += 1;
            }
            if next_join >= joins.len() { break 'replay }
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
    while at_join.len() < joins.len() { at_join.push(acc.clone()) }
    println!("baseline seeded {baseline_seeded} entities; replayed {updates} entity updates on top");

    // Encode each join's state as a full snapshot. Absolute indices throughout:
    // unambiguous, and it avoids depending on the incremental index arithmetic
    // being right.
    // Two encoding choices below came from a live svc_bad crash (playdemo and
    // viewdemo both, same underlying cause) and are backed by measurement, not
    // guesswork -- checked across 3 healthy demos, ~1.65M real entity
    // observations combined:
    //
    // - entity_index 0 (world) appears in ZERO of them, full snapshots or
    //   deltas. The client keeps its signon baseline for it permanently; no
    //   real server ever restates it. So it is dropped here rather than
    //   encoded, matching real traffic exactly rather than inventing a
    //   representation nothing has ever validated.
    // - `is_absolute_entity_index = true` appears in 0 of 2,337 entities across
    //   every real *full* `SvcPacketEntities` sampled -- yet in ~4% of entities
    //   in real *delta* packets. So the 11-bit absolute-index path is a
    //   genuinely exercised, presumably-correct part of the wire format, but
    //   specifically for the message type we are NOT constructing here. The
    //   previous version of this code used absolute indexing for every single
    //   entity in the injected full snapshot, exercising a path with zero
    //   precedent for that message type -- and does not reach the parser via a
    //   normal read either, only the injector's own writer. Increment/
    //   difference encoding, computed against a running index starting at 0
    //   (dem-patch's own decoder, `packet_entities.rs`), is used instead,
    //   falling back to absolute only for a gap too wide for 6 bits (>63) --
    //   never observed in real full snapshots, but not structurally
    //   impossible for a set reconstructed from scratch rather than walked
    //   live by a server.
    let snapshots: Vec<SvcPacketEntities> = at_join.iter().enumerate().map(|(n, state)| {
        let live: Vec<(&u16, &EntAcc)> = state.iter().filter(|(idx, _)| **idx != 0).collect();
        let players = live.iter().filter(|(i, _)| **i >= 1 && **i <= 32).count();
        println!("  join {}: {} live entities ({players} player-slot, {} world, entity 0 excluded)",
            n + 1, live.len(), live.len() - players);

        let mut running: u32 = 0;
        let mut states: Vec<EntityState> = Vec::with_capacity(live.len());
        for (idx, e) in live {
            let idx32 = *idx as u32;
            let gap = idx32 - running; // ascending BTreeMap keys, entity 0 excluded: always > 0
            let (increment_entity_number, is_absolute_entity_index, absolute_entity_index, entity_index_difference) =
                if gap == 1 {
                    (true, None, None, None)
                } else if gap <= 63 {
                    (false, Some(false), None, Some(dem::nbit_num!(gap, 6)))
                } else {
                    (false, Some(true), Some(dem::nbit_num!(idx32, 11)), None)
                };
            running = idx32;
            states.push(EntityState {
                entity_index: *idx,
                increment_entity_number,
                is_absolute_entity_index,
                absolute_entity_index,
                entity_index_difference,
                has_custom_delta: e.custom,
                has_baseline_index: false,
                baseline_index: None,
                delta: e.fields.clone(),
            });
        }
        SvcPacketEntities {
            entity_count: dem::nbit_num!(states.len() as u32, 16),
            entity_states: states,
        }
    }).collect();

    // *Replace* the first post-join delta packet rather than sitting in front of
    // it. Both messages would share one frame, so they share one
    // incoming_sequence and one slot in the client's frame ring -- whichever is
    // parsed second is the state that survives. Prepending the snapshot means
    // the delta still lands on top of it, resolved against a ring slot that
    // holds a frame from the *prefix*, which is the state this whole probe
    // exists to stop the client from using.
    let mut injected = 0usize;
    let mut idx = 0usize;
    for entry in demo.directory.entries.iter_mut().skip(1) {
        for f in entry.frames.iter_mut() {
            let here = idx;
            idx += 1;
            if injected >= joins.len() || here <= joins[injected].0 { continue }
            let FrameData::NetworkMessage(bt) = &mut f.frame_data else { continue };
            let MessageData::Parsed(msgs) = &mut bt.1.messages else { continue };
            let Some(at) = msgs.iter().position(|m| matches!(m, NetMessage::EngineMessage(em)
                if matches!(**em, EngineMessage::SvcDeltaPacketEntities(_)))) else { continue };
            msgs[at] = NetMessage::EngineMessage(Box::new(
                EngineMessage::SvcPacketEntities(snapshots[injected].clone())));
            println!("replaced the first delta packet after join {} with its snapshot, at t={:.2}s",
                injected + 1, f.time);
            injected += 1;
        }
    }
    if injected < joins.len() {
        println!("only {injected} of {} joins got a snapshot -- no frame carrying entity data followed the rest",
            joins.len());
    }
    if injected == 0 { return }

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
