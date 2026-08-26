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
// Only what that needs is parsed. Nodes, leaves, visibility, lighting and
// clipnodes are skipped entirely: the flush wants "where are the surfaces and
// how big are they", not a renderer.
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
const LUMP_TEXINFO: usize = 6;
const LUMP_FACES: usize = 7;
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

/// Only `first_face`/`num_faces` matter here. Model 0 is the world; every other
/// model is a brush entity — a door, a lift, a train — whose faces move with it
/// and whose coordinates are therefore only true while it is where it was.
#[derive(Debug, Clone, Copy)]
pub struct Model {
    pub first_face: i32,
    pub num_faces: i32,
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
            })
        })?;

        let texture_names = parse_texture_names(lump(bytes, LUMP_TEXTURES)?)?;

        let mut bsp = Bsp {
            planes,
            vertices,
            edges,
            surfedges,
            faces,
            texinfo,
            texture_names,
            models,
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
            }],
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
            },
            Model {
                first_face: 0,
                num_faces: 1,
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
}
