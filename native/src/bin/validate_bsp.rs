//! TEMPORARY validation harness — not for commit.
//!
//! Stage 2 of docs/decal_flush_bsp_surfaces.md. Every harvested decal in a demo
//! is a coordinate the engine provably accepted, so running thousands of them
//! against the parsed map geometry validates the lump offsets, edge winding and
//! coordinate space in one pass. A wrong offset collapses the hit rate rather
//! than producing plausible garbage.
//!
//! Prints one TSV line: map, demo, decals tested, hits at each tolerance.

use native::patch::bsp::Bsp;
use native::patch::decal_atlas::MapKey;
use native::patch::decal_strip::proven_world_coordinates;

fn main() {
    let mut args = std::env::args().skip(1);
    let demo_path = args.next().expect("usage: <demo> <maps_dir> <label>");
    let maps_dir = std::path::PathBuf::from(args.next().expect("maps dir"));
    let label = args.next().unwrap_or_default();

    let path = std::path::PathBuf::from(&demo_path);
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let bail = |why: &str| {
        println!("{}\t{}\tSKIP\t{}", label, name, why);
        std::process::exit(0);
    };

    let Ok(bytes) = std::fs::read(&path) else {
        bail("read failed");
        return;
    };
    let Ok(demo) = dem::open_demo_from_bytes(&bytes) else {
        bail("parse failed");
        return;
    };
    let Some(key) = MapKey::from_header(&demo.header) else {
        bail("no map name in header");
        return;
    };

    let bsp_path = maps_dir.join(format!("{}.bsp", key.name));
    let bsp = match Bsp::from_file(&bsp_path) {
        Ok(b) => b,
        Err(e) => {
            bail(&format!("bsp: {}", e));
            return;
        }
    };

    // Harvest with no capture windows: every decal in the demo, which is what
    // we want here — this is about the parser, not about the flush.
    // World decals only. A mark on a door is a true coordinate for the demo but
    // sits on a brush entity, and the world model is what we parsed.
    let pts = proven_world_coordinates(&demo);
    let pts = &pts;
    if pts.is_empty() {
        bail("no world decals harvested");
    }

    let mut hits = [0usize; 4];
    let tolerances = [1.0f32, 2.0, 4.0, 8.0];
    let mut worst_miss = 0usize;
    for p in pts {
        let mut landed = false;
        for (i, tol) in tolerances.iter().enumerate() {
            if bsp.nearest_face(p, *tol).is_some() {
                hits[i] += 1;
                landed = true;
            }
        }
        if !landed {
            worst_miss += 1;
        }
    }

    let pct = |n: usize| (n as f32 / pts.len() as f32) * 100.0;
    println!(
        "{}\t{}\tOK\t{}\t{}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{}\t{}\t{}",
        label,
        name,
        key.name,
        pts.len(),
        pct(hits[0]),
        pct(hits[1]),
        pct(hits[2]),
        pct(hits[3]),
        worst_miss,
        bsp.world_faces().len(),
        bsp.faces.len(),
    );
}
