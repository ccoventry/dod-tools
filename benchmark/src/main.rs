//! Developer benchmarking and parser validation tool.
//! Parses `.dem` files using both unoptimized and optimized pipelines,
//! compares the parsed states to assert correctness, and measures speedups.

use analysis::Analysis;
use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tabled::{builder::Builder, settings::Style};

#[derive(Parser, Debug)]
#[command(version, about = "Developer utility to profile and verify Day of Defeat demo parser performance.")]
struct Args {
    /// Folder paths to scan for .dem files to benchmark
    paths: Vec<PathBuf>,

    /// Limit the number of parsed demos for benchmark (default 50 to complete fast, pass 99999 for all)
    #[arg(long, default_value_t = 50)]
    limit: usize,
}

struct DemoResult {
    path: PathBuf,
    map_name: String,
    size_mb: f64,
    total_frames: usize,
    live_frame: Option<usize>,
    skipped_pct: f64,
    unopt_time: Duration,
    opt_time: Duration,
    speedup: f64,
    mismatch: Option<String>,
}

fn main() {
    let args = Args::parse();
    if args.paths.is_empty() {
        eprintln!("Usage: dod-benchmark <folder1> <folder2> ...");
        std::process::exit(1);
    }

    println!("Scanning directories...");
    let mut files = vec![];
    for folder in &args.paths {
        if folder.exists() {
            scan_dir(folder, &mut files);
        } else {
            eprintln!("Warning: Path does not exist: {}", folder.display());
        }
    }

    println!("Found {} total .dem files.", files.len());
    
    // Sort files alphabetically so traversal is deterministic
    files.sort();

    let limit = args.limit;
    if files.len() > limit {
        println!("Limiting benchmark evaluation to the first {} files (use --limit <num> to customize).", limit);
        files.truncate(limit);
    }

    let mut results = vec![];
    let total_files = files.len();

    for (i, path) in files.into_iter().enumerate() {
        println!("[{}/{}] Parsing: {}", i + 1, total_files, path.display());
        match benchmark_demo(&path) {
            Ok(res) => {
                if let Some(ref err) = res.mismatch {
                    eprintln!("  [MISMATCH] Correctness check failed: {}", err);
                } else {
                    println!(
                        "  -> Map: {}, Size: {:.2} MB, Frames: {}, Skipped: {} ({:.1}%), Speedup: {:.2}x",
                        res.map_name,
                        res.size_mb,
                        res.total_frames,
                        res.live_frame.map_or("None".to_string(), |idx| idx.to_string()),
                        res.skipped_pct,
                        res.speedup
                    );
                }
                results.push(res);
            }
            Err(e) => {
                eprintln!("  [ERROR] Failed to parse: {}", e);
            }
        }
    }

    print_report(&results);
}

fn scan_dir(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_dir(&path, files);
            } else if path.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("dem")) {
                files.push(path);
            }
        }
    }
}

fn benchmark_demo(path: &Path) -> Result<DemoResult, String> {
    let file_bytes = fs::read(path).map_err(|e| format!("Read error: {}", e))?;
    let size_mb = file_bytes.len() as f64 / (1024.0 * 1024.0);

    let (analysis, diag) = Analysis::parse_with_diagnostics(&file_bytes)?;
    
    let skipped_pct = if let Some(live_frame) = diag.live_frame_index {
        (live_frame as f64 / diag.total_frames as f64) * 100.0
    } else {
        0.0
    };

    let speedup = diag.unopt_duration.as_secs_f64() / diag.opt_duration.as_secs_f64();

    Ok(DemoResult {
        path: path.to_path_buf(),
        map_name: analysis.demo_info.map_name,
        size_mb,
        total_frames: diag.total_frames,
        live_frame: diag.live_frame_index,
        skipped_pct,
        unopt_time: diag.unopt_duration,
        opt_time: diag.opt_duration,
        speedup,
        mismatch: if diag.states_matched { None } else { diag.mismatch_reason },
    })
}

fn print_report(results: &[DemoResult]) {
    if results.is_empty() {
        println!("No successful runs to report.");
        return;
    }

    let mut table_builder = Builder::default();
    table_builder.push_record([
        "File",
        "Map",
        "Size (MB)",
        "Frames",
        "Live Frame",
        "Skip %",
        "Unopt (ms)",
        "Opt (ms)",
        "Speedup",
        "Correct?",
    ]);

    let mut total_unopt = Duration::ZERO;
    let mut total_opt = Duration::ZERO;
    let mut total_size = 0.0;
    let mut total_frames = 0;
    let mut mismatch_count = 0;

    for res in results {
        let file_name = res.path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
        let live_frame_str = res.live_frame.map_or("None".to_string(), |idx| idx.to_string());
        
        table_builder.push_record([
            file_name.to_string(),
            res.map_name.clone(),
            format!("{:.1}", res.size_mb),
            res.total_frames.to_string(),
            live_frame_str,
            format!("{:.1}%", res.skipped_pct),
            format!("{:.1}", res.unopt_time.as_secs_f64() * 1000.0),
            format!("{:.1}", res.opt_time.as_secs_f64() * 1000.0),
            format!("{:.2}x", res.speedup),
            match &res.mismatch {
                None => "YES".to_string(),
                Some(err) => {
                    mismatch_count += 1;
                    format!("NO: {}", err)
                }
            },
        ]);

        total_unopt += res.unopt_time;
        total_opt += res.opt_time;
        total_size += res.size_mb;
        total_frames += res.total_frames;
    }

    let mut table = table_builder.build();
    table.with(Style::markdown());

    println!("\n### Parsing Benchmarks & Correctness Report\n");
    println!("{}", table);
    println!("\n### Summary Statistics\n");
    println!("* **Total unique demos processed**: {}", results.len());
    println!("* **Total size processed**: {:.2} MB", total_size);
    println!("* **Total frames processed**: {}", total_frames);
    println!("* **Total unoptimized time**: {:.3} seconds", total_unopt.as_secs_f64());
    println!("* **Total optimized time**: {:.3} seconds", total_opt.as_secs_f64());
    println!("* **Overall average speedup**: {:.2}x", total_unopt.as_secs_f64() / total_opt.as_secs_f64());
    if mismatch_count > 0 {
        println!("* **WARNING**: {} demo(s) failed the correctness assertion! See log details above.", mismatch_count);
    } else {
        println!("* **Validation**: All parsed states matched perfectly between unoptimized and optimized pipelines!");
    }
}
