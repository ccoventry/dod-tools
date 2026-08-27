// patch/bsp.rs
// Minimal read-only parser for GoldSrc BSP version 30.
//
// ── Why the decal flush reads maps at all ────────────────────────────────────
// Every source of flush positions the pipeline had before this one is drawn
// from player behaviour: harvested decals are where somebody shot, floor points
// are where somebody walked. That correlates with where the camera looked,
// which is exactly what the line-of-sight test rejects — so the demos that fail
// generate tens of thousands of candidates and keep none of them. The map's own
// geometry is the one surface source that owes nothing to what happened in the
// match. See `docs/decal_flush_bsp_surfaces.md`.
//
// It answers two questions, and only parses what they need. Lighting,
// clipnodes, marksurfaces and entities are skipped entirely.
//
//   1. WHERE ARE THE SURFACES — planes, vertices, edges, surfedges, faces,
//      texinfo, textures, models. Coordinates that owe nothing to the match.
//   2. WHAT CAN SEE WHAT — nodes, leaves, visibility. The flush's safety claim
//      is that its decals are never on screen, and a cone test with no notion
//      of walls is a poor way to establish that.
//
// ── Trusting these offsets ───────────────────────────────────────────────────
// The struct layouts below are the documented v30 format, not something
// verified against a file by hand. They are not taken on trust either:
// `nearest_face` exists so thousands of coordinates the engine provably
// accepted — every harvested decal in every demo — can be run against the
// parsed geometry. If a lump offset, an edge winding or a coordinate space is
// wrong, that check collapses rather than quietly producing plausible garbage.

/// Lump indices, in header order. Only the ones this module reads are named.
const LUMP_PLANES: usize = 1;
const LUMP_TEXTURES: usize = 2;
const LUMP_VERTICES: usize = 3;
const LUMP_VISIBILITY: usize = 4;
const LUMP_NODES: usize = 5;
const LUMP_TEXINFO: usize = 6;
const LUMP_FACES: usize = 7;
const LUMP_LEAVES: usize = 10;
const LUMP_EDGES: usize = 12;
const LUMP_SURFEDGES: usize = 13;
const LUMP_MODELS: usize = 14;

const LUMP_COUNT: usize = 15;
const HEADER_SIZE: usize = 4 + LUMP_COUNT * 8;

pub const BSP_VERSION: i32 = 30;

/// `TEX_SPECIAL`. Set on sky and other surfaces the renderer treats specially;
/// they hold no decal.
const TEX_SPECIAL: i32 = 1;

#[derive(Debug, Clone, Copy)]
pub struct Plane {
    pub normal: [f32; 3],
    pub dist: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Face {
    pub plane: u16,
    /// Non-zero means the face looks along the plane's *negative* normal.
    pub side: i16,
    pub first_edge: i32,
    pub num_edges: i16,
    pub texinfo: i16,
}

#[derive(Debug, Clone, Copy)]
pub struct TexInfo {
    pub miptex: i32,
    pub flags: i32,
}

/// Model 0 is the world; every other model is a brush entity — a door, a lift,
/// a train — whose faces move with it and whose coordinates are therefore only
/// true while it is where it was.
#[derive(Debug, Clone, Copy)]
pub struct Model {
    pub first_face: i32,
    pub num_faces: i32,
    /// Root of this model's node tree for hull 0, the point hull.
    pub head_node: i32,
    /// Leaves the visibility lump carries rows for. Excludes the solid leaf.
    pub vis_leafs: i32,
}

pub struct Bsp {
    pub planes: Vec<Plane>,
    pub vertices: Vec<[f32; 3]>,
    pub edges: Vec<[u16; 2]>,
    pub surfedges: Vec<i32>,
    pub faces: Vec<Face>,
    pub texinfo: Vec<TexInfo>,
    pub texture_names: Vec<String>,
    pub models: Vec<Model>,
    pub nodes: Vec<Node>,
    pub leaves: Vec<Leaf>,
    /// Raw run-length-encoded visibility lump. Empty when the map was compiled
    /// without vis, which is common for scrim and test maps.
    pub visibility: Vec<u8>,
    /// Root of the world tree, cached from model 0.
    pub head_node: i32,
    /// How many leaves the visibility rows cover.
    pub vis_leaf_count: usize,
    /// Axis-aligned bounds per face, cached so point lookups can reject most
    /// faces without touching their polygons.
    bounds: Vec<([f32; 3], [f32; 3])>,
}

fn rd_i32(b: &[u8], at: usize) -> Result<i32, String> {
    b.get(at..at + 4)
        .map(|s| i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| format!("truncated at byte {}", at))
}

fn rd_u32(b: &[u8], at: usize) -> Result<u32, String> {
    rd_i32(b, at).map(|v| v as u32)
}

fn rd_i16(b: &[u8], at: usize) -> Result<i16, String> {
    b.get(at..at + 2)
        .map(|s| i16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| format!("truncated at byte {}", at))
}

fn rd_u16(b: &[u8], at: usize) -> Result<u16, String> {
    rd_i16(b, at).map(|v| v as u16)
}

fn rd_f32(b: &[u8], at: usize) -> Result<f32, String> {
    rd_u32(b, at).map(f32::from_bits)
}

fn rd_vec3(b: &[u8], at: usize) -> Result<[f32; 3], String> {
    Ok([rd_f32(b, at)?, rd_f32(b, at + 4)?, rd_f32(b, at + 8)?])
}

/// One lump's byte range, validated against the file length so every later read
/// is inside the file.
fn lump(bytes: &[u8], index: usize) -> Result<&[u8], String> {
    let at = 4 + index * 8;
    let offset = rd_i32(bytes, at)? as usize;
    let length = rd_i32(bytes, at + 4)? as usize;
    bytes
        .get(offset..offset + length)
        .ok_or_else(|| format!("lump {} runs past the end of the file", index))
}

/// Reads a lump as a fixed-stride array.
fn entries<T>(
    data: &[u8],
    stride: usize,
    mut parse: impl FnMut(&[u8], usize) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    let n = data.len() / stride;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(parse(data, i * stride)?);
    }
    Ok(out)
}

impl Bsp {
    pub fn parse(bytes: &[u8]) -> Result<Bsp, String> {
        if bytes.len() < HEADER_SIZE {
            return Err("file is shorter than a BSP header".to_string());
        }
        let version = rd_i32(bytes, 0)?;
        if version != BSP_VERSION {
            return Err(format!(
                "BSP version {} is not the GoldSrc version {}",
                version, BSP_VERSION
            ));
        }

        let planes = entries(lump(bytes, LUMP_PLANES)?, 20, |b, at| {
            Ok(Plane {
                normal: rd_vec3(b, at)?,
                dist: rd_f32(b, at + 12)?,
            })
        })?;

        let vertices = entries(lump(bytes, LUMP_VERTICES)?, 12, rd_vec3)?;

        let edges = entries(lump(bytes, LUMP_EDGES)?, 4, |b, at| {
            Ok([rd_u16(b, at)?, rd_u16(b, at + 2)?])
        })?;

        let surfedges = entries(lump(bytes, LUMP_SURFEDGES)?, 4, rd_i32)?;

        let faces = entries(lump(bytes, LUMP_FACES)?, 20, |b, at| {
            Ok(Face {
                plane: rd_u16(b, at)?,
                side: rd_i16(b, at + 2)?,
                first_edge: rd_i32(b, at + 4)?,
                num_edges: rd_i16(b, at + 8)?,
                texinfo: rd_i16(b, at + 10)?,
            })
        })?;

        let texinfo = entries(lump(bytes, LUMP_TEXINFO)?, 40, |b, at| {
            Ok(TexInfo {
                miptex: rd_i32(b, at + 32)?,
                flags: rd_i32(b, at + 36)?,
            })
        })?;

        let models = entries(lump(bytes, LUMP_MODELS)?, 64, |b, at| {
            Ok(Model {
                first_face: rd_i32(b, at + 56)?,
                num_faces: rd_i32(b, at + 60)?,
                head_node: rd_i32(b, at + 36)?,
                vis_leafs: rd_i32(b, at + 52)?,
            })
        })?;

        let texture_names = parse_texture_names(lump(bytes, LUMP_TEXTURES)?)?;

        // dnode_t: i32 planenum; i16 children[2]; i16 mins[3]; i16 maxs[3];
        // u16 firstface; u16 numfaces.
        let nodes = entries(lump(bytes, LUMP_NODES)?, 24, |b, at| {
            Ok(Node {
                plane: rd_u32(b, at)?,
                children: [rd_i16(b, at + 4)? as i32, rd_i16(b, at + 6)? as i32],
            })
        })?;

        // dleaf_t: i32 contents; i32 visofs; i16 mins[3]; i16 maxs[3];
        // u16 firstmarksurface; u16 nummarksurfaces; u8 ambient_level[4].
        let leaves = entries(lump(bytes, LUMP_LEAVES)?, 28, |b, at| {
            Ok(Leaf {
                contents: rd_i32(b, at)?,
                vis_offset: rd_i32(b, at + 4)?,
            })
        })?;

        let visibility = lump(bytes, LUMP_VISIBILITY)?.to_vec();
        let (head_node, vis_leaf_count) = models
            .first()
            .map(|m| (m.head_node, m.vis_leafs.max(0) as usize))
            .unwrap_or((0, 0));

        let mut bsp = Bsp {
            planes,
            vertices,
            edges,
            surfedges,
            faces,
            texinfo,
            texture_names,
            models,
            nodes,
            leaves,
            visibility,
            head_node,
            vis_leaf_count,
            bounds: Vec::new(),
        };
        bsp.bounds = (0..bsp.faces.len()).map(|i| bsp.compute_bounds(i)).collect();
        Ok(bsp)
    }

    pub fn from_file(path: &std::path::Path) -> Result<Bsp, String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("could not read {}: {}", path.display(), e))?;
        Bsp::parse(&bytes)
    }

    /// Faces belonging to the world model. Everything else is a brush entity.
    pub fn world_faces(&self) -> std::ops::Range<usize> {
        match self.models.first() {
            Some(m) if m.num_faces > 0 => {
                let start = m.first_face.max(0) as usize;
                let end = (start + m.num_faces.max(0) as usize).min(self.faces.len());
                start..end
            }
            _ => 0..0,
        }
    }

    /// The face's polygon, in winding order.
    ///
    /// A surfedge is an index into the edge list; a negative one means the edge
    /// is traversed backwards, so its second vertex comes first. Getting this
    /// backwards produces a polygon with the right vertices in the wrong order,
    /// which point-in-polygon quietly gets wrong rather than rejecting.
    pub fn face_polygon(&self, face_index: usize) -> Vec<[f32; 3]> {
        let Some(face) = self.faces.get(face_index) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(face.num_edges.max(0) as usize);
        for i in 0..face.num_edges.max(0) as usize {
            let Some(&se) = self.surfedges.get(face.first_edge.max(0) as usize + i) else {
                return Vec::new();
            };
            let (edge_index, first) = if se >= 0 {
                (se as usize, 0usize)
            } else {
                ((-se) as usize, 1usize)
            };
            let Some(edge) = self.edges.get(edge_index) else {
                return Vec::new();
            };
            let Some(v) = self.vertices.get(edge[first] as usize) else {
                return Vec::new();
            };
            out.push(*v);
        }
        out
    }

    /// Outward normal of a face: the plane's, flipped when the face sits on the
    /// plane's back side.
    pub fn face_normal(&self, face_index: usize) -> Option<[f32; 3]> {
        let face = self.faces.get(face_index)?;
        let plane = self.planes.get(face.plane as usize)?;
        Some(if face.side != 0 {
            [-plane.normal[0], -plane.normal[1], -plane.normal[2]]
        } else {
            plane.normal
        })
    }

    pub fn texture_name(&self, face_index: usize) -> Option<&str> {
        let face = self.faces.get(face_index)?;
        let ti = self.texinfo.get(face.texinfo.max(0) as usize)?;
        self.texture_names
            .get(ti.miptex.max(0) as usize)
            .map(|s| s.as_str())
    }

    /// Whether a face can actually hold a decal.
    ///
    /// Sky holds nothing, liquids hold nothing, and trigger/clip brushes are not
    /// rendered at all. A coordinate on any of them creates no decal, which
    /// costs the sweep a ring slot without reporting anything.
    pub fn face_takes_decals(&self, face_index: usize) -> bool {
        let Some(face) = self.faces.get(face_index) else {
            return false;
        };
        if let Some(ti) = self.texinfo.get(face.texinfo.max(0) as usize) {
            if ti.flags & TEX_SPECIAL != 0 {
                return false;
            }
        }
        match self.texture_name(face_index) {
            Some(name) => {
                let n = name.to_ascii_lowercase();
                !(n.starts_with("sky")
                    || n.starts_with('!')
                    || n.starts_with('*')
                    || n.starts_with("water")
                    || n.starts_with("aaatrigger")
                    || n.starts_with("clip")
                    || n.contains("nodraw")
                    || n.starts_with("origin")
                    || n.starts_with("hint")
                    || n.starts_with("skip"))
            }
            None => false,
        }
    }

    fn compute_bounds(&self, face_index: usize) -> ([f32; 3], [f32; 3]) {
        let poly = self.face_polygon(face_index);
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for v in &poly {
            for a in 0..3 {
                lo[a] = lo[a].min(v[a]);
                hi[a] = hi[a].max(v[a]);
            }
        }
        (lo, hi)
    }

    pub fn face_bounds(&self, face_index: usize) -> ([f32; 3], [f32; 3]) {
        self.bounds
            .get(face_index)
            .copied()
            .unwrap_or(([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]))
    }

    /// Area of a face, so a big quiet wall can be preferred over a doorframe.
    pub fn face_area(&self, face_index: usize) -> f32 {
        let poly = self.face_polygon(face_index);
        if poly.len() < 3 {
            return 0.0;
        }
        // Fan triangulation from the first vertex; every BSP face is convex.
        let mut total = 0.0;
        for i in 1..poly.len() - 1 {
            let a = sub(&poly[i], &poly[0]);
            let b = sub(&poly[i + 1], &poly[0]);
            total += norm(&cross(&a, &b)) * 0.5;
        }
        total
    }

    /// Perpendicular distance from a point to a face's plane, when the point
    /// projects inside the face's polygon. `None` when it lands off the face.
    pub fn point_on_face(&self, face_index: usize, p: &[f32; 3], tolerance: f32) -> Option<f32> {
        let (lo, hi) = self.face_bounds(face_index);
        for a in 0..3 {
            if p[a] < lo[a] - tolerance || p[a] > hi[a] + tolerance {
                return None;
            }
        }

        let face = self.faces.get(face_index)?;
        let plane = self.planes.get(face.plane as usize)?;
        let dist = (dot(&plane.normal, p) - plane.dist).abs();
        if dist > tolerance {
            return None;
        }

        let poly = self.face_polygon(face_index);
        if poly.len() < 3 {
            return None;
        }
        if point_in_polygon(&poly, &plane.normal, p) {
            Some(dist)
        } else {
            None
        }
    }

    /// Where the engine will actually draw a decal aimed at this point, or
    /// `None` when there is no surface close enough for it to land on.
    ///
    /// `R_DecalShoot` does not draw at the coordinate it is given: it finds the
    /// surface nearest that coordinate and projects onto it. So a coordinate
    /// buried in a wall is not invisible — it is drawn on one of that wall's
    /// faces, and which face that is decides whether a camera sees it. Testing
    /// the coordinate instead of this point is how a sweep can be certified
    /// hidden and still cover a wall in plain view: a point inside solid is
    /// occluded from everywhere, always, by the very surface it renders on.
    ///
    /// The result is lifted `lift` units onto the face's front side, both
    /// because that is the side the decal is visible from and so a trace to it
    /// is not stopped by the face itself.
    pub fn decal_draw_point(&self, p: &[f32; 3], reach: f32, lift: f32) -> Option<[f32; 3]> {
        let (face, _) = self.nearest_face(p, reach)?;
        let normal = self.face_normal(face)?;
        let vertex = *self.face_polygon(face).first()?;
        // Project onto the face plane, then step off it along the normal.
        let offset = dot(&normal, p) - dot(&normal, &vertex);
        Some([
            p[0] - normal[0] * offset + normal[0] * lift,
            p[1] - normal[1] * offset + normal[1] * lift,
            p[2] - normal[2] * offset + normal[2] * lift,
        ])
    }

    /// The nearest world face a point sits on, within `tolerance`.
    ///
    /// This is the parser's own test. Every harvested decal is a coordinate the
    /// engine accepted, so if the lumps are being read correctly nearly all of
    /// them land on a face here.
    pub fn nearest_face(&self, p: &[f32; 3], tolerance: f32) -> Option<(usize, f32)> {
        let mut best: Option<(usize, f32)> = None;
        for i in self.world_faces() {
            if let Some(d) = self.point_on_face(i, p, tolerance) {
                if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                    best = Some((i, d));
                }
            }
        }
        best
    }
}

/// The TEXTURES lump is a directory: a count, then one offset per texture
/// relative to the lump start, then the miptex headers those point at. An
/// offset of -1 means the texture lives in a WAD and only the name is present.
fn parse_texture_names(data: &[u8]) -> Result<Vec<String>, String> {
    if data.len() < 4 {
        return Ok(Vec::new());
    }
    let count = rd_u32(data, 0)? as usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let at = 4 + i * 4;
        let Ok(offset) = rd_i32(data, at) else {
            break;
        };
        if offset < 0 {
            out.push(String::new());
            continue;
        }
        let start = offset as usize;
        let name = data
            .get(start..start + 16)
            .map(|raw| {
                let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
                String::from_utf8_lossy(&raw[..end]).into_owned()
            })
            .unwrap_or_default();
        out.push(name);
    }
    Ok(out)
}

fn sub(a: &[f32; 3], b: &[f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: &[f32; 3], b: &[f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(v: &[f32; 3]) -> f32 {
    dot(v, v).sqrt()
}

/// The axis to drop when flattening a polygon whose plane has this normal:
/// the one the normal points most strongly along, which is the projection that
/// cannot collapse the polygon to a line.
pub(super) fn dominant_axis(normal: &[f32; 3]) -> usize {
    let (mut best, mut idx) = (0.0f32, 0usize);
    for (a, n) in normal.iter().enumerate() {
        if n.abs() > best {
            best = n.abs();
            idx = a;
        }
    }
    idx
}

/// Point-in-polygon by the crossing-number rule, in the plane's dominant
/// projection. Convex faces would allow a cheaper sign test, but BSP faces are
/// not reliably wound consistently across compilers and this does not care.
fn point_in_polygon(poly: &[[f32; 3]], normal: &[f32; 3], p: &[f32; 3]) -> bool {
    let drop = dominant_axis(normal);
    let (u, v) = match drop {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };

    let (px, py) = (p[u], p[v]);
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = (poly[i][u], poly[i][v]);
        let (xj, yj) = (poly[j][u], poly[j][v]);
        if (yi > py) != (yj > py) {
            let t = (py - yi) / (yj - yi);
            if px < xi + t * (xj - xi) {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}


// ── Visibility: is a point hidden from a point ───────────────────────────────
//
// The flush's whole safety claim is that its decals are never on screen, and
// until now the only test available was a cone: reject anything within N
// degrees of a camera's view axis, with no notion of what stands between them.
// That is wrong in both directions, and the expensive one is over-rejection —
// most of what the cone throws away on a busy map is behind a wall.
//
// Two tests, cheapest first:
//
//   1. PVS. If the candidate's leaf is absent from the camera leaf's
//      potentially-visible set, nothing in that leaf can be rendered from
//      anywhere in the camera's leaf. That is a hard guarantee for one lookup.
//   2. A segment trace through the node tree for whatever survives.

/// Leaf contents that stop a line of sight. Empty space and water do not;
/// water is transparent, and a decal behind it is visible through it.
pub const CONTENTS_SOLID: i32 = -2;
pub const CONTENTS_SKY: i32 = -6;

/// Guard against a malformed tree sending the descent or the trace into a
/// runaway. Real GoldSrc trees are nowhere near this deep.
const MAX_TREE_DEPTH: u32 = 512;

/// Distance either side of a splitting plane treated as "on it", so a segment
/// running along a wall does not thrash between children on rounding noise.
const PLANE_EPSILON: f32 = 0.03125;

#[derive(Debug, Clone, Copy)]
pub struct Node {
    pub plane: u32,
    /// Non-negative is a child node index; negative encodes a leaf as
    /// `-1 - child`.
    pub children: [i32; 2],
}

#[derive(Debug, Clone, Copy)]
pub struct Leaf {
    pub contents: i32,
    /// Offset into the visibility lump, or -1 when the map has no vis data.
    pub vis_offset: i32,
}

impl Bsp {
    /// Leaf containing a point, by descending the world node tree.
    pub fn leaf_at(&self, p: &[f32; 3]) -> usize {
        let mut node = self.head_node;
        let mut depth = 0;
        while node >= 0 {
            depth += 1;
            if depth > MAX_TREE_DEPTH {
                return 0;
            }
            let Some(n) = self.nodes.get(node as usize) else {
                return 0;
            };
            let Some(plane) = self.planes.get(n.plane as usize) else {
                return 0;
            };
            let side = usize::from(dot(&plane.normal, p) - plane.dist < 0.0);
            node = n.children[side];
        }
        (-1 - node) as usize
    }

    /// Contents of a leaf, or `CONTENTS_SOLID` for an index the tree should
    /// not have produced — the same conservative reading `leaf_blocks` takes.
    pub fn leaf_contents(&self, leaf: usize) -> i32 {
        self.leaves.get(leaf).map(|l| l.contents).unwrap_or(CONTENTS_SOLID)
    }

    fn leaf_blocks(&self, leaf: usize) -> bool {
        match self.leaves.get(leaf) {
            Some(l) => l.contents == CONTENTS_SOLID || l.contents == CONTENTS_SKY,
            // A leaf index the tree should not have produced. Treating it as
            // blocking is the conservative choice: it costs a candidate
            // position rather than exposing one.
            None => true,
        }
    }

    /// Whether anything solid stands between two points.
    ///
    /// This is the geometric half of "can the camera see it". Brush entities —
    /// doors, lifts — are deliberately not consulted: they are not in the world
    /// tree, and a door's position at any given moment is not knowable from
    /// here. Ignoring them means a spot hidden behind a closed door reads as
    /// visible, which loses a usable position rather than exposing one.
    pub fn line_blocked(&self, from: &[f32; 3], to: &[f32; 3]) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        self.segment_blocked(self.head_node, from, to, 0)
    }

    fn segment_blocked(&self, node: i32, p1: &[f32; 3], p2: &[f32; 3], depth: u32) -> bool {
        if depth > MAX_TREE_DEPTH {
            return true;
        }
        if node < 0 {
            return self.leaf_blocks((-1 - node) as usize);
        }
        let Some(n) = self.nodes.get(node as usize) else {
            return true;
        };
        let Some(plane) = self.planes.get(n.plane as usize) else {
            return true;
        };

        let d1 = dot(&plane.normal, p1) - plane.dist;
        let d2 = dot(&plane.normal, p2) - plane.dist;

        // Wholly on one side: only that child can contain the segment.
        if d1 >= -PLANE_EPSILON && d2 >= -PLANE_EPSILON {
            return self.segment_blocked(n.children[0], p1, p2, depth + 1);
        }
        if d1 < PLANE_EPSILON && d2 < PLANE_EPSILON {
            return self.segment_blocked(n.children[1], p1, p2, depth + 1);
        }

        // Straddles the plane: split it and walk the near half first, so a hit
        // close to the origin ends the search without touching the far half.
        let denom = d1 - d2;
        let frac = if denom.abs() < 1e-6 {
            0.5
        } else {
            (d1 / denom).clamp(0.0, 1.0)
        };
        let mid = [
            p1[0] + (p2[0] - p1[0]) * frac,
            p1[1] + (p2[1] - p1[1]) * frac,
            p1[2] + (p2[2] - p1[2]) * frac,
        ];
        let (near, far) = if d1 >= 0.0 { (0, 1) } else { (1, 0) };

        self.segment_blocked(n.children[near], p1, &mid, depth + 1)
            || self.segment_blocked(n.children[far], &mid, p2, depth + 1)
    }

    /// Bytes of one leaf's decompressed PVS row, or `None` when the map carries
    /// no visibility data at all (`-vis` never run, or a leaf outside it).
    ///
    /// The lump is run-length encoded: a non-zero byte is a bitmask of eight
    /// leaves, and a zero byte is followed by a count of zero bytes to skip.
    pub fn pvs_row(&self, leaf: usize) -> Option<Vec<u8>> {
        let vis_offset = self.leaves.get(leaf)?.vis_offset;
        if vis_offset < 0 || self.visibility.is_empty() || self.vis_leaf_count == 0 {
            return None;
        }

        let row_bytes = self.vis_leaf_count.div_ceil(8);
        let mut row = vec![0u8; row_bytes];
        let mut at = vis_offset as usize;
        let mut i = 0usize;

        while i < row_bytes {
            let byte = *self.visibility.get(at)?;
            at += 1;
            if byte != 0 {
                row[i] = byte;
                i += 1;
                continue;
            }
            // A zero run. A zero-length run would loop forever on malformed
            // data, so it ends the row instead.
            let run = *self.visibility.get(at)? as usize;
            at += 1;
            if run == 0 {
                break;
            }
            i += run;
        }
        Some(row)
    }

    /// Whether `leaf` is set in a decompressed PVS row.
    ///
    /// Leaf 0 is the solid leaf and is not represented in vis data, so rows are
    /// indexed from leaf 1.
    pub fn pvs_contains(row: &[u8], leaf: usize) -> bool {
        if leaf == 0 {
            return false;
        }
        let bit = leaf - 1;
        row.get(bit / 8)
            .map(|b| b & (1 << (bit % 8)) != 0)
            .unwrap_or(false)
    }

    /// Union of the PVS of every leaf in `leaves`, as one row.
    ///
    /// Cameras cluster heavily, so the caller is expected to collapse thousands
    /// of samples into a handful of distinct leaves before calling this. A
    /// `None` return means at least one leaf had no vis data, in which case
    /// nothing can be ruled out and the caller must fall back to tracing.
    pub fn pvs_union(&self, leaves: &[usize]) -> Option<Vec<u8>> {
        if leaves.is_empty() || self.vis_leaf_count == 0 {
            return None;
        }
        let row_bytes = self.vis_leaf_count.div_ceil(8);
        let mut out = vec![0u8; row_bytes];
        let mut any = false;
        for &leaf in leaves {
            let Some(row) = self.pvs_row(leaf) else {
                // No vis data for a camera's own leaf: it could potentially see
                // anywhere, so the union is useless as a filter.
                return None;
            };
            any = true;
            for (o, r) in out.iter_mut().zip(row.iter()) {
                *o |= r;
            }
        }
        any.then_some(out)
    }

    /// Whether the map carries usable visibility data.
    pub fn has_vis(&self) -> bool {
        !self.visibility.is_empty() && self.vis_leaf_count > 0
    }
}

/// The lump the map checksum deliberately leaves out.
///
/// Entities are what a server operator edits — spawn counts, round timers — and
/// excluding them lets a server run a tweaked map without every client
/// reporting a mismatch. It also means the checksum answers exactly the question
/// worth asking here: is this the same *geometry* the demo was recorded on.
const LUMP_ENTITIES: usize = 0;

/// The map checksum the engine stamps into a demo header, computed from a map
/// file.
///
/// CRC-32 (the ordinary reflected IEEE polynomial) over lumps 1..14 in header
/// order, entities excluded — and left *unfinalised*, with no closing XOR. The
/// engine's `CRC32_Final` is not called on this value before it is written, so
/// finalising it here would miss by exactly `0xFFFFFFFF`.
///
/// Verified against 390 first-person demos: every one matches its own map.
pub fn map_checksum(bytes: &[u8]) -> Result<u32, String> {
    if bytes.len() < HEADER_SIZE {
        return Err("file is shorter than a BSP header".to_string());
    }

    let mut table = [0u32; 256];
    for (n, slot) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }

    let mut crc = 0xFFFF_FFFFu32;
    for index in 0..LUMP_COUNT {
        if index == LUMP_ENTITIES {
            continue;
        }
        for b in lump(bytes, index)? {
            crc = table[((crc ^ *b as u32) & 0xFF) as usize] ^ (crc >> 8);
        }
    }
    Ok(crc)
}

/// `map_checksum` for a map on disk.
pub fn map_checksum_of_file(path: &std::path::Path) -> Result<u32, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    map_checksum(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A single square face at x = 100, spanning y and z from 0 to 64, built by
    /// hand so the parser's own arithmetic is exercised without a map file.
    fn synthetic_bsp() -> Bsp {
        let mut bsp = Bsp {
            planes: vec![Plane {
                normal: [1.0, 0.0, 0.0],
                dist: 100.0,
            }],
            vertices: vec![
                [100.0, 0.0, 0.0],
                [100.0, 64.0, 0.0],
                [100.0, 64.0, 64.0],
                [100.0, 0.0, 64.0],
            ],
            edges: vec![[0, 1], [1, 2], [2, 3], [3, 0]],
            surfedges: vec![0, 1, 2, 3],
            faces: vec![Face {
                plane: 0,
                side: 0,
                first_edge: 0,
                num_edges: 4,
                texinfo: 0,
            }],
            texinfo: vec![TexInfo {
                miptex: 0,
                flags: 0,
            }],
            texture_names: vec!["brick_wall".to_string()],
            models: vec![Model {
                first_face: 0,
                num_faces: 1,
                head_node: 0,
                vis_leafs: 2,
            }],
            // A one-plane world: everything at x > 100 is inside the wall,
            // everything at x < 100 is the room in front of it. Enough to make
            // the leaf descent and the segment trace answerable by hand.
            nodes: vec![Node {
                plane: 0,
                // Front of the plane is solid, back is the open room. Children
                // encode a leaf as -1 - index, so -1 is leaf 0 and -2 is leaf 1.
                children: [-1, -2],
            }],
            leaves: vec![
                Leaf {
                    contents: CONTENTS_SOLID,
                    vis_offset: -1,
                },
                Leaf {
                    contents: -1,
                    vis_offset: 0,
                },
            ],
            // One row, one byte: leaf 1 can see itself and not leaf 2.
            visibility: vec![0b0000_0001],
            head_node: 0,
            vis_leaf_count: 2,
            bounds: Vec::new(),
        };
        bsp.bounds = (0..bsp.faces.len()).map(|i| bsp.compute_bounds(i)).collect();
        bsp
    }

    #[test]
    fn a_polygon_is_recovered_in_winding_order() {
        let bsp = synthetic_bsp();
        let poly = bsp.face_polygon(0);
        assert_eq!(poly.len(), 4);
        assert!(poly.iter().all(|v| (v[0] - 100.0).abs() < 0.001));
    }

    #[test]
    fn a_negative_surfedge_traverses_its_edge_backwards() {
        // Getting this wrong yields the right vertices in the wrong order, which
        // point-in-polygon silently gets wrong rather than rejecting.
        let mut bsp = synthetic_bsp();
        // Edge 1 joins vertices 1 and 2, so the two directions are
        // distinguishable. (Edge 0 would not be: -0 == 0.)
        bsp.surfedges = vec![1, 1, 1, 1];
        let forward = bsp.face_polygon(0);
        bsp.surfedges = vec![-1, -1, -1, -1];
        let backward = bsp.face_polygon(0);

        assert_eq!(
            forward[0], bsp.vertices[1],
            "a positive surfedge starts at its edge's first vertex"
        );
        assert_eq!(
            backward[0], bsp.vertices[2],
            "a negative one starts at its edge's second vertex"
        );
    }

    #[test]
    fn a_point_on_the_face_is_found() {
        let bsp = synthetic_bsp();
        let hit = bsp.nearest_face(&[100.0, 32.0, 32.0], 4.0);
        assert_eq!(hit.map(|(i, _)| i), Some(0));
        assert!(hit.unwrap().1 < 0.001);
    }

    #[test]
    fn a_point_off_the_end_of_the_face_is_not() {
        // The whole risk of tiling past a wall's edge: the coordinate looks
        // plausible and creates no decal.
        let bsp = synthetic_bsp();
        assert!(bsp.nearest_face(&[100.0, 400.0, 32.0], 4.0).is_none());
    }

    #[test]
    fn a_point_off_the_plane_beyond_tolerance_is_not() {
        let bsp = synthetic_bsp();
        assert!(bsp.nearest_face(&[100.0, 32.0, 32.0], 4.0).is_some());
        assert!(bsp.nearest_face(&[120.0, 32.0, 32.0], 4.0).is_none());
    }

    #[test]
    fn sky_and_trigger_faces_are_rejected() {
        let mut bsp = synthetic_bsp();
        assert!(bsp.face_takes_decals(0));

        bsp.texture_names = vec!["sky".to_string()];
        assert!(!bsp.face_takes_decals(0));

        bsp.texture_names = vec!["aaatrigger".to_string()];
        assert!(!bsp.face_takes_decals(0));

        bsp.texture_names = vec!["brick_wall".to_string()];
        bsp.texinfo[0].flags = TEX_SPECIAL;
        assert!(!bsp.face_takes_decals(0), "TEX_SPECIAL holds no decal");
    }

    #[test]
    fn area_is_measured_in_world_units() {
        let bsp = synthetic_bsp();
        assert!((bsp.face_area(0) - 64.0 * 64.0).abs() < 1.0);
    }

    #[test]
    fn only_the_world_model_is_walked() {
        let mut bsp = synthetic_bsp();
        bsp.models = vec![
            Model {
                first_face: 0,
                num_faces: 0,
                head_node: 0,
                vis_leafs: 0,
            },
            Model {
                first_face: 0,
                num_faces: 1,
                head_node: 0,
                vis_leafs: 0,
            },
        ];
        assert!(
            bsp.nearest_face(&[100.0, 32.0, 32.0], 4.0).is_none(),
            "a face owned by a brush entity must not count as world surface"
        );
    }

    #[test]
    fn a_non_goldsrc_file_is_rejected() {
        let mut bytes = vec![0u8; HEADER_SIZE];
        bytes[0..4].copy_from_slice(&46_i32.to_le_bytes());
        assert!(Bsp::parse(&bytes).is_err());
    }

    #[test]
    fn a_truncated_file_is_rejected_rather_than_panicking() {
        let mut bytes = vec![0u8; HEADER_SIZE];
        bytes[0..4].copy_from_slice(&BSP_VERSION.to_le_bytes());
        // Every lump claims a huge length that runs past the end.
        for i in 0..LUMP_COUNT {
            let at = 4 + i * 8;
            bytes[at..at + 4].copy_from_slice(&(HEADER_SIZE as i32).to_le_bytes());
            bytes[at + 4..at + 8].copy_from_slice(&1_000_000_i32.to_le_bytes());
        }
        assert!(Bsp::parse(&bytes).is_err());
    }

    #[test]
    fn a_point_descends_to_the_leaf_it_is_in() {
        let bsp = synthetic_bsp();
        // In front of the wall is the open room, behind its plane is solid.
        assert_eq!(bsp.leaf_at(&[50.0, 32.0, 32.0]), 1, "the room");
        assert_eq!(bsp.leaf_at(&[150.0, 32.0, 32.0]), 0, "inside the wall");
    }

    #[test]
    fn a_wall_between_two_points_blocks_the_line() {
        // The whole point of the occlusion work: a spot the cone test would
        // reject as "in front of the camera" is fine if this says blocked.
        let bsp = synthetic_bsp();
        assert!(
            bsp.line_blocked(&[50.0, 32.0, 32.0], &[150.0, 32.0, 32.0]),
            "a segment crossing into solid must be blocked"
        );
    }

    #[test]
    fn an_open_line_is_not_blocked() {
        let bsp = synthetic_bsp();
        assert!(!bsp.line_blocked(&[20.0, 32.0, 32.0], &[60.0, 32.0, 32.0]));
    }

    #[test]
    fn a_line_is_blocked_from_either_end() {
        // The trace splits at the plane and walks the near half first, so the
        // two directions take different paths through the recursion and both
        // have to agree.
        let bsp = synthetic_bsp();
        let a = [50.0, 32.0, 32.0];
        let b = [150.0, 32.0, 32.0];
        assert_eq!(bsp.line_blocked(&a, &b), bsp.line_blocked(&b, &a));
    }

    #[test]
    fn sky_blocks_a_line_the_way_solid_does() {
        let mut bsp = synthetic_bsp();
        bsp.leaves[0].contents = CONTENTS_SKY;
        assert!(bsp.line_blocked(&[50.0, 32.0, 32.0], &[150.0, 32.0, 32.0]));
    }

    #[test]
    fn water_does_not_block_a_line() {
        // Water is transparent: a decal behind it is visible through it, so
        // treating it as an occluder would hide a spot that is actually in shot.
        let mut bsp = synthetic_bsp();
        bsp.leaves[0].contents = -3; // CONTENTS_WATER
        assert!(!bsp.line_blocked(&[50.0, 32.0, 32.0], &[150.0, 32.0, 32.0]));
    }

    #[test]
    fn a_pvs_row_decompresses_and_reads_back() {
        let bsp = synthetic_bsp();
        let row = bsp.pvs_row(1).expect("leaf 1 has vis data");
        assert!(Bsp::pvs_contains(&row, 1), "leaf 1 sees itself");
        assert!(!Bsp::pvs_contains(&row, 2), "and not leaf 2");
        assert!(
            !Bsp::pvs_contains(&row, 0),
            "leaf 0 is the solid leaf and is never in a vis row"
        );
    }

    #[test]
    fn a_zero_run_skips_the_leaves_it_covers() {
        // The lump is run-length encoded: a zero byte is followed by a count of
        // zero bytes to skip. Reading that as a literal byte would shift every
        // later leaf's bit and quietly mis-answer visibility across the map.
        let mut bsp = synthetic_bsp();
        bsp.vis_leaf_count = 24;
        // Skip two zero bytes (leaves 1-16), then set bit 0 of the third byte,
        // which is leaf 17.
        bsp.visibility = vec![0x00, 0x02, 0b0000_0001];
        let row = bsp.pvs_row(1).unwrap();

        assert_eq!(row.len(), 3);
        assert!(Bsp::pvs_contains(&row, 17), "leaf 17 should be visible");
        for leaf in 1..=16 {
            assert!(!Bsp::pvs_contains(&row, leaf), "leaf {} should not be", leaf);
        }
    }

    #[test]
    fn a_leaf_without_vis_data_has_no_row() {
        let bsp = synthetic_bsp();
        assert!(bsp.pvs_row(0).is_none(), "the solid leaf carries no vis");
    }

    #[test]
    fn a_map_compiled_without_vis_reports_it() {
        let mut bsp = synthetic_bsp();
        bsp.visibility.clear();
        assert!(!bsp.has_vis());
        assert!(bsp.pvs_row(1).is_none());
        assert!(bsp.pvs_union(&[1]).is_none());
    }

    #[test]
    fn a_union_that_cannot_be_completed_is_refused() {
        // If any camera leaf has no vis data it could potentially see anywhere,
        // so the union rules nothing out and the caller must fall back to
        // tracing rather than trusting a partial answer.
        let bsp = synthetic_bsp();
        assert!(bsp.pvs_union(&[1]).is_some());
        assert!(
            bsp.pvs_union(&[0, 1]).is_none(),
            "a leaf without vis must void the union, not be skipped"
        );
    }

    #[test]
    fn a_union_covers_every_leaf_any_camera_can_see() {
        let mut bsp = synthetic_bsp();
        bsp.vis_leaf_count = 8;
        bsp.visibility = vec![0b0000_0001, 0b0000_0010];
        bsp.leaves = vec![
            Leaf { contents: CONTENTS_SOLID, vis_offset: -1 },
            Leaf { contents: -1, vis_offset: 0 },
            Leaf { contents: -1, vis_offset: 1 },
        ];

        let union = bsp.pvs_union(&[1, 2]).unwrap();
        assert!(Bsp::pvs_contains(&union, 1), "from leaf 1");
        assert!(Bsp::pvs_contains(&union, 2), "from leaf 2");
        assert!(!Bsp::pvs_contains(&union, 3));
    }

    /// The defect this exists for: the engine draws on the surface, not at the
    /// coordinate, so the coordinate is not what a camera test may judge.
    #[test]
    fn decal_draw_point_projects_onto_the_face_and_steps_off_it() {
        let bsp = synthetic_bsp();
        // Two units off the plane, well inside the face's own square.
        let drawn = bsp
            .decal_draw_point(&[98.0, 32.0, 32.0], 4.0, 1.0)
            .expect("a face two units away is within reach");
        // Projected back onto x = 100, then lifted one unit along the normal.
        assert!((drawn[0] - 101.0).abs() < 1e-3, "{:?}", drawn);
        assert!((drawn[1] - 32.0).abs() < 1e-3, "{:?}", drawn);
        assert!((drawn[2] - 32.0).abs() < 1e-3, "{:?}", drawn);
    }

    /// Tiling lays a grid across a fitted plane, and the plane runs on past the
    /// brush that proved it. Tiles landing in open air are not decal spots and
    /// must resolve to nothing rather than to some distant face.
    #[test]
    fn decal_draw_point_gives_up_beyond_its_reach() {
        let bsp = synthetic_bsp();
        assert!(bsp.decal_draw_point(&[80.0, 32.0, 32.0], 4.0, 1.0).is_none());
        // The same point is answerable if the reach is widened to cover it,
        // which is what makes the constant the thing that decides, not the map.
        assert!(bsp.decal_draw_point(&[80.0, 32.0, 32.0], 32.0, 1.0).is_some());
    }

    /// Off the end of the face: on the plane, but there is no surface there.
    #[test]
    fn decal_draw_point_needs_the_face_not_just_its_plane() {
        let bsp = synthetic_bsp();
        assert!(bsp.decal_draw_point(&[98.0, 500.0, 32.0], 4.0, 1.0).is_none());
    }
}
