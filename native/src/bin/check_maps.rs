//! Which demos in a folder can actually be played, and on the right map build.
//!
//! ```text
//! cargo run --release -p native --bin check_maps -- <demo-or-folder> [maps-dir]
//! ```
//!
//! `maps-dir` defaults to `<folder>/maps`, because demos live inside the `dod`
//! folder and the maps sit beside them.

use native::patch::map_check::{check_demo, MapStatus};

fn main() {
    let mut args = std::env::args().skip(1);
    let target = std::path::PathBuf::from(
        args.next()
            .expect("usage: check_maps <demo-or-folder> [maps-dir]"),
    );

    let demos: Vec<std::path::PathBuf> = if target.is_dir() {
        let mut found: Vec<_> = std::fs::read_dir(&target)
            .expect("read folder")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dem")))
            .collect();
        found.sort();
        found
    } else {
        vec![target.clone()]
    };

    let maps_dir = args.next().map(std::path::PathBuf::from).unwrap_or_else(|| {
        let base = if target.is_dir() {
            target.clone()
        } else {
            target.parent().map(|p| p.to_path_buf()).unwrap_or_default()
        };
        base.join("maps")
    });

    println!("maps: {}", maps_dir.display());
    println!("demos: {}\n", demos.len());

    let (mut ok, mut wrong, mut missing, mut unverifiable, mut unreadable, mut failed) =
        (0, 0, 0, 0, 0, 0);
    let mut needed: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();

    for demo in &demos {
        let name = demo.file_name().unwrap_or_default().to_string_lossy();
        match check_demo(demo, &maps_dir) {
            Ok((reference, status)) => {
                match &status {
                    MapStatus::Ok { .. } => ok += 1,
                    MapStatus::WrongBuild { .. } => {
                        wrong += 1;
                        *needed.entry(reference.map_name.clone()).or_default() += 1;
                    }
                    MapStatus::Missing => {
                        missing += 1;
                        *needed.entry(reference.map_name.clone()).or_default() += 1;
                    }
                    MapStatus::Unverifiable => unverifiable += 1,
                    MapStatus::Unreadable { .. } => unreadable += 1,
                }
                if !matches!(status, MapStatus::Ok { .. }) {
                    println!("{:<52} {}", name, status.summary(&reference.map_name));
                }
            }
            Err(e) => {
                failed += 1;
                println!("{:<52} unreadable demo: {}", name, e);
            }
        }
    }

    println!(
        "\nok {}  wrong-build {}  missing {}  unverifiable {}  unreadable-map {}  unreadable-demo {}",
        ok, wrong, missing, unverifiable, unreadable, failed
    );
    if !needed.is_empty() {
        println!("\nmaps to fetch:");
        for (map, count) in &needed {
            println!("  {:<24} wanted by {} demo(s)", map, count);
        }
    }
}
