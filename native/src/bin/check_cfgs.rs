//! What the game's own config files set behind the pipeline's back.
//!
//! ```text
//! cargo run --release -p native --bin check_cfgs -- <dod-folder>
//! ```
//!
//! Read-only. This never writes, edits or removes a config file.

use native::patch::cfg_scan;

fn main() {
    let dir = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: check_cfgs <dod-folder>"),
    );

    let scan = cfg_scan::scan(&dir);
    println!("{}\n{} config file(s) executed\n", dir.display(), scan.files_read);

    let effective = scan.effective_settings();
    if effective.is_empty() {
        println!("Nothing the pipeline depends on is set by an executed config.");
    } else {
        println!("Set by configs the engine runs on its own:");
        for s in &effective {
            println!("  {:<12} {:<8} {}:{}", s.cvar, s.value, s.file_name(), s.line);
        }
    }

    let shadowed: Vec<_> = scan
        .settings
        .iter()
        .filter(|s| s.auto_executed && !effective.iter().any(|e| std::ptr::eq(*e, *s)))
        .collect();
    if !shadowed.is_empty() {
        println!("\nAlso set, but overridden later:");
        for s in shadowed {
            println!("  {:<12} {:<8} {}:{}", s.cvar, s.value, s.file_name(), s.line);
        }
    }

    if !scan.unreferenced.is_empty() {
        println!("\nConfigs present that nothing execs (they set nothing today):");
        for p in &scan.unreferenced {
            println!("  {}", p.file_name().unwrap_or_default().to_string_lossy());
        }
    }
}
