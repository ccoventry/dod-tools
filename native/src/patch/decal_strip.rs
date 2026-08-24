// patch/decal_strip.rs
// Decal hygiene for capture demos: keeps walls clean at the start of every
// recorded clip without reloading the demo.
//
// ── Why r_decals alone cannot do this ────────────────────────────────────────
// GoldSrc stores decals in a fixed pool with a single rotating index:
//
//     limit = min(r_decals, MAX_RENDER_DECALS)
//     if (gDecalCount >= limit) gDecalCount = 0;
//     pdecal = &gDecalPool[gDecalCount++];
//     R_DecalUnlink(pdecal);          // the ONLY path that clears an old decal
//
// A decal is removed *only* when the ring index lands on its slot. The cvar
// never sweeps anything — it just bounds how far the index may travel before
// wrapping. Lowering r_decals mid-demo therefore strands every decal sitting in
// a slot >= the new limit: the index can no longer reach them, so they stay on
// the wall permanently while new decals churn through the small surviving
// window. (That asymmetry is exactly what you see in game when dropping
// r_decals from 5555 to 1.) The clean walls at demo load come from
// R_ClearDecals() on level load, not from the cvar.
//
// ── What this module does instead ────────────────────────────────────────────
// Two complementary passes over the demo's own byte stream:
//
//  1. STRIP — replace decal-creating messages outside the capture windows with
//     SvcNop, so the fast-forwarded stretches between clips contribute no
//     buildup at all. SvcNop is one byte on the wire and demo_writer recomputes
//     payload lengths, so no manual offset math is involved.
//
//  2. FLUSH BURST — pin r_decals to a modest ring size, then inject exactly
//     that many synthetic decals into the gap before each clip. That walks the
//     ring index a full revolution, unlinking every real decal still on a wall.
//     The synthetic decals are stacked at one coordinate near the player's
//     spawn (see `resolve_flush_position`) so they land where the capture
//     camera never looks.
//
// Pinning the ring small is what makes the burst cheap: a sweep costs
// `ring_limit` injections regardless of how many decals are actually out there,
// so 256 keeps a full flush at ~2KB of injected messages instead of the ~36KB a
// 4096-slot ring would need.

use dem::open_demo_from_bytes;
use dem::types::{
    ByteString, ConsoleCommand, EngineMessage, Frame, FrameData, MessageData, NetMessage,
    SvcTempEntity, TempEntity,
};

/// Temp-entity `entity_type` values that place a persistent decal onto a wall
/// or world surface: bullet holes, grenade scorch marks, generic BSP/world
/// decals. Deliberately excludes TePlayerDecal (112) — blood on a player model
/// isn't wall clutter and clears itself when that player respawns.
const WALL_DECAL_ENTITY_TYPES: &[u8] = &[13, 104, 109, 116, 117, 118];

/// TE_WORLDDECAL. 7 bytes: 3 × WRITE_COORD + 1 × WRITE_BYTE texture index.
/// Chosen for the flush burst because it takes no entity index and — unlike
/// TE_GUNSHOTDECAL — plays no ricochet sound.
const TE_WORLDDECAL: u8 = 116;

/// Distance from a standing player's origin down to the floor: the origin sits
/// at the centre of a 72-unit hull, so the feet are 36 below it.
const ORIGIN_TO_FLOOR: f32 = 36.0;

#[derive(Debug, Clone)]
pub struct DecalCleanOptions {
    /// Blank decal messages outside the capture windows.
    pub strip_outside_windows: bool,
    /// Inject ring-sweeping decal bursts ahead of each capture window.
    pub flush_burst: bool,
    /// Value r_decals is pinned to. The burst size follows from this: a full
    /// ring revolution is what guarantees every occupied slot gets unlinked.
    /// Must NOT be changed anywhere else in the demo — lowering it later
    /// strands decals in the slots above the new limit.
    pub ring_limit: u32,
    /// Extra injections beyond `ring_limit`. A decal spanning several surfaces
    /// consumes one pool slot per surface, so the sweep is deliberately
    /// over-provisioned rather than counted 1:1 against messages.
    pub burst_margin: usize,
    /// Cap on synthetic decals added to any single network packet, so injection
    /// never meaningfully grows a frame the engine already sized.
    pub max_per_frame: usize,
    /// Finish the burst this many ticks before the capture window opens.
    pub lead_ticks: i32,
    /// Emit `r_decals <ring_limit>` as a console-command frame at playback
    /// start, making a patched demo self-contained for testing.
    pub inject_r_decals_command: bool,
    /// Hand-picked flush coordinate, overriding spawn detection.
    pub flush_coord: Option<[f32; 3]>,
    /// Vertical drop applied to the settled spawn origin to reach the floor.
    /// Only used when the demo yielded no real decal to anchor to.
    pub floor_drop: f32,
    /// Consecutive on-ground frames with a stable Z required before a spawn
    /// position is trusted, so a player still falling from an elevated spawn
    /// point is never sampled mid-air.
    pub grounded_settle_frames: usize,
    /// How far the flush coordinate must stay from every camera position
    /// recorded inside a capture window. Distance is only a proxy for
    /// visibility — a long sightline can still expose a distant spot — but it
    /// reliably rules out the camera walking straight over the flush point.
    pub min_camera_clearance: f32,
    /// Half-angle of the cone treated as "on screen" for the line-of-sight
    /// test. DoD's default FOV is ~90 degrees horizontal, so 40 is a slightly
    /// generous half-angle.
    pub visibility_cone_degrees: f32,
    /// Beyond this range a single decal is not readable on screen, so the
    /// line-of-sight test stops caring.
    pub visibility_max_distance: f32,
}

impl Default for DecalCleanOptions {
    fn default() -> Self {
        Self {
            strip_outside_windows: true,
            flush_burst: true,
            ring_limit: 256,
            burst_margin: 16,
            max_per_frame: 4,
            lead_ticks: 300,
            inject_r_decals_command: true,
            flush_coord: None,
            floor_drop: ORIGIN_TO_FLOOR,
            grounded_settle_frames: 10,
            min_camera_clearance: 900.0,
            visibility_cone_degrees: 40.0,
            visibility_max_distance: 1800.0,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct DecalCleanStats {
    pub temp_entity_stripped: usize,
    pub player_spray_stripped: usize,
    pub flush_decals_injected: usize,
    pub bursts_placed: usize,
    /// Windows whose gap had too little room to fit a full sweep. These clips
    /// are NOT guaranteed clean, so they are reported rather than silently
    /// under-flushed.
    pub bursts_short: Vec<(i32, usize, usize)>,
    pub flush_coord: Option<[f32; 3]>,
    pub flush_source: Option<FlushSource>,
    pub flush_texture_index: Option<u8>,
    /// Settled on-ground spawn origin, when one was found.
    pub spawn_reference: Option<[f32; 3]>,
    /// How far the flush coordinate ended up from that spawn reference.
    pub spawn_to_flush_distance: Option<f32>,
    pub harvested_decals: usize,
    /// Closest approach between the flush coordinate and any camera position
    /// inside a capture window.
    pub min_camera_distance: Option<f32>,
    /// Sampled in-window camera frames where the flush point falls inside the
    /// camera's cone. Must be 0 — anything else means the flush stack is on
    /// screen during a recorded clip.
    pub flush_on_camera_frames: usize,
    /// Total in-window camera samples the two figures above were measured over.
    pub camera_samples: usize,
}

fn decode_coord(b: &[u8]) -> f32 {
    i16::from_le_bytes([b[0], b[1]]) as f32 / 8.0
}

fn encode_coord(v: f32) -> [u8; 2] {
    ((v * 8.0).round() as i16).to_le_bytes()
}

/// Texture index carried by a decal temp entity, by wire layout.
/// TE_WORLDDECAL/HIGH: coord(6) + index. TE_GUNSHOTDECAL/TE_DECALHIGH:
/// coord(6) + entity(2) + index. TE_DECAL: coord(6) + index + entity(2).
fn decal_texture_index(entity_type: u8, payload: &[u8]) -> Option<u8> {
    match entity_type {
        116 | 117 if payload.len() >= 7 => Some(payload[6]),
        104 if payload.len() >= 7 => Some(payload[6]),
        109 | 118 if payload.len() >= 9 => Some(payload[8]),
        _ => None,
    }
}

fn decal_position(payload: &[u8]) -> Option<[f32; 3]> {
    if payload.len() < 6 {
        return None;
    }
    Some([
        decode_coord(&payload[0..2]),
        decode_coord(&payload[2..4]),
        decode_coord(&payload[4..6]),
    ])
}

fn distance(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    let (dx, dy, dz) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn build_world_decal(pos: &[f32; 3], texture_index: u8) -> NetMessage {
    let mut payload = Vec::with_capacity(7);
    payload.extend_from_slice(&encode_coord(pos[0]));
    payload.extend_from_slice(&encode_coord(pos[1]));
    payload.extend_from_slice(&encode_coord(pos[2]));
    payload.push(texture_index);

    NetMessage::EngineMessage(Box::new(EngineMessage::SvcTempEntity(SvcTempEntity {
        entity_type: TE_WORLDDECAL,
        entity: TempEntity::TeWorldDecal(payload),
    })))
}

/// Everything the survey pass needs to pick a flush coordinate and place bursts.
struct Survey {
    /// Positions of decals the engine actually accepted during playback.
    harvested: Vec<[f32; 3]>,
    texture_index: Option<u8>,
    /// Earliest camera eye position seen in playback.
    spawn_eye: Option<[f32; 3]>,
    /// Player origin once the spawn has settled onto solid ground — see the
    /// grounded-run detection below. This, not `spawn_eye`, is the spawn
    /// reference worth trusting.
    grounded_origin: Option<[f32; 3]>,
    /// Camera (eye position, forward vector) pairs sampled inside the capture
    /// windows. The forward vector is what makes a real "is it on screen?"
    /// test possible, rather than distance alone.
    window_cameras: Vec<([f32; 3], [f32; 3])>,
}

/// Running frame ordinal, matching `engine.rs`'s `frame_counter`: every frame
/// record in file order, across all directory entries, 1-based.
///
/// This — NOT `Frame::frame` — is the tick space the rest of the patch pipeline
/// schedules in. `Frame::frame` is the engine's tick, and several frame records
/// share one of those (a DemoBuffer, a ClientData and a NetworkMessage per
/// tick), so the two spaces differ by roughly 2.6x on a real demo. Mixing them
/// silently targets the wrong frames.
fn frame_ordinals(demo: &dem::types::Demo) -> Vec<(usize, usize, i32)> {
    let mut out = Vec::new();
    let mut ordinal = 0i32;
    for (entry_idx, entry) in demo.directory.entries.iter().enumerate() {
        for frame_idx in 0..entry.frames.len() {
            ordinal += 1;
            out.push((entry_idx, frame_idx, ordinal));
        }
    }
    out
}

fn in_window(ordinal: i32, keep_windows: &[(i32, i32)]) -> bool {
    keep_windows.iter().any(|&(s, e)| ordinal >= s && ordinal <= e)
}

fn survey(demo: &dem::types::Demo, keep_windows: &[(i32, i32)], opts: &DecalCleanOptions) -> Survey {
    let mut out = Survey {
        harvested: Vec::new(),
        texture_index: None,
        spawn_eye: None,
        grounded_origin: None,
        window_cameras: Vec::new(),
    };
    // A world decal's index is read straight from the byte after the coords,
    // so prefer harvesting from the same message type we intend to emit.
    let mut fallback_index: Option<u8> = None;

    // Spawn points are not guaranteed to sit flush on the floor — many maps
    // place them slightly above it and let the player drop. Sampling at the
    // spawn instant can therefore return a mid-air position, whose "floor"
    // would be open space. So wait for a run of consecutive frames that report
    // on_ground with a stable Z before trusting the position.
    let mut grounded_run = 0usize;
    let mut last_z: Option<f32> = None;
    let mut camera_stride = 0usize;

    let mut ordinal = 0i32;
    for entry in &demo.directory.entries {
        for frame in &entry.frames {
            ordinal += 1;
            let FrameData::NetworkMessage(net_msg_box) = &frame.frame_data else {
                continue;
            };

            let rp = &net_msg_box.1.info.refparams;
            let origin = &rp.view_origin;
            if origin.len() >= 3 {
                let pos = [origin[0], origin[1], origin[2]];
                if pos != [0.0, 0.0, 0.0] {
                    if out.spawn_eye.is_none() {
                        out.spawn_eye = Some(pos);
                    }

                    if out.grounded_origin.is_none() {
                        let settled = last_z.map(|z| (pos[2] - z).abs() < 2.0).unwrap_or(false);
                        if rp.on_ground != 0 && settled {
                            grounded_run += 1;
                            if grounded_run >= opts.grounded_settle_frames {
                                // Prefer the engine's own player origin; fall
                                // back to backing the view offset out of the
                                // eye position when sim_org isn't populated.
                                let sim = &rp.sim_org;
                                let resolved = if sim.len() >= 3 && sim[2] != 0.0 {
                                    [sim[0], sim[1], sim[2]]
                                } else {
                                    let view_z = rp.view_height.get(2).copied().unwrap_or(28.0);
                                    [pos[0], pos[1], pos[2] - view_z]
                                };
                                out.grounded_origin = Some(resolved);
                            }
                        } else {
                            grounded_run = 0;
                        }
                        last_z = Some(pos[2]);
                    }

                    // Subsampled: every candidate decal is scored against this
                    // whole set, and consecutive frames sit a few units apart,
                    // so a stride costs no meaningful accuracy.
                    if in_window(ordinal, keep_windows) {
                        camera_stride += 1;
                        if camera_stride % 4 == 0 {
                            let fwd = &rp.forward;
                            if fwd.len() >= 3 {
                                out.window_cameras.push((pos, [fwd[0], fwd[1], fwd[2]]));
                            }
                        }
                    }
                }
            }

            let MessageData::Parsed(messages) = &net_msg_box.1.messages else {
                continue;
            };
            for msg in messages {
                let NetMessage::EngineMessage(eng) = msg else {
                    continue;
                };
                let EngineMessage::SvcTempEntity(te) = eng.as_ref() else {
                    continue;
                };
                // TE_BSPDECAL carries its fields in a different shape to the
                // rest (a 16-bit texture index rather than 8-bit, ahead of the
                // entity index), so it is unpacked separately instead of being
                // forced through the shared byte-offset table. It was already
                // in the strip set; leaving it out of the harvest set meant a
                // demo whose only decals were BSP decals offered no anchor.
                if let TempEntity::TeBspDecal(d) = &te.entity {
                    if let Some(pos) = decal_position(&d.unknown1) {
                        out.harvested.push(pos);
                    }
                    if d.unknown1.len() >= 8 {
                        let raw = i16::from_le_bytes([d.unknown1[6], d.unknown1[7]]);
                        // The emitted TE_WORLDDECAL writes this index as one
                        // byte, so an index that doesn't fit is unusable.
                        if (0..=255).contains(&raw) {
                            fallback_index.get_or_insert(raw as u8);
                        }
                    }
                    continue;
                }

                let payload: &[u8] = match &te.entity {
                    TempEntity::TeWorldDecal(p)
                    | TempEntity::TeWorldDecalHigh(p)
                    | TempEntity::TeGunshotDecal(p)
                    | TempEntity::TeDecal(p)
                    | TempEntity::TeDecalHigh(p) => p,
                    _ => continue,
                };
                if let Some(pos) = decal_position(payload) {
                    out.harvested.push(pos);
                }
                if let Some(idx) = decal_texture_index(te.entity_type, payload) {
                    if matches!(te.entity_type, 116 | 117) {
                        out.texture_index.get_or_insert(idx);
                    } else {
                        fallback_index.get_or_insert(idx);
                    }
                }
            }
        }
    }

    if out.texture_index.is_none() {
        out.texture_index = fallback_index;
    }
    out
}

/// How the flush coordinate was chosen, for reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlushSource {
    /// Caller supplied the coordinate outright.
    Override,
    /// A decal position lifted from the demo — the engine already accepted it,
    /// so the surface is proven — picked as the one nearest the spawn.
    HarvestedNearSpawn,
    /// Computed floor point beneath the settled spawn position. Geometrically
    /// derived rather than proven, so only used when nothing was harvested.
    ComputedSpawnFloor,
}

/// Picks where the synthetic flush decals go.
///
/// A coordinate that misses every surface produces no decal and therefore no
/// pool allocation, which would make the whole sweep silently do nothing. So
/// proven geometry is preferred over computed geometry:
///
///  1. An explicit override, when the caller has eyeballed a spot in game.
///  2. The harvested real decal nearest the spawn. Every harvested position is
///     one the engine actually rendered a decal at, so the surface is certain —
///     and picking the closest to spawn keeps it in the backfield, away from
///     wherever the highlight fighting happened.
///  3. The computed floor beneath the settled spawn position. Used only when
///     the demo yielded no decals at all; a spawn can sit above the floor on
///     some maps, so this is a geometric guess and is reported as such.
fn resolve_flush_position(
    survey: &Survey,
    opts: &DecalCleanOptions,
) -> Option<([f32; 3], FlushSource)> {
    if let Some(coord) = opts.flush_coord {
        return Some((coord, FlushSource::Override));
    }

    let reference = survey.grounded_origin.or(survey.spawn_eye)?;

    // Two independent disqualifiers, because neither alone is sufficient:
    //
    //  - Distance: the camera physically walking over the spot. Spawn is also
    //    the corridor players leave through, so the decal nearest spawn is a
    //    prime offender.
    //  - Line of sight: a spot 1200 units away, dead centre of frame down a
    //    long sightline, is plainly visible despite comfortable "clearance".
    //    Distance-only selection picks these, so the forward vector recorded
    //    with each in-window camera sample is used for a real frustum test.
    let cos_cone = opts.visibility_cone_degrees.to_radians().cos();

    let on_camera_frames = |pos: &[f32; 3]| -> usize {
        survey
            .window_cameras
            .iter()
            .filter(|(eye, fwd)| {
                let v = [pos[0] - eye[0], pos[1] - eye[1], pos[2] - eye[2]];
                let dist = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                if dist < 1.0 || dist > opts.visibility_max_distance {
                    return false;
                }
                let fl = (fwd[0] * fwd[0] + fwd[1] * fwd[1] + fwd[2] * fwd[2]).sqrt();
                if fl < 0.5 {
                    return false;
                }
                let dot = (v[0] * fwd[0] + v[1] * fwd[1] + v[2] * fwd[2]) / (dist * fl);
                dot >= cos_cone
            })
            .count()
    };

    let clearance = |pos: &[f32; 3]| -> f32 {
        survey
            .window_cameras
            .iter()
            .map(|(eye, _)| distance(pos, eye))
            .fold(f32::INFINITY, f32::min)
    };

    if !survey.harvested.is_empty() {
        let mut scored: Vec<([f32; 3], f32, usize)> = survey
            .harvested
            .iter()
            .map(|pos| (*pos, clearance(pos), on_camera_frames(pos)))
            .collect();

        // Never on screen during a recorded clip AND never walked over.
        let mut clear: Vec<&([f32; 3], f32, usize)> = scored
            .iter()
            .filter(|(_, c, seen)| *seen == 0 && *c >= opts.min_camera_clearance)
            .collect();

        if !clear.is_empty() {
            clear.sort_by(|a, b| {
                distance(&a.0, &reference)
                    .partial_cmp(&distance(&b.0, &reference))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            return Some((clear[0].0, FlushSource::HarvestedNearSpawn));
        }

        // Nothing was fully clear. Prefer the least-seen candidate, breaking
        // ties on distance; the caller reports both so a marginal pick is
        // visible rather than silent.
        scored.sort_by(|a, b| {
            a.2.cmp(&b.2)
                .then(b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
        });
        return Some((scored[0].0, FlushSource::HarvestedNearSpawn));
    }

    Some((
        [reference[0], reference[1], reference[2] - opts.floor_drop],
        FlushSource::ComputedSpawnFloor,
    ))
}

/// Strips decal messages outside `keep_windows` and injects ring-sweeping decal
/// bursts ahead of each window.
///
/// `keep_windows` are inclusive `[start, stop]` pairs in **frame-ordinal space**
/// — the same tick space `PatchJob::scheduled_commands` uses and `engine.rs`
/// compares against `frame_counter`, i.e. a 1-based count of every frame record
/// in file order. They should be the real record-start/record-stop ticks used to
/// schedule `mirv_recordmovie_start`/`stop`, not the wider highlight bounds.
///
/// These are NOT `Frame::frame` values. That field holds the engine tick, which
/// several frame records share, so the two spaces differ by roughly 2.6x on a
/// real demo — passing one where the other is expected silently targets the
/// wrong part of the demo.
pub fn clean_demo_decals(
    demo_bytes: &[u8],
    keep_windows: &[(i32, i32)],
    opts: &DecalCleanOptions,
) -> Result<(Vec<u8>, DecalCleanStats), String> {
    let mut demo = open_demo_from_bytes(demo_bytes)
        .map_err(|e| format!("Could not parse demo file: {}", e))?;

    let mut stats = DecalCleanStats::default();

    let survey = survey(&demo, keep_windows, opts);
    stats.harvested_decals = survey.harvested.len();
    stats.spawn_reference = survey.grounded_origin;
    stats.flush_texture_index = survey.texture_index;

    let resolved = resolve_flush_position(&survey, opts);
    let flush_pos = resolved.map(|(pos, _)| pos);
    stats.flush_coord = flush_pos;
    stats.flush_source = resolved.map(|(_, src)| src);

    if let (Some(pos), Some(reference)) = (flush_pos, survey.grounded_origin) {
        stats.spawn_to_flush_distance = Some(distance(&pos, &reference));
    }

    if let Some(pos) = flush_pos {
        stats.min_camera_distance = survey
            .window_cameras
            .iter()
            .map(|(eye, _)| distance(&pos, eye))
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Sampled in-window frames where the flush point would fall inside the
        // camera's cone. Non-zero means the stack is on screen during a
        // recorded clip — the failure this whole heuristic exists to avoid.
        let cos_cone = opts.visibility_cone_degrees.to_radians().cos();
        stats.camera_samples = survey.window_cameras.len();
        stats.flush_on_camera_frames = survey
            .window_cameras
            .iter()
            .filter(|(eye, fwd)| {
                let v = [pos[0] - eye[0], pos[1] - eye[1], pos[2] - eye[2]];
                let dist = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                if dist < 1.0 || dist > opts.visibility_max_distance {
                    return false;
                }
                let fl = (fwd[0] * fwd[0] + fwd[1] * fwd[1] + fwd[2] * fwd[2]).sqrt();
                if fl < 0.5 {
                    return false;
                }
                (v[0] * fwd[0] + v[1] * fwd[1] + v[2] * fwd[2]) / (dist * fl) >= cos_cone
            })
            .count();
    }

    // ── Pass 1: strip decal messages outside the capture windows ─────────────
    if opts.strip_outside_windows {
        let mut ordinal = 0i32;
        for entry in &mut demo.directory.entries {
            for frame in &mut entry.frames {
                ordinal += 1;
                if in_window(ordinal, keep_windows) {
                    continue;
                }
                let FrameData::NetworkMessage(net_msg_box) = &mut frame.frame_data else {
                    continue;
                };
                let MessageData::Parsed(messages) = &mut net_msg_box.1.messages else {
                    continue;
                };
                for msg in messages.iter_mut() {
                    let NetMessage::EngineMessage(eng) = msg else {
                        continue;
                    };
                    let strip = match eng.as_ref() {
                        EngineMessage::SvcTempEntity(te) => {
                            WALL_DECAL_ENTITY_TYPES.contains(&te.entity_type)
                        }
                        EngineMessage::SvcDecalName(_) => true,
                        _ => false,
                    };
                    if !strip {
                        continue;
                    }
                    if matches!(eng.as_ref(), EngineMessage::SvcDecalName(_)) {
                        stats.player_spray_stripped += 1;
                    } else {
                        stats.temp_entity_stripped += 1;
                    }
                    *eng = Box::new(EngineMessage::SvcNop);
                }
            }
        }
    }

    // ── Pass 2: flush bursts ahead of each capture window ────────────────────
    if opts.flush_burst {
        if let (Some(pos), Some(texture_index)) = (flush_pos, survey.texture_index) {
            let burst_count = opts.ring_limit as usize + opts.burst_margin;

            // Eligible carriers, in global frame order: parsed network frames
            // small enough that a handful of extra 9-byte messages cannot push
            // the packet near the engine's buffer ceiling. Built across every
            // entry at once so a window is never confined to one entry's frames.
            let eligible: Vec<(usize, usize, i32)> = frame_ordinals(&demo)
                .into_iter()
                .filter(|&(entry_idx, frame_idx, _)| {
                    demo.directory.entries[entry_idx]
                        .frames
                        .get(frame_idx)
                        .map(|frame| match &frame.frame_data {
                            FrameData::NetworkMessage(b) => {
                                matches!(b.1.messages, MessageData::Parsed(_))
                                    && b.1.message_length < 1024
                            }
                            _ => false,
                        })
                        .unwrap_or(false)
                })
                .collect();

            let mut used: std::collections::HashSet<(usize, usize)> =
                std::collections::HashSet::new();
            let mut plan: Vec<(usize, usize, usize)> = Vec::new();

            for &(window_start, _) in keep_windows {
                let deadline = window_start - opts.lead_ticks;
                let mut remaining = burst_count;

                // Walk backwards from the deadline so the sweep finishes as
                // late as possible — nothing after it can re-dirty a wall.
                let start_at = match eligible.iter().rposition(|&(_, _, ord)| ord <= deadline) {
                    Some(p) => p,
                    None => {
                        stats.bursts_short.push((window_start, 0, burst_count));
                        continue;
                    }
                };

                for slot in (0..=start_at).rev() {
                    if remaining == 0 {
                        break;
                    }
                    let (entry_idx, frame_idx, _) = eligible[slot];
                    if !used.insert((entry_idx, frame_idx)) {
                        continue;
                    }
                    let take = remaining.min(opts.max_per_frame);
                    plan.push((entry_idx, frame_idx, take));
                    remaining -= take;
                }

                if remaining > 0 {
                    stats
                        .bursts_short
                        .push((window_start, burst_count - remaining, burst_count));
                }
                if remaining < burst_count {
                    stats.bursts_placed += 1;
                }
            }

            for (entry_idx, frame_idx, count) in plan {
                let Some(frame) = demo
                    .directory
                    .entries
                    .get_mut(entry_idx)
                    .and_then(|e| e.frames.get_mut(frame_idx))
                else {
                    continue;
                };
                let FrameData::NetworkMessage(net_msg_box) = &mut frame.frame_data else {
                    continue;
                };
                let MessageData::Parsed(messages) = &mut net_msg_box.1.messages else {
                    continue;
                };
                for _ in 0..count {
                    messages.push(build_world_decal(&pos, texture_index));
                    stats.flush_decals_injected += 1;
                }
            }
        }
    }

    // ── Pin r_decals so the ring stays small and never strands a slot ────────
    if opts.inject_r_decals_command {
        let playback_idx = demo
            .directory
            .entries
            .iter()
            .position(|e| e.type_ == 1)
            .or_else(|| demo.directory.entries.len().checked_sub(1));

        if let Some(entry) = playback_idx.and_then(|i| demo.directory.entries.get_mut(i)) {
            // DemoStart (type 2) must be processed before any ConsoleCommand
            // (type 3), or the engine reads uninitialised memory.
            let insert_at = entry
                .frames
                .iter()
                .rposition(|f| matches!(f.frame_data, FrameData::DemoStart))
                .map(|p| p + 1)
                .unwrap_or(0);
            let anchor = entry
                .frames
                .get(insert_at)
                .or_else(|| entry.frames.first())
                .map(|f| (f.time, f.frame))
                .unwrap_or((0.0, 0));
            let cmd = format!("r_decals {}", opts.ring_limit);
            entry.frames.insert(
                insert_at,
                Frame {
                    time: anchor.0,
                    frame: anchor.1,
                    frame_data: FrameData::ConsoleCommand(ConsoleCommand {
                        command: ByteString::from(cmd.as_str()),
                    }),
                },
            );
            entry.frame_count = entry.frames.len() as i32;
        }
    }

    Ok((demo.write_to_bytes(), stats))
}

/// Strip-only entry point, kept for callers that just want the decal messages
/// outside `keep_windows` blanked with no burst injection or cvar pinning.
pub fn strip_decals_outside_windows(
    demo_bytes: &[u8],
    keep_windows: &[(i32, i32)],
) -> Result<(Vec<u8>, DecalCleanStats), String> {
    let opts = DecalCleanOptions {
        flush_burst: false,
        inject_r_decals_command: false,
        ..Default::default()
    };
    clean_demo_decals(demo_bytes, keep_windows, &opts)
}
