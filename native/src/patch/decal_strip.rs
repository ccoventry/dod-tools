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
/// decals, and player spray logos.
///
/// Note TE_PLAYERDECAL (112) belongs here: it is the spray-paint logo message
/// (`impulse 201`), writing a player index plus a position and decal index to
/// stamp that player's logo onto a surface. It is wall clutter in exactly the
/// sense this pass exists to remove.
const WALL_DECAL_ENTITY_TYPES: &[u8] = &[13, 104, 109, 112, 116, 117, 118];

/// TE_PLAYERDECAL — tracked separately in the stats so sprays are visible as
/// their own number rather than lost among bullet holes.
const TE_PLAYERDECAL: u8 = 112;

/// TE_WORLDDECAL. 7 bytes: 3 × WRITE_COORD + 1 × WRITE_BYTE texture index.
/// Chosen as the flush burst's carrier message because it takes no entity index
/// and — unlike TE_GUNSHOTDECAL — plays no ricochet sound. Note this is the
/// message type, independent of which texture index it is asked to draw.
const TE_WORLDDECAL: u8 = 116;

/// TE_GUNSHOTDECAL — the bullet-hole message. Not emitted (it plays a ricochet
/// sound), but its texture index is the one worth borrowing: a small hole
/// rather than the large scorch a TE_WORLDDECAL index usually denotes.
const TE_GUNSHOTDECAL: u8 = 109;

/// Distance from a standing player's origin down to the floor: the origin sits
/// at the centre of a 72-unit hull, so the feet are 36 below it.
const ORIGIN_TO_FLOOR: f32 = 36.0;

/// The engine's own `MAX_OVERLAP_DECALS`. `R_DecalCreate` counts how many
/// existing decals a new one would overlap, and once that reaches this many it
/// recycles one of them instead of allocating:
///
/// ```c
/// pold = R_DecalIntersect( decalinfo, surf, &count );
/// if( count < MAX_OVERLAP_DECALS ) pold = NULL;
/// ```
///
/// `R_DecalAlloc` only walks the ring when handed NULL, so a recycled decal
/// does NOT advance `gDecalCount`. This is why a flush burst must be spread
/// across distinct positions: piling every decal on one spot stops advancing
/// the ring after the sixth, and the sweep silently accomplishes nothing.
pub const MAX_OVERLAP_DECALS: usize = 6;

/// Flush decals to place at each distinct position. Kept below
/// `MAX_OVERLAP_DECALS` so every one of them allocates a fresh ring slot.
pub const DECALS_PER_POSITION: usize = MAX_OVERLAP_DECALS - 2;

/// Minimum spacing between two flush positions, so the engine cannot see them
/// as overlapping. Comfortably wider than a decal's own footprint.
const MIN_POSITION_SPACING: f32 = 28.0;

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
    /// Hand-picked decal texture index for the flush burst, overriding the
    /// harvested one. Useful when a demo contains no bullet-hole decal to
    /// borrow a small texture from and would otherwise fall back to a large
    /// grenade scorch.
    pub flush_texture_index: Option<u8>,
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
            flush_texture_index: None,
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
    /// Distinct positions the burst was spread across.
    pub flush_positions: usize,
    /// Positions needed to place the whole burst without any spot exceeding
    /// the engine's overlap limit. If `flush_positions` is below this, some
    /// injected decals get recycled instead of turning the ring.
    pub flush_positions_wanted: usize,
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
    /// Floor points sampled beneath the player wherever they stood on solid
    /// ground. A sweep needs far more distinct positions than a demo has
    /// decals, and every one of these is a surface the demo proves exists —
    /// the player was standing on it. Walking naturally spreads them out, so
    /// they satisfy the no-overlap requirement for free.
    floor_candidates: Vec<[f32; 3]>,
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
        floor_candidates: Vec::new(),
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

                    // Floor beneath the player wherever they are actually
                    // standing. Sampled sparsely and only when far enough from
                    // the last sample to be a genuinely separate spot.
                    if rp.on_ground != 0 {
                        let sim = &rp.sim_org;
                        let origin = if sim.len() >= 3 && sim[2] != 0.0 {
                            [sim[0], sim[1], sim[2]]
                        } else {
                            let vh = rp.view_height.get(2).copied().unwrap_or(28.0);
                            [pos[0], pos[1], pos[2] - vh]
                        };
                        let floor = [origin[0], origin[1], origin[2] - opts.floor_drop];
                        let far_enough = out
                            .floor_candidates
                            .last()
                            .map(|last| distance(&floor, last) >= MIN_POSITION_SPACING)
                            .unwrap_or(true);
                        if far_enough {
                            out.floor_candidates.push(floor);
                        }
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

                // A spray marks a proven surface just as well as a bullet hole,
                // so its position is worth harvesting — but its texture index
                // never is: that index is somebody's logo, and a stack of those
                // is the most conspicuous thing that could be left on a wall.
                // Its layout also differs (a leading player index before the
                // coordinates), hence the separate arm.
                if let TempEntity::TePlayerDecal(p) = &te.entity {
                    if p.len() >= 7 {
                        if let Some(pos) = decal_position(&p[1..7]) {
                            out.harvested.push(pos);
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
                    // Prefer a bullet-hole texture. TE_GUNSHOTDECAL indices are
                    // small and unremarkable; TE_WORLDDECAL ones are typically
                    // grenade scorches — large, dark and immediately obvious.
                    // Flush decals exist to be unnoticed, so the small mark
                    // wins and the scorch is only a fallback.
                    if te.entity_type == TE_GUNSHOTDECAL {
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
    /// Floor points under the player's own walked path. Proven surfaces (they
    /// stood on them) and naturally spread apart, which is what a sweep needs.
    PlayerFloorPath,
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
/// Picks the set of positions the flush burst is spread across.
///
/// `wanted` is how many distinct spots are needed to place the whole burst at
/// `DECALS_PER_POSITION` each. Returning fewer means the sweep will fall short,
/// which the caller reports rather than hiding.
fn resolve_flush_positions(
    survey: &Survey,
    opts: &DecalCleanOptions,
    wanted: usize,
) -> (Vec<[f32; 3]>, Option<FlushSource>) {
    if let Some(coord) = opts.flush_coord {
        return (vec![coord], Some(FlushSource::Override));
    }

    // Only the single-position fallback still needs a spawn reference.
    let Some(reference) = survey.grounded_origin.or(survey.spawn_eye) else {
        return (Vec::new(), None);
    };

    let cos_cone = opts.visibility_cone_degrees.to_radians().cos();
    let safe = |pos: &[f32; 3]| -> bool {
        let mut nearest = f32::INFINITY;
        for (eye, fwd) in &survey.window_cameras {
            let v = [pos[0] - eye[0], pos[1] - eye[1], pos[2] - eye[2]];
            let dist = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            nearest = nearest.min(dist);
            if nearest < opts.min_camera_clearance {
                return false;
            }
            if dist < 1.0 || dist > opts.visibility_max_distance {
                continue;
            }
            let fl = (fwd[0] * fwd[0] + fwd[1] * fwd[1] + fwd[2] * fwd[2]).sqrt();
            if fl < 0.5 {
                continue;
            }
            if (v[0] * fwd[0] + v[1] * fwd[1] + v[2] * fwd[2]) / (dist * fl) >= cos_cone {
                return false;
            }
        }
        true
    };

    // Furthest approach any in-window camera makes to a position. Ranking by
    // this — rather than by nearness to spawn — matches what these spots are
    // actually for. Spawn proximity was a holdover from "hide it near spawn";
    // a flush spot has no reason to be anywhere in particular except far from
    // the lens.
    let clearance = |pos: &[f32; 3]| -> f32 {
        survey
            .window_cameras
            .iter()
            .map(|(eye, _)| distance(pos, eye))
            .fold(f32::INFINITY, f32::min)
    };

    // Harvested decal positions first: they sit on walls the demo has already
    // drawn decals on. Floor points under the player's own path come last —
    // they are plentiful and their surfaces are proven, but they are by
    // construction where the player walks, which is the worst place to hide
    // something. Used only to make up a shortfall in count.
    let mut pool: Vec<[f32; 3]> = Vec::new();
    let mut source = None;
    for (candidates, src) in [
        (&survey.harvested, FlushSource::HarvestedNearSpawn),
        (&survey.floor_candidates, FlushSource::PlayerFloorPath),
    ] {
        let mut ok: Vec<[f32; 3]> = candidates.iter().copied().filter(|p| safe(p)).collect();
        // Furthest from the camera first.
        ok.sort_by(|a, b| {
            clearance(b)
                .partial_cmp(&clearance(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for p in ok {
            if pool.len() >= wanted {
                break;
            }
            // Enforce spacing across the whole pool, not just within a source,
            // so two sources cannot contribute overlapping spots.
            if pool.iter().all(|q| distance(&p, q) >= MIN_POSITION_SPACING) {
                pool.push(p);
                source.get_or_insert(src);
            }
        }
        if pool.len() >= wanted {
            break;
        }
    }

    if !pool.is_empty() {
        return (pool, source);
    }

    legacy_single_position(survey, opts, reference)
}

/// Original single-position selection, retained as the last resort for demos
/// that yield no safe spread at all.
fn legacy_single_position(
    survey: &Survey,
    opts: &DecalCleanOptions,
    reference: [f32; 3],
) -> (Vec<[f32; 3]>, Option<FlushSource>) {

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
            return (vec![clear[0].0], Some(FlushSource::HarvestedNearSpawn));
        }

        // Nothing was fully clear. Prefer the least-seen candidate, breaking
        // ties on distance; the caller reports both so a marginal pick is
        // visible rather than silent.
        scored.sort_by(|a, b| {
            a.2.cmp(&b.2)
                .then(b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
        });
        return (vec![scored[0].0], Some(FlushSource::HarvestedNearSpawn));
    }

    (
        vec![[reference[0], reference[1], reference[2] - opts.floor_drop]],
        Some(FlushSource::ComputedSpawnFloor),
    )
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
    let texture_index = opts.flush_texture_index.or(survey.texture_index);
    stats.flush_texture_index = texture_index;

    // Distinct spots needed so every injected decal allocates a fresh ring slot
    // instead of being recycled as an overlap of one already placed.
    let burst_count = opts.ring_limit as usize + opts.burst_margin;
    let positions_wanted = burst_count.div_ceil(DECALS_PER_POSITION);

    let (flush_positions, flush_source) = resolve_flush_positions(&survey, opts, positions_wanted);
    stats.flush_coord = flush_positions.first().copied();
    stats.flush_source = flush_source;
    stats.flush_positions = flush_positions.len();
    stats.flush_positions_wanted = positions_wanted;

    if let (Some(pos), Some(reference)) = (flush_positions.first(), survey.grounded_origin) {
        stats.spawn_to_flush_distance = Some(distance(pos, &reference));
    }

    if !flush_positions.is_empty() {
        stats.min_camera_distance = survey
            .window_cameras
            .iter()
            .flat_map(|(eye, _)| flush_positions.iter().map(move |p| distance(p, eye)))
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Sampled in-window frames where ANY flush position falls inside the
        // camera's cone. Non-zero means part of the spread is on screen during
        // a recorded clip — the failure this heuristic exists to avoid.
        let cos_cone = opts.visibility_cone_degrees.to_radians().cos();
        stats.camera_samples = survey.window_cameras.len();
        stats.flush_on_camera_frames = survey
            .window_cameras
            .iter()
            .filter(|(eye, fwd)| {
                flush_positions.iter().any(|pos| {
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
                    // Only messages that PLACE a decal are stripped.
                    //
                    // SvcDecalName (36) deliberately is not: it registers a
                    // decal name against an index (how a custom player spray is
                    // announced) and places nothing. Blanking it does not
                    // remove a decal — it destroys the texture lookup that
                    // decals referencing that index still need, including any
                    // index this pass harvests for its own flush burst.
                    let strip_type = match eng.as_ref() {
                        EngineMessage::SvcTempEntity(te)
                            if WALL_DECAL_ENTITY_TYPES.contains(&te.entity_type) =>
                        {
                            te.entity_type
                        }
                        _ => continue,
                    };
                    if strip_type == TE_PLAYERDECAL {
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
        if let (false, Some(texture_index)) = (flush_positions.is_empty(), texture_index) {
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

            // Walk the position list so no spot receives more than
            // DECALS_PER_POSITION consecutive decals. Exceeding
            // MAX_OVERLAP_DECALS at one spot makes the engine recycle instead
            // of allocate, which stops the ring advancing and voids the sweep.
            let mut placed_here = 0usize;
            let mut pos_idx = 0usize;

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
                    let pos = flush_positions[pos_idx % flush_positions.len()];
                    messages.push(build_world_decal(&pos, texture_index));
                    stats.flush_decals_injected += 1;
                    placed_here += 1;
                    if placed_here >= DECALS_PER_POSITION {
                        placed_here = 0;
                        pos_idx += 1;
                    }
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
