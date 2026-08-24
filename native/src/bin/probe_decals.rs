//! Measures GoldSrc's decal placement tolerance — how far from a solid surface
//! a decal message's position may sit and still create a decal.
//!
//! Strips every decal out of a demo, stamps a three-row grid of decals onto one
//! wall the demo proves exists, and pins r_decals high. Each column of the grid
//! uses a larger offset than the last, so a row simply runs out of holes where
//! the engine stopped accepting the position. Play the output back, count the
//! holes in each row, and the threshold falls out.
//!
//! This is the measurement the flush burst in `strip_decals` currently guesses
//! at: if positions can be synthesised near one known-good surface point rather
//! than only harvested from the demo, position supply stops being the binding
//! constraint on the ring sweep.

use clap::Parser;
use native::patch::{probe_decal_offsets, ProbeOptions, ProbeRow};
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Stamps an offset-measurement decal grid onto a wall in a DoD demo."
)]
struct Args {
    /// Path to the source .dem file
    demo_path: PathBuf,

    /// Output path for the probe demo
    #[arg(long)]
    out: PathBuf,

    /// Offsets from the surface to test, ascending, e.g. --offsets 2,4,8,16,32
    #[arg(long, value_parser = parse_offsets)]
    offsets: Option<Vec<f32>>,

    /// Gap between adjacent columns, along the wall.
    #[arg(long)]
    column_spacing: Option<f32>,

    /// Gap between the three rows, across the wall.
    #[arg(long)]
    row_gap: Option<f32>,

    /// Decal texture index, overriding the one harvested from the demo.
    #[arg(long)]
    texture_index: Option<u8>,

    /// Force the plane's normal axis: 0=x, 1=y, 2=z.
    #[arg(long)]
    axis: Option<usize>,

    /// Hand-picked anchor "x,y,z" on a known surface, skipping plane
    /// detection. Requires --axis.
    #[arg(long, value_parser = parse_coord)]
    anchor: Option<[f32; 3]>,

    /// Decals that must share a plane before it counts as a wall.
    #[arg(long)]
    min_plane_decals: Option<usize>,

    /// Extent a plane's decals must span to count as a wall rather than a
    /// cluster on some small prop.
    #[arg(long)]
    min_plane_spread: Option<f32>,

    /// Value to pin r_decals to.
    #[arg(long)]
    ring_limit: Option<u32>,

    /// Leave the demo's own decals in place instead of stripping them.
    #[arg(long)]
    no_strip: bool,

    /// Frame ordinals of lead time between the grid being stamped and the
    /// first moment the camera looks at it.
    #[arg(long)]
    lead_ticks: Option<i32>,
}

fn parse_offsets(s: &str) -> Result<Vec<f32>, String> {
    s.split(',')
        .map(|p| {
            p.trim()
                .parse::<f32>()
                .map_err(|_| format!("bad offset '{}' in '{}'", p, s))
        })
        .collect()
}

fn parse_coord(s: &str) -> Result<[f32; 3], String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 3 {
        return Err(format!("expected x,y,z — got '{}'", s));
    }
    let mut out = [0.0f32; 3];
    for (i, p) in parts.iter().enumerate() {
        out[i] = p
            .trim()
            .parse()
            .map_err(|_| format!("bad number '{}' in '{}'", p, s))?;
    }
    Ok(out)
}

fn axis_name(axis: usize) -> &'static str {
    ["X", "Y", "Z"][axis]
}

fn clock(seconds: f32) -> String {
    let total = seconds.max(0.0) as u32;
    format!("{}:{:02}", total / 60, total % 60)
}

fn main() {
    let args = Args::parse();

    let bytes = match fs::read(&args.demo_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error reading demo file: {}", e);
            std::process::exit(1);
        }
    };

    let defaults = ProbeOptions::default();
    let opts = ProbeOptions {
        offsets: args.offsets.unwrap_or(defaults.offsets),
        column_spacing: args.column_spacing.unwrap_or(defaults.column_spacing),
        row_gap: args.row_gap.unwrap_or(defaults.row_gap),
        texture_index: args.texture_index,
        axis: args.axis,
        anchor: args.anchor,
        min_plane_decals: args.min_plane_decals.unwrap_or(defaults.min_plane_decals),
        min_plane_spread: args.min_plane_spread.unwrap_or(defaults.min_plane_spread),
        ring_limit: args.ring_limit.unwrap_or(defaults.ring_limit),
        strip_all: !args.no_strip,
        lead_ticks: args.lead_ticks.unwrap_or(defaults.lead_ticks),
        ..defaults
    };

    let (out_bytes, stats) = match probe_decal_offsets(&bytes, &opts) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error building probe demo: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = fs::write(&args.out, &out_bytes) {
        eprintln!("Error writing output demo: {}", e);
        std::process::exit(1);
    }

    let normal = axis_name(stats.axis);
    let col = axis_name(stats.column_axis);
    let row = axis_name(stats.row_axis);
    let sign = if stats.outward > 0.0 { "+" } else { "-" };

    println!("Wrote probe demo to: {}", args.out.display());
    println!();
    println!("SURFACE");
    println!(
        "  Normal axis:        {}  ({})",
        normal,
        if stats.axis == 2 { "floor/ceiling" } else { "upright wall" }
    );
    println!("  Plane coordinate:   {} = {:.1}", normal, stats.plane_value);
    println!(
        "  Decals proving it:  {} of {} harvested, spanning {:.0} x {:.0} units",
        stats.plane_members, stats.harvested_decals, stats.plane_spread.0, stats.plane_spread.1
    );
    println!(
        "  Anchor:             [{:.1}, {:.1}, {:.1}]",
        stats.anchor[0], stats.anchor[1], stats.anchor[2]
    );
    println!("  Open space lies:    {}{}", sign, normal);
    println!("  Columns run along:  {},  rows step along {}", col, row);
    println!("  Column pitch:       {:.0} units (each column sits on a real decal)", stats.column_pitch);
    println!();

    println!(
        "GRID  {} decals, texture index {}, stamped at frame ordinal {}",
        stats.probes.len(),
        stats.texture_index,
        stats.injected_at_ordinal
    );
    let offsets: Vec<f32> = stats
        .probes
        .iter()
        .filter(|p| p.row == ProbeRow::Control)
        .map(|p| p.offset)
        .collect();
    let columns: Vec<f32> = stats
        .probes
        .iter()
        .filter(|p| p.row == ProbeRow::Control)
        .map(|p| p.position[stats.column_axis])
        .collect();
    print!("  Offset (units):  ");
    for o in &offsets {
        print!("{:>7.0}", o);
    }
    println!();
    print!("  Column {}:       ", col);
    for c in &columns {
        print!("{:>7.0}", c);
    }
    println!();
    print!("  Nearest decal:   ");
    for e in &stats.column_evidence {
        if e.is_finite() {
            print!("{:>7.0}", e);
        } else {
            print!("{:>7}", "-");
        }
    }
    println!();
    for r in [ProbeRow::Out, ProbeRow::Control, ProbeRow::In] {
        let sample = stats.probes.iter().find(|p| p.row == r).unwrap();
        let toward = match r {
            ProbeRow::Out => format!("{} = plane {} offset  (into open space)", normal, sign),
            ProbeRow::Control => format!("{} = plane exactly    (control)", normal),
            ProbeRow::In => format!(
                "{} = plane {} offset  (into the solid)",
                normal,
                if stats.outward > 0.0 { "-" } else { "+" }
            ),
        };
        println!(
            "  {} row:  {} = {:>6.1},  {}",
            r.label(),
            row,
            sample.position[stats.row_axis],
            toward
        );
    }
    println!();

    if stats.columns_backed < offsets.len() {
        println!(
            "  WARNING: only {} of {} columns have a real decal within half a pitch.\n  \
             The rest are placed by arithmetic and may hang over a doorway or an edge.\n  \
             The CTL row will show it if so — a short CTL row voids the run.",
            stats.columns_backed,
            offsets.len()
        );
    }
    println!();

    println!(
        "Stripped from the demo: {} decals, {} sprays",
        stats.decals_stripped, stats.sprays_stripped
    );
    println!("r_decals pinned to:     {}", opts.ring_limit);
    println!();

    println!("HOW TO READ IT");
    if stats.sightings.is_empty() {
        println!("  No moment found where the camera looks at this wall from close range.");
        println!("  The grid is stamped near the start of the demo and nothing can evict it,");
        println!("  so it is on the wall for the whole playback — but you will have to find");
        println!("  the wall yourself. Re-run with --anchor x,y,z --axis N to aim it at a");
        println!("  surface you know the POV player looks at.");
    } else {
        let times: Vec<String> = stats.sightings.iter().take(6).map(|t| clock(*t)).collect();
        println!(
            "  Play (don't seek) to {} on the viewdemo clock and look at the wall.",
            times.join(", then ")
        );
        if let Some(d) = stats.best_sighting_distance {
            println!("  Closest the camera gets to it: {:.0} units.", d);
        }
    }
    let (top, bottom) = if stats.row_axis == 2 {
        ("top", "bottom")
    } else {
        ("+{axis} side", "-{axis} side")
    };
    println!();
    println!(
        "  Expect up to three parallel rows of {} holes each.",
        offsets.len()
    );
    println!(
        "  The {} row is OUT, the middle row is CTL, the {} row is IN.",
        top.replace("{axis}", row),
        bottom.replace("{axis}", row)
    );
    println!();
    println!("  Count the holes in each row and report three numbers.");
    println!(
        "    CTL must be {}/{}. Anything less means the grid overran an edge or a",
        offsets.len(),
        offsets.len()
    );
    println!("    doorway and the run is void — re-run with a smaller --column-spacing.");
    println!("    Every row's first hole sits ON the surface, so a row with no holes at");
    println!("    all missed the wall vertically and is void rather than a zero result.");
    println!("    The rest ascend by offset, so a row that stops after N holes accepted");
    println!("    up to that column's offset and rejected the next:");
    for (i, o) in offsets.iter().enumerate() {
        if *o == 0.0 {
            println!("      {} hole  -> on the surface only; no offset worked", i + 1);
        } else {
            println!("      {} holes -> accepted up to {:.0} units", i + 1, o);
        }
    }
    println!("      0 holes -> that row missed the wall; run void for it");
    println!();
    println!("  If a hole shows up displaced from its row rather than missing, that still");
    println!("  counts as created — the engine projected it onto whichever surface it");
    println!("  found first. Say so and count it.");
}
