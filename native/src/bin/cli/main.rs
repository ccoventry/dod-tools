// native/src/bin/cli/main.rs
// Headless Preview Generator — secondary binary target (`preview_cli`).
//
// Usage: Drag and drop any mix of .dem files and/or folders directly onto this executable.
// For each directory input, scans for .dem files and writes `_preview.dem` copies to a
// `previews/` subdirectory relative to that directory.
// For each individual .dem file input, writes `_preview.dem` to a `previews/`
// subdirectory relative to that file's parent directory.

fn main() {
    let mut input_paths: Vec<String> = std::env::args().skip(1).collect();
    let mut is_interactive = false;

    if input_paths.is_empty() {
        is_interactive = true;
        println!("No arguments detected.\n[Interactive Mode] Drag and drop .dem files or folders here, then press Enter:");

        let mut buffer = String::new();
        if std::io::stdin().read_line(&mut buffer).is_ok() {
            let trimmed = buffer.trim();
            let mut current = String::new();
            let mut in_quotes = false;

            for ch in trimmed.chars() {
                match ch {
                    '"' => {
                        if in_quotes {
                            let item = current.trim().to_string();
                            if !item.is_empty() {
                                input_paths.push(item);
                            }
                            current.clear();
                            in_quotes = false;
                        } else {
                            in_quotes = true;
                        }
                    }
                    ' ' if !in_quotes => {
                        let item = current.trim().to_string();
                        if !item.is_empty() {
                            input_paths.push(item);
                        }
                        current.clear();
                    }
                    _ => {
                        current.push(ch);
                    }
                }
            }
            let item = current.trim().to_string();
            if !item.is_empty() {
                input_paths.push(item);
            }
        }
    }

    let patcher_config = native::patch::PatcherConfig::default();
    let cancel_token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let mut processed = 0usize;
    let mut skipped = 0usize;

    for input in &input_paths {
        let path = std::path::PathBuf::from(input);

        if path.is_dir() {
            // ── Directory input: scan all .dem files inside ──────────────────
            let output_dir = path.join("previews");
            if let Err(e) = std::fs::create_dir_all(&output_dir) {
                eprintln!("Error: Failed to create output directory {:?}: {}", output_dir, e);
                continue;
            }
            println!("Created output directory: {:?}", output_dir);

            let entries = match std::fs::read_dir(&path) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("Error: Failed to read directory {:?}: {}", path, e);
                    continue;
                }
            };

            for entry in entries.flatten() {
                let file_path = entry.path();
                if !file_path.is_file() {
                    continue;
                }
                let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext.to_lowercase() != "dem" {
                    continue;
                }
                process_demo(&file_path, &output_dir, &patcher_config, &cancel_token, &mut processed, &mut skipped);
            }
        } else if path.is_file() {
            // ── Individual file input ────────────────────────────────────────
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext.to_lowercase() != "dem" {
                eprintln!("Skipped: {:?} — not a .dem file.", path.file_name().unwrap_or_default());
                skipped += 1;
                continue;
            }

            let output_dir = path
                .parent()
                .unwrap_or(std::path::Path::new(""))
                .join("previews");

            if let Err(e) = std::fs::create_dir_all(&output_dir) {
                eprintln!("Error: Failed to create output directory {:?}: {}", output_dir, e);
                skipped += 1;
                continue;
            }
            println!("Created output directory: {:?}", output_dir);

            process_demo(&path, &output_dir, &patcher_config, &cancel_token, &mut processed, &mut skipped);
        } else {
            eprintln!("Skipped: {:?} — path does not exist or is not accessible.", path);
            skipped += 1;
        }
    }

    println!(
        "\n[dod-tools] Batch complete. Previews are ready.\n  Processed: {} demo(s)  |  Skipped: {} demo(s)",
        processed, skipped
    );

    if is_interactive {
        println!("\nPress Enter to exit...");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
}

fn process_demo(
    path: &std::path::Path,
    output_dir: &std::path::Path,
    patcher_config: &native::patch::PatcherConfig,
    cancel_token: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    processed: &mut usize,
    skipped: &mut usize,
) {
    let original_filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    println!("Processing: {}", original_filename);

    let (_tickrate, mut streaks, is_pov, local_player_idx, _playback_frames, _match_start, _frame_times) =
        match native::patch::scan_demo_for_highlights(path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("  Skipped: {} — scan error: {}", original_filename, e);
                *skipped += 1;
                return;
            }
        };

    if is_pov {
        streaks.retain(|s| Some(s.player_index) == local_player_idx);
    }

    if streaks.is_empty() {
        println!("  Skipped: {} — no highlights found.", original_filename);
        *skipped += 1;
        return;
    }

    let jobs = native::patch::build_preview_patch_jobs(streaks, Some(output_dir));

    for job in &jobs {
        let new_filename = job.output_demo
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let patcher = native::patch::StreamPatcher::new(&job.source_demo, &job.output_demo);
        match patcher.patch(job, patcher_config, cancel_token) {
            Ok(()) => {
                println!("  Saved: {}", new_filename);
                *processed += 1;
            }
            Err(e) => {
                eprintln!("  Error writing {}: {}", new_filename, e);
                *skipped += 1;
            }
        }
    }
}
