// patch/decal_probe.rs
// Measures the one number the flush design is currently guessing at: how far a
// decal message's position may sit from a solid surface and still create a
// decal.
//
// ── Why it matters ───────────────────────────────────────────────────────────
// The ring sweep in `decal_strip.rs` can only place flush decals at positions
// the demo hands it — real decal positions it harvests, or floor points under
// the player's own path. On a 20-minute match demo that yields ~30 usable
// spots against the 68 a 256-slot ring needs, and relaxing the camera filters
// does not help: line of sight is the binding constraint, not proximity.
//
// If positions could instead be SYNTHESISED near one known-good surface point,
// supply stops being a constraint entirely and every spot can be chosen far
// from the lens. Whether that works hinges on the engine's tolerance:
//
//     R_DecalShoot walks the BSP from the world root:
//         dist = PlaneDiff(m_Position, node->plane)
//         if      dist >  m_Size  -> descend front
//         else if dist < -m_Size  -> descend back
//         else                    -> apply to surfaces on this node
//
// Nothing there consults the camera, the PVS or the viewer — decal creation
// happens at message-parse time, so a flush decal never needs to be *rendered*
// to consume a ring slot. The only constraint is `m_Size`: the position must
// land within roughly a decal's own radius of solid geometry. That radius has
// been estimated at 8-16 units from texture dimensions and never measured.
//
// ── How this measures it ─────────────────────────────────────────────────────
// Strip every decal out of the demo, then stamp a three-row grid onto one patch
// of surface the demo proves exists, and pin r_decals high enough that nothing
// can evict anything:
//
//     OUT row   offsets pushed away from the surface, into the room
//     CTL row   dead on the surface — the control
//     IN  row   the same offsets pushed into the solid
//
// Column j of every row uses offset `offsets[j]`, ascending, so a row simply
// runs out of holes at the point the engine stopped accepting the position.
// The readout is three counts. The control row proves each column has surface
// behind it; if it comes up short, the grid overran an edge and the run is void
// rather than silently misread.
//
// The measurement is symmetric on purpose. `PlaneDiff` compares |dist| against
// m_Size in both directions, so a position buried in solid should fail exactly
// as a position floating in air does — and an inward offset is the more useful
// half for production, since a synthesised spot nudged INTO a wall cannot stick
// out of it visually.

use dem::open_demo_from_bytes;
use dem::types::{
    ByteString, ConsoleCommand, EngineMessage, Frame, FrameData, MessageData, NetMessage,
};

use super::decal_strip::{
    build_world_decal, distance, frame_ordinals, strip_decal_messages, survey, DecalCleanOptions,
};

/// Offsets tested, in world units, ascending. Ascending order is what makes a
/// row's hole count readable as a threshold rather than a pattern to decode.
const DEFAULT_OFFSETS: &[f32] = &[2.0, 4.0, 8.0, 12.0, 16.0, 24.0, 32.0, 48.0];

/// Gap between adjacent columns, along the surface. Wide enough that two holes
/// never read as one, and comfortably past the engine's overlap radius so no
/// probe can be lost to recycling instead of to the offset under test.
const DEFAULT_COLUMN_SPACING: f32 = 40.0;

/// Floor on column pitch when a patch is too narrow to seat every column at the
/// requested spacing. Matches the flush burst's own overlap floor.
const MIN_COLUMN_SPACING: f32 = 28.0;

/// Gap between the three rows, across the surface.
const DEFAULT_ROW_GAP: f32 = 40.0;

/// Cap on synthetic decals added to any single network packet, matching the
/// flush burst's own limit.
const MAX_PER_FRAME: usize = 4;

#[derive(Debug, Clone)]
pub struct ProbeOptions {
    /// Offsets from the surface to test, one column each. Must be ascending.
    pub offsets: Vec<f32>,
    pub column_spacing: f32,
    pub row_gap: f32,
    /// Decal texture index, overriding the one harvested from the demo. A
    /// small bullet hole reads far better in a grid than a grenade scorch.
    pub texture_index: Option<u8>,
    /// Force the surface's normal axis (0=x, 1=y, 2=z) instead of detecting it.
    pub axis: Option<usize>,
    /// Hand-picked anchor point on a known surface, overriding detection
    /// entirely. Requires `axis` so the offset direction is defined.
    pub anchor: Option<[f32; 3]>,
    /// How far apart two decals may sit along the normal and still count as
    /// coplanar. Real decals land on the surface itself, so this only absorbs
    /// coordinate quantisation (1/8 unit) and slight surface relief.
    pub plane_tolerance: f32,
    /// How close two coplanar decals must be to count as being on the same
    /// patch of surface.
    pub link_radius: f32,
    /// Decals that must share a patch before it is worth measuring on.
    pub min_plane_decals: usize,
    /// Extent a patch must span to be worth measuring on, rather than a tight
    /// cluster on some small prop.
    pub min_plane_spread: f32,
    /// Pinned r_decals. Wants to be large: the probe grid must survive every
    /// client-side decal the POV player's own gunfire creates, and those are
    /// predicted locally so stripping cannot touch them.
    pub ring_limit: u32,
    /// Blank every decal message in the demo, so the grid is the only thing on
    /// the surface that arrived over the wire.
    pub strip_all: bool,
    /// Frame ordinals of lead time between the grid being stamped and the
    /// first moment the camera looks at it.
    pub lead_ticks: i32,
    /// Half-angle of the cone counted as "looking at it" when hunting for a
    /// timestamp to hand the user.
    pub sighting_cone_degrees: f32,
    /// Range past which the grid is too small on screen to be counted.
    pub sighting_max_distance: f32,
}

impl Default for ProbeOptions {
    fn default() -> Self {
        Self {
            offsets: DEFAULT_OFFSETS.to_vec(),
            column_spacing: DEFAULT_COLUMN_SPACING,
            row_gap: DEFAULT_ROW_GAP,
            texture_index: None,
            axis: None,
            anchor: None,
            plane_tolerance: 2.0,
            link_radius: 160.0,
            min_plane_decals: 6,
            min_plane_spread: 96.0,
            ring_limit: 4096,
            strip_all: true,
            lead_ticks: 400,
            sighting_cone_degrees: 25.0,
            sighting_max_distance: 900.0,
        }
    }
}

/// One injected probe, kept so the caller can print a map of what to expect on
/// the surface next to what it means.
#[derive(Debug, Clone, Copy)]
pub struct Probe {
    pub row: ProbeRow,
    pub column: usize,
    pub offset: f32,
    pub position: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeRow {
    /// Pushed away from the surface, into open space.
    Out,
    /// Dead on the surface. Every one of these must appear.
    Control,
    /// Pushed into the solid behind the surface.
    In,
}

impl ProbeRow {
    pub fn label(self) -> &'static str {
        match self {
            ProbeRow::Out => "OUT",
            ProbeRow::Control => "CTL",
            ProbeRow::In => "IN ",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProbeStats {
    pub harvested_decals: usize,
    /// Normal axis of the chosen surface: 0=x, 1=y, 2=z.
    pub axis: usize,
    /// Coordinate of the surface along that axis.
    pub plane_value: f32,
    /// Decals proving that patch of surface exists.
    pub plane_members: usize,
    /// Extent those decals span along the patch's two in-plane axes.
    pub plane_spread: (f32, f32),
    /// +1 or -1: which way along the normal axis open space lies.
    pub outward: f32,
    /// Centre of the grid.
    pub anchor: [f32; 3],
    /// In-plane axis the columns run along, and the one the rows step across.
    pub column_axis: usize,
    pub row_axis: usize,
    /// Pitch the columns ended up at, after any relaxation needed to fit them
    /// into a stretch of surface the demo can vouch for.
    pub column_pitch: f32,
    /// Distance from each column to the nearest real decal on the same patch.
    pub column_evidence: Vec<f32>,
    /// Columns with a real decal within half a pitch of them. Anything less
    /// than every column means part of the grid is placed by arithmetic.
    pub columns_backed: usize,
    pub texture_index: u8,
    pub probes: Vec<Probe>,
    pub decals_stripped: usize,
    pub sprays_stripped: usize,
    pub injected_at_ordinal: i32,
    /// Viewdemo timestamps (SvcTime seconds) where the camera is pointed at
    /// the grid from close enough to count it.
    pub sightings: Vec<f32>,
    /// Closest the camera ever gets to the grid while looking at it.
    pub best_sighting_distance: Option<f32>,
}

/// A camera sample with enough context to both aim the grid and time it.
struct CameraSample {
    ordinal: i32,
    eye: [f32; 3],
    forward: [f32; 3],
    svc_time: f32,
}

fn norm(v: &[f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// Distance from `cam` to `point` if the camera is pointed at it from close
/// enough to count holes, `None` otherwise.
fn look_distance(cam: &CameraSample, point: &[f32; 3], opts: &ProbeOptions) -> Option<f32> {
    let v = [
        point[0] - cam.eye[0],
        point[1] - cam.eye[1],
        point[2] - cam.eye[2],
    ];
    let d = norm(&v);
    if d < 24.0 || d > opts.sighting_max_distance {
        return None;
    }
    let fl = norm(&cam.forward);
    if fl < 0.5 {
        return None;
    }
    let dot = (v[0] * cam.forward[0] + v[1] * cam.forward[1] + v[2] * cam.forward[2]) / (d * fl);
    if dot >= opts.sighting_cone_degrees.to_radians().cos() {
        Some(d)
    } else {
        None
    }
}

/// Groups values into runs no wider than `tolerance`, returning each run's mean
/// and its members' indices.
///
/// A sweep rather than bucket-rounding: rounding puts two values a hair apart
/// into different buckets whenever they straddle a boundary, which would split
/// one surface into two undersized patches and lose it to `min_plane_decals`.
fn cluster(values: &[f32], tolerance: f32) -> Vec<(f32, Vec<usize>)> {
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|&a, &b| {
        values[a]
            .partial_cmp(&values[b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut out: Vec<(f32, Vec<usize>)> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut anchor = f32::NAN;

    for idx in order {
        let v = values[idx];
        if current.is_empty() {
            anchor = v;
            current.push(idx);
        } else if (v - anchor).abs() <= tolerance {
            current.push(idx);
        } else {
            let mean = current.iter().map(|&i| values[i]).sum::<f32>() / current.len() as f32;
            out.push((mean, std::mem::take(&mut current)));
            anchor = v;
            current.push(idx);
        }
    }
    if !current.is_empty() {
        let mean = current.iter().map(|&i| values[i]).sum::<f32>() / current.len() as f32;
        out.push((mean, current));
    }
    out
}

/// The patch of surface the grid gets stamped onto, and the layout chosen for
/// it.
struct Target {
    /// Normal axis: 0=x, 1=y, 2=z.
    axis: usize,
    /// Coordinate of the surface along that axis.
    value: f32,
    /// In-plane axis the columns run along, and the one the rows step across.
    col_axis: usize,
    row_axis: usize,
    /// Column positions along `col_axis`, evenly spaced.
    columns: Vec<f32>,
    /// Distance from each column to the nearest real decal.
    evidence: Vec<f32>,
    /// Columns with a real decal within half a pitch of them.
    backed: usize,
    /// Pitch those columns ended up at.
    pitch: f32,
    /// Middle row's coordinate along `row_axis`.
    row_center: f32,
    /// Decals proving this patch of surface exists.
    members: Vec<[f32; 3]>,
    /// Extent those decals span along the two in-plane axes.
    spread: (f32, f32),
}

/// The two axes that lie in a plane whose normal runs along `axis`.
fn tangent_axes(axis: usize) -> (usize, usize) {
    match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    }
}

fn extent(members: &[[f32; 3]], ax: usize) -> f32 {
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for m in members {
        lo = lo.min(m[ax]);
        hi = hi.max(m[ax]);
    }
    if lo.is_finite() { hi - lo } else { 0.0 }
}

/// Splits a coplanar set into spatially connected patches.
///
/// Coplanar is not contiguous. Every floor in a map that happens to sit at the
/// same height lands in one Z cluster — the first run of this picked exactly
/// that: a "plane" whose decals spanned 2173 x 5273 units across the whole map.
/// A grid centred anywhere in it would have had columns hanging in mid-air over
/// a different room. Linking members that sit within `radius` of each other is
/// what makes "there is surface between these two decals" a defensible claim.
fn connected_patches(members: &[[f32; 3]], radius: f32) -> Vec<Vec<[f32; 3]>> {
    let n = members.len();
    let mut seen = vec![false; n];
    let mut out = Vec::new();

    for start in 0..n {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![start];
        let mut patch = Vec::new();
        while let Some(i) = stack.pop() {
            patch.push(members[i]);
            for j in 0..n {
                if !seen[j] && distance(&members[i], &members[j]) <= radius {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        out.push(patch);
    }
    out
}

/// Centre of the `+/- half` window containing the most values.
fn densest_band(values: &[f32], half: f32) -> Option<f32> {
    let mut best: Option<(usize, f32)> = None;
    for &centre in values {
        let n = values.iter().filter(|v| (**v - centre).abs() <= half).count();
        if best.map(|(b, _)| n > b).unwrap_or(true) {
            best = Some((n, centre));
        }
    }
    best.map(|(_, c)| c)
}

/// One candidate row of columns, and how well the demo backs it.
struct Layout {
    columns: Vec<f32>,
    /// Distance from each column to the nearest real decal along the same axis.
    /// A column within half a pitch of one has surface either side of it in the
    /// demo's own evidence; anything further is arithmetic and might be hanging
    /// over a doorway.
    evidence: Vec<f32>,
    backed: usize,
    pitch: f32,
}

/// Lays an evenly spaced row of columns across the stretch of a patch with the
/// most real decals in it.
///
/// Even spacing rather than snapping each column onto a real decal, which was
/// the first attempt: snapping produced columns at Y = -144, -109, -69, 62, 95,
/// ... — a 131-unit hole in the middle of an otherwise regular row. The whole
/// readout is "count the holes", and a row with a visible gap in it invites
/// exactly the miscount the design exists to avoid. So the grid stays regular
/// and each column instead carries how far it sits from the nearest decal, so a
/// stretch the demo cannot vouch for is reported rather than hidden.
///
/// Pitch is relaxed toward `MIN_COLUMN_SPACING` only when the wider grid cannot
/// be fully backed: a tighter grid fits inside a denser stretch of wall.
fn lay_columns(band: &[[f32; 3]], col_axis: usize, wanted: usize, spacing: f32) -> Option<Layout> {
    let mut coords: Vec<f32> = band.iter().map(|m| m[col_axis]).collect();
    coords.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let (lo, hi) = (*coords.first()?, *coords.last()?);

    let mut best: Option<(usize, f32, Layout)> = None;
    let mut pitch = spacing;

    while pitch >= MIN_COLUMN_SPACING {
        let span = pitch * (wanted as f32 - 1.0);
        // Every real decal is tried as the row's left end, plus the placement
        // that centres the row on the patch. That is enough candidates to find
        // the densest stretch without sweeping continuously.
        let mut starts: Vec<f32> = coords.clone();
        starts.push((lo + hi - span) / 2.0);

        for start in starts {
            let columns: Vec<f32> = (0..wanted).map(|j| start + j as f32 * pitch).collect();
            let evidence: Vec<f32> = columns
                .iter()
                .map(|c| {
                    coords
                        .iter()
                        .map(|d| (d - c).abs())
                        .fold(f32::INFINITY, f32::min)
                })
                .collect();
            let backed = evidence.iter().filter(|e| **e <= pitch / 2.0).count();
            let slack: f32 = evidence.iter().map(|e| e.min(pitch)).sum();

            let better = best
                .as_ref()
                .map(|(b, s, _)| backed > *b || (backed == *b && slack < *s))
                .unwrap_or(true);
            if better {
                best = Some((
                    backed,
                    slack,
                    Layout {
                        columns,
                        evidence,
                        backed,
                        pitch,
                    },
                ));
            }
        }

        if best.as_ref().map(|(b, _, _)| *b == wanted).unwrap_or(false) {
            break;
        }
        pitch -= 4.0;
    }

    best.map(|(_, _, l)| l)
}

/// Picks the patch of surface to measure on.
///
/// Only harvested decal positions are eligible. They are the one input that
/// involves no derivation at all — the engine created a decal there, so the
/// surface is proven to the exact coordinate. Floor points under the player's
/// path would be plentiful and their surfaces real, but they are computed by
/// dropping a fixed offset below the player's origin, and a wrong offset would
/// bias every measurement here by that amount. A measuring instrument does not
/// get to guess at its own zero.
///
/// Among the patches that qualify, the one the camera spends the most time
/// looking at wins. A grid nobody can find measures nothing.
fn choose_target(
    harvested: &[[f32; 3]],
    cameras: &[CameraSample],
    opts: &ProbeOptions,
) -> Option<Target> {
    let wanted = opts.offsets.len();
    let mut best: Option<(f32, Target)> = None;

    for axis in 0..3 {
        if let Some(forced) = opts.axis {
            if axis != forced {
                continue;
            }
        }
        let (t1, t2) = tangent_axes(axis);
        let values: Vec<f32> = harvested.iter().map(|p| p[axis]).collect();

        for (value, idxs) in cluster(&values, opts.plane_tolerance) {
            if idxs.len() < opts.min_plane_decals {
                continue;
            }
            let coplanar: Vec<[f32; 3]> = idxs.iter().map(|&i| harvested[i]).collect();

            for patch in connected_patches(&coplanar, opts.link_radius) {
                if patch.len() < opts.min_plane_decals {
                    continue;
                }
                let spread = (extent(&patch, t1), extent(&patch, t2));
                if spread.0.max(spread.1) < opts.min_plane_spread {
                    continue;
                }

                // Columns run along whichever in-plane axis the patch is
                // widest in; for an upright wall that is the horizontal one,
                // which puts the rows above one another where they read as
                // rows.
                let (col_axis, row_axis) = if spread.0 >= spread.1 {
                    (t1, t2)
                } else {
                    (t2, t1)
                };

                // Band the rows will occupy: the stretch of `row_axis` holding
                // the most decals, so there is evidence of surface above and
                // below the middle row as well as along it.
                let row_values: Vec<f32> = patch.iter().map(|m| m[row_axis]).collect();
                let band_half = opts.row_gap * 1.5;
                let Some(row_center) = densest_band(&row_values, band_half) else {
                    continue;
                };
                let band: Vec<[f32; 3]> = patch
                    .iter()
                    .copied()
                    .filter(|m| (m[row_axis] - row_center).abs() <= band_half)
                    .collect();

                let Some(layout) = lay_columns(&band, col_axis, wanted, opts.column_spacing)
                else {
                    continue;
                };

                let centroid = {
                    let n = patch.len() as f32;
                    let mut c = [0.0f32; 3];
                    for m in &patch {
                        for k in 0..3 {
                            c[k] += m[k] / n;
                        }
                    }
                    c
                };
                let sightings = cameras
                    .iter()
                    .filter(|cam| look_distance(cam, &centroid, opts).is_some())
                    .count();

                // Being visible at all comes first — a grid nobody can find
                // measures nothing. Then how much of the row the demo can vouch
                // for, then whether the patch is tall enough to hold all three
                // rows inside the stretch its decals evidence, then dwell time,
                // then patch size. An upright wall is preferred over a floor of
                // equal standing: three rows stacked up a wall sit in front of
                // the player, where a grid on the ground has to be looked down
                // at and foreshortens.
                let tall_enough = extent(&patch, row_axis) >= opts.row_gap * 2.0;
                let orientation = if axis == 2 { 0.85 } else { 1.0 };
                let score = (usize::from(sightings > 0) as f32 * 1.0e7
                    + layout.backed as f32 * 1.0e5
                    + usize::from(tall_enough) as f32 * 1.0e4
                    + sightings.min(500) as f32 * 10.0
                    + patch.len() as f32)
                    * orientation;

                if best.as_ref().map(|(b, _)| score > *b).unwrap_or(true) {
                    best = Some((
                        score,
                        Target {
                            axis,
                            value,
                            col_axis,
                            row_axis,
                            columns: layout.columns,
                            evidence: layout.evidence,
                            backed: layout.backed,
                            pitch: layout.pitch,
                            row_center,
                            members: patch,
                            spread,
                        },
                    ));
                }
            }
        }
    }

    best.map(|(_, t)| t)
}

/// Which side of the surface the room is on.
///
/// Taken from where the camera actually was: an eye position is by definition
/// in open space, so the sign of its offset from the plane is the outward
/// direction. Cameras near the patch are weighted first, since a wall far
/// across the map may well have the player on the other side of it.
fn outward_sign(target: &Target, anchor: &[f32; 3], cameras: &[CameraSample]) -> f32 {
    for radius in [800.0f32, 2000.0, f32::INFINITY] {
        let mut sum = 0.0f32;
        let mut n = 0usize;
        for cam in cameras {
            if distance(&cam.eye, anchor) > radius {
                continue;
            }
            sum += cam.eye[target.axis] - target.value;
            n += 1;
        }
        if n > 0 && sum.abs() > 1.0 {
            return if sum > 0.0 { 1.0 } else { -1.0 };
        }
    }
    1.0
}

/// Camera samples with ordinals and the viewdemo clock attached.
///
/// `SvcTime` is tracked rather than `Frame::time`: the former is what the
/// viewdemo window displays, and the two diverge by minutes on a demo whose
/// recording started before the player connected. A timestamp the user cannot
/// scrub to is worthless.
fn collect_cameras(demo: &dem::types::Demo) -> Vec<CameraSample> {
    let mut out = Vec::new();
    let mut ordinal = 0i32;
    let mut svc_time = 0.0f32;
    let mut stride = 0usize;

    for entry in &demo.directory.entries {
        for frame in &entry.frames {
            ordinal += 1;
            let FrameData::NetworkMessage(net_msg_box) = &frame.frame_data else {
                continue;
            };

            if let MessageData::Parsed(messages) = &net_msg_box.1.messages {
                for msg in messages {
                    if let NetMessage::EngineMessage(eng) = msg {
                        if let EngineMessage::SvcTime(t) = eng.as_ref() {
                            svc_time = t.time;
                        }
                    }
                }
            }

            let rp = &net_msg_box.1.info.refparams;
            let (origin, fwd) = (&rp.view_origin, &rp.forward);
            if origin.len() < 3 || fwd.len() < 3 {
                continue;
            }
            let eye = [origin[0], origin[1], origin[2]];
            if eye == [0.0, 0.0, 0.0] {
                continue;
            }
            stride += 1;
            if stride % 4 != 0 {
                continue;
            }
            out.push(CameraSample {
                ordinal,
                eye,
                forward: [fwd[0], fwd[1], fwd[2]],
                svc_time,
            });
        }
    }
    out
}

/// Frame ordinal of the last DemoStart, i.e. the last level load.
///
/// Nothing may be injected before it. `R_ClearDecals()` runs on level load and
/// memsets the whole pool, so a grid stamped ahead of that point is wiped
/// before anyone can look at it — the exact mechanism that makes walls clean
/// when a demo is first loaded.
fn last_level_load_ordinal(demo: &dem::types::Demo) -> i32 {
    let mut ordinal = 0i32;
    let mut last = 0i32;
    for entry in &demo.directory.entries {
        for frame in &entry.frames {
            ordinal += 1;
            if matches!(frame.frame_data, FrameData::DemoStart) {
                last = ordinal;
            }
        }
    }
    last
}

/// Builds the three-row grid.
fn build_probes(target: &Target, outward: f32, opts: &ProbeOptions) -> Vec<Probe> {
    let mut out = Vec::new();

    for (row, row_step) in [
        (ProbeRow::Out, 1.0f32),
        (ProbeRow::Control, 0.0),
        (ProbeRow::In, -1.0),
    ] {
        for (column, (&offset, &col_coord)) in
            opts.offsets.iter().zip(target.columns.iter()).enumerate()
        {
            let mut pos = [0.0f32; 3];
            pos[target.axis] = target.value
                + match row {
                    ProbeRow::Out => outward * offset,
                    ProbeRow::Control => 0.0,
                    ProbeRow::In => -outward * offset,
                };
            pos[target.col_axis] = col_coord;
            pos[target.row_axis] = target.row_center + row_step * opts.row_gap;

            out.push(Probe {
                row,
                column,
                offset,
                position: pos,
            });
        }
    }
    out
}

/// Strips the demo, stamps a measurement grid onto one proven surface, and pins
/// r_decals high enough that nothing can evict it.
pub fn probe_decal_offsets(
    demo_bytes: &[u8],
    opts: &ProbeOptions,
) -> Result<(Vec<u8>, ProbeStats), String> {
    if opts.offsets.is_empty() {
        return Err("no offsets to probe".into());
    }
    if opts.offsets.windows(2).any(|w| w[1] <= w[0]) {
        return Err("offsets must be strictly ascending — a row's hole count is \
                    only readable as a threshold if they are"
            .into());
    }
    if opts.offsets[0] < 0.0 {
        return Err("offsets are magnitudes; the OUT and IN rows apply the sign".into());
    }

    // Every row gets a zero-offset column of its own, ahead of the offsets
    // under test.
    //
    // Only the middle row sits in the stretch of surface the patch's decals
    // directly evidence; the OUT and IN rows are a row-gap above and below it,
    // which is arithmetic. If a wall happens to end just past the band that got
    // shot up — a low wall, a windowsill — a whole row lands on nothing and
    // reads as "the engine rejected every offset". A hole that IS on the plane
    // distinguishes those two cases for each row independently: no leading hole
    // means that row's band missed the wall and the row is void, rather than
    // being a threshold of zero.
    let opts = &ProbeOptions {
        offsets: if opts.offsets[0] == 0.0 {
            opts.offsets.clone()
        } else {
            std::iter::once(0.0).chain(opts.offsets.iter().copied()).collect()
        },
        ..opts.clone()
    };

    let mut demo =
        open_demo_from_bytes(demo_bytes).map_err(|e| format!("Could not parse demo file: {}", e))?;

    // One window spanning the whole demo, so the survey harvests decals and a
    // texture index from every frame rather than from capture windows it has
    // no concept of here.
    let whole_demo = [(1i32, i32::MAX)];
    let survey = survey(&demo, &whole_demo, &DecalCleanOptions::default());
    let cameras = collect_cameras(&demo);

    let target = match (opts.anchor, opts.axis) {
        (Some(a), Some(axis)) => {
            let (t1, t2) = tangent_axes(axis);
            let columns: Vec<f32> = (0..opts.offsets.len())
                .map(|j| {
                    a[t1]
                        + (j as f32 - (opts.offsets.len() as f32 - 1.0) / 2.0)
                            * opts.column_spacing
                })
                .collect();
            Target {
                axis,
                value: a[axis],
                col_axis: t1,
                row_axis: t2,
                evidence: vec![0.0; opts.offsets.len()],
                backed: opts.offsets.len(),
                columns,
                pitch: opts.column_spacing,
                row_center: a[t2],
                members: vec![a],
                spread: (0.0, 0.0),
            }
        }
        (Some(_), None) => {
            return Err("--anchor needs --axis: without a normal there is no \
                        direction to offset along"
                .into())
        }
        (None, _) => choose_target(&survey.harvested, &cameras, opts).ok_or_else(|| {
            format!(
                "no usable surface: {} harvested decals, none forming a connected patch \
                 of {}+ decals coplanar within {:.1} units, spanning {:.0}+ units, and \
                 wide enough to seat {} columns. Try a demo with more decal traffic, \
                 lower --min-plane-decals, a smaller --column-spacing, or pass \
                 --anchor x,y,z with --axis.",
                survey.harvested.len(),
                opts.min_plane_decals,
                opts.plane_tolerance,
                opts.min_plane_spread,
                opts.offsets.len()
            )
        })?,
    };

    let anchor = {
        let mut a = [0.0f32; 3];
        a[target.axis] = target.value;
        a[target.col_axis] =
            target.columns.iter().sum::<f32>() / target.columns.len() as f32;
        a[target.row_axis] = target.row_center;
        a
    };

    let outward = outward_sign(&target, &anchor, &cameras);
    let probes = build_probes(&target, outward, opts);

    let texture_index = opts.texture_index.or(survey.texture_index).ok_or_else(|| {
        "no decal texture index available: the demo registered none and none was \
         supplied. Pass --texture-index."
            .to_string()
    })?;

    // ── When the camera is pointed at the grid ───────────────────────────────
    let sighted: Vec<&CameraSample> = cameras
        .iter()
        .filter(|c| look_distance(c, &anchor, opts).is_some())
        .collect();
    let best_sighting_distance = sighted
        .iter()
        .filter_map(|c| look_distance(c, &anchor, opts))
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Distinct viewing moments rather than every sampled frame: consecutive
    // samples during one long look are the same sighting.
    let mut sightings: Vec<f32> = Vec::new();
    let mut last = f32::NEG_INFINITY;
    for cam in &sighted {
        if cam.svc_time - last > 3.0 {
            sightings.push(cam.svc_time);
            last = cam.svc_time;
        }
    }

    // ── Strip, so the grid is the only thing on the surface ──────────────────
    let (decals_stripped, sprays_stripped) = if opts.strip_all {
        strip_decal_messages(&mut demo, &[])
    } else {
        (0, 0)
    };

    // ── Stamp the grid, after the last level load and ahead of the first look ─
    // Not before the last level load, and not before the client is actually in
    // the map. A frame only carries a usable camera once the player is in game,
    // so the first camera sample is a cheap stand-in for "finished connecting"
    // — injecting during the connect handshake would reference a decal texture
    // index the client has not resolved yet.
    let load_ordinal = last_level_load_ordinal(&demo)
        .max(cameras.first().map(|c| c.ordinal).unwrap_or(0));
    let eligible: Vec<(usize, usize, i32)> = frame_ordinals(&demo)
        .into_iter()
        .filter(|&(entry_idx, frame_idx, ordinal)| {
            ordinal > load_ordinal
                && demo.directory.entries[entry_idx]
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

    let needed = probes.len().div_ceil(MAX_PER_FRAME);
    if eligible.len() < needed {
        return Err(format!(
            "only {} parsed network frames after the last level load, need {}",
            eligible.len(),
            needed
        ));
    }

    // Walk back from the first sighting so the grid lands as late as it can
    // while still preceding the look, then fill forwards so column order on the
    // surface matches the order the offsets are reported in.
    let deadline = sighted
        .first()
        .map(|c| c.ordinal - opts.lead_ticks)
        .unwrap_or(i32::MIN);
    let last_slot = eligible
        .iter()
        .rposition(|&(_, _, ord)| ord <= deadline)
        .unwrap_or(needed - 1)
        .max(needed - 1);
    let first_slot = last_slot + 1 - needed;
    let injected_at_ordinal = eligible[first_slot].2;

    let mut probe_iter = probes.iter();
    let mut placed = 0usize;
    for slot in first_slot..eligible.len() {
        if placed >= probes.len() {
            break;
        }
        let (entry_idx, frame_idx, _) = eligible[slot];
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
        for _ in 0..MAX_PER_FRAME {
            let Some(probe) = probe_iter.next() else {
                break;
            };
            messages.push(build_world_decal(&probe.position, texture_index));
            placed += 1;
        }
    }

    if placed < probes.len() {
        return Err(format!(
            "only {} of {} probes could be placed — too few eligible frames",
            placed,
            probes.len()
        ));
    }

    // ── Pin r_decals well above the grid size ────────────────────────────────
    // The POV player's own gunfire is predicted client-side and never reaches
    // the wire, so stripping cannot remove it. A generous ring keeps those from
    // rotating the grid off the surface before it can be counted.
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
        let anchor_frame = entry
            .frames
            .get(insert_at)
            .or_else(|| entry.frames.first())
            .map(|f| (f.time, f.frame))
            .unwrap_or((0.0, 0));
        let cmd = format!("r_decals {}", opts.ring_limit);
        entry.frames.insert(
            insert_at,
            Frame {
                time: anchor_frame.0,
                frame: anchor_frame.1,
                frame_data: FrameData::ConsoleCommand(ConsoleCommand {
                    command: ByteString::from(cmd.as_str()),
                }),
            },
        );
        entry.frame_count = entry.frames.len() as i32;
    }

    let stats = ProbeStats {
        harvested_decals: survey.harvested.len(),
        axis: target.axis,
        plane_value: target.value,
        plane_members: target.members.len(),
        plane_spread: target.spread,
        outward,
        anchor,
        column_axis: target.col_axis,
        row_axis: target.row_axis,
        column_pitch: target.pitch,
        column_evidence: target.evidence.clone(),
        columns_backed: target.backed,
        texture_index,
        probes,
        decals_stripped,
        sprays_stripped,
        injected_at_ordinal,
        sightings,
        best_sighting_distance,
    };

    Ok((demo.write_to_bytes(), stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts_with(offsets: &[f32]) -> ProbeOptions {
        ProbeOptions {
            offsets: offsets.to_vec(),
            ..Default::default()
        }
    }

    #[test]
    fn cluster_splits_only_on_gaps_wider_than_tolerance() {
        // Two surfaces 40 units apart, each with slight coordinate jitter.
        let values = [100.0, 100.5, 99.8, 140.0, 140.2];
        let groups = cluster(&values, 2.0);
        assert_eq!(groups.len(), 2);
        let sizes: Vec<usize> = groups.iter().map(|(_, m)| m.len()).collect();
        assert!(sizes.contains(&3) && sizes.contains(&2));
    }

    #[test]
    fn connected_patches_separates_coplanar_but_distant_surfaces() {
        // The failure this exists to prevent: two floors at the same height in
        // different rooms read as one plane, and a grid centred between them
        // would hang in mid-air over neither.
        let members = [
            [0.0, 0.0, -384.0],
            [40.0, 0.0, -384.0],
            [80.0, 0.0, -384.0],
            [2000.0, 0.0, -384.0],
            [2040.0, 0.0, -384.0],
        ];
        let patches = connected_patches(&members, 160.0);
        assert_eq!(patches.len(), 2);
        assert_eq!(patches.iter().map(|p| p.len()).sum::<usize>(), 5);
    }

    #[test]
    fn connected_patches_keeps_a_chain_together() {
        // Links are transitive: end-to-end distance may exceed the radius as
        // long as each hop does not, which is what lets one long wall stay one
        // patch.
        let members: Vec<[f32; 3]> = (0..6).map(|i| [i as f32 * 100.0, 0.0, 0.0]).collect();
        let patches = connected_patches(&members, 160.0);
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].len(), 6);
    }

    #[test]
    fn densest_band_centres_on_the_busiest_window() {
        let values = [0.0, 200.0, 205.0, 210.0, 215.0, 500.0];
        let centre = densest_band(&values, 20.0).unwrap();
        assert!((200.0..=215.0).contains(&centre), "got {}", centre);
    }

    #[test]
    fn lay_columns_spaces_evenly_and_reports_backing() {
        // Decals every 40 units across 320 units of wall, with a 120-unit hole
        // in the middle where a doorway would be.
        let mut band: Vec<[f32; 3]> = Vec::new();
        for i in 0..5 {
            band.push([i as f32 * 40.0, 0.0, 0.0]);
        }
        for i in 0..4 {
            band.push([320.0 + i as f32 * 40.0, 0.0, 0.0]);
        }

        let layout = lay_columns(&band, 0, 5, 40.0).expect("a layout exists");
        assert_eq!(layout.columns.len(), 5);
        assert_eq!(layout.evidence.len(), 5);

        // Evenly spaced, whatever pitch it settled on.
        for w in layout.columns.windows(2) {
            assert!(
                (w[1] - w[0] - layout.pitch).abs() < 0.01,
                "columns {:?} are not evenly spaced at pitch {}",
                layout.columns,
                layout.pitch
            );
        }
        // The left run alone seats all five columns, so it should find full
        // backing rather than straddling the gap.
        assert_eq!(layout.backed, 5);
        assert!(layout.evidence.iter().all(|e| *e <= layout.pitch / 2.0));
    }

    #[test]
    fn lay_columns_may_interleave_between_decals_and_still_count_as_backed() {
        // Four decals over 120 units, five columns wanted. The row does not
        // have to land on them: sliding half a pitch puts every column between
        // two decals, which evidences surface either side of it just as well.
        let band: Vec<[f32; 3]> = (0..4).map(|i| [i as f32 * 40.0, 0.0, 0.0]).collect();
        let layout = lay_columns(&band, 0, 5, 40.0).expect("a layout exists");
        assert_eq!(layout.backed, 5);
        assert!(layout.evidence.iter().all(|e| *e <= layout.pitch / 2.0));
        assert!(layout.pitch >= MIN_COLUMN_SPACING);
    }

    #[test]
    fn lay_columns_reports_a_column_it_cannot_back() {
        // Two decals 40 units apart cannot back five columns at any pitch down
        // to the floor, so the shortfall must be visible rather than silent.
        let band = [[0.0, 0.0, 0.0], [40.0, 0.0, 0.0]];
        let layout = lay_columns(&band, 0, 5, 40.0).expect("a layout exists");
        assert!(
            layout.backed < 5,
            "expected a shortfall, got {} at pitch {}",
            layout.backed,
            layout.pitch
        );
    }

    fn test_target() -> Target {
        Target {
            axis: 1,
            value: 448.0,
            col_axis: 0,
            row_axis: 2,
            columns: vec![100.0, 140.0, 180.0],
            evidence: vec![0.0; 3],
            backed: 3,
            pitch: 40.0,
            row_center: -200.0,
            members: vec![[100.0, 448.0, -200.0]],
            spread: (80.0, 0.0),
        }
    }

    #[test]
    fn build_probes_puts_the_rows_on_opposite_sides_of_the_surface() {
        let opts = opts_with(&[0.0, 8.0, 32.0]);
        let target = test_target();
        let probes = build_probes(&target, -1.0, &opts);
        assert_eq!(probes.len(), 9);

        for p in &probes {
            match p.row {
                // Outward is -Y here, so OUT must sit below the plane value
                // and IN above it — the sign is taken from where the camera
                // was, not assumed.
                ProbeRow::Out => assert!(p.position[1] <= target.value),
                ProbeRow::In => assert!(p.position[1] >= target.value),
                ProbeRow::Control => assert_eq!(p.position[1], target.value),
            }
            if p.offset == 0.0 {
                assert_eq!(
                    p.position[1], target.value,
                    "every row's zero column must sit on the surface"
                );
            }
        }
    }

    #[test]
    fn build_probes_separates_the_rows_by_a_full_row_gap() {
        let opts = opts_with(&[0.0, 8.0]);
        let target = test_target();
        let probes = build_probes(&target, 1.0, &opts);

        let z_of = |row: ProbeRow| probes.iter().find(|p| p.row == row).unwrap().position[2];
        assert_eq!(z_of(ProbeRow::Control), target.row_center);
        assert!((z_of(ProbeRow::Out) - z_of(ProbeRow::Control) - opts.row_gap).abs() < 0.01);
        assert!((z_of(ProbeRow::Control) - z_of(ProbeRow::In) - opts.row_gap).abs() < 0.01);
    }

    #[test]
    fn descending_offsets_are_rejected() {
        let demo: &[u8] = b"not a demo";
        let opts = opts_with(&[8.0, 4.0]);
        let err = probe_decal_offsets(demo, &opts).unwrap_err();
        assert!(err.contains("ascending"), "got: {}", err);
    }
}
