use std::path::PathBuf;
use std::sync::{Arc, mpsc::Sender, atomic::{AtomicBool, Ordering}};
use crate::types::{CaptureJob, EngineEvent};

/// Poll interval for the child-process watch loop (16 ms ≈ 60 FPS cadence).
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);

pub fn spawn_capture_engine(
    jobs: Vec<CaptureJob>,
    hlae_path: Arc<PathBuf>,
    hl_path: Arc<PathBuf>,
    tx: Sender<EngineEvent>,
    cancel_token: Arc<AtomicBool>,
    config: native::patch::PatcherConfig,
) {
    std::thread::Builder::new()
        .name("capture_engine".into())
        .spawn(move || {
            macro_rules! log_crash_abort {
                ($tx:expr, $msg:expr) => {
                    {
                        log::error!("{}", $msg);
                        use std::io::Write;
                        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open("local/crash_log.md") {
                            let _ = writeln!(file, "{}", $msg);
                        }
                        let _ = $tx.send(EngineEvent::Error("Capture Engine Aborted - Check crash_log.md".to_string()));
                    }
                };
            }

            let total = jobs.len();
            if tx.send(EngineEvent::Starting(total)).is_err() {
                return;
            }

            let dod_dir = match hl_path.parent() {
                Some(parent) => parent.join("dod"),
                None => {
                    log_crash_abort!(tx, "Invalid hl.exe path: hl_path has no parent");
                    return;
                }
            };

            let dll_path = match hlae_path.parent() {
                Some(parent) => parent.join("AfxHookGoldSrc.dll"),
                None => {
                    log_crash_abort!(tx, "Invalid hlae.exe path: hlae_path has no parent");
                    return;
                }
            };

            for job in jobs {
                // ── Cancellation check before each new job ─────────────────────
                if cancel_token.load(Ordering::Relaxed) {
                    let _ = tx.send(EngineEvent::Cancelled);
                    return;
                }

                let demo_filename = match job.patched_demo_path.file_name() {
                    Some(name) => name.to_string_lossy().replace("-", "_"),
                    None => {
                        log_crash_abort!(tx, format!("Invalid demo path: {:?}", job.patched_demo_path));
                        continue;
                    }
                };

                let dest_demo_path = dod_dir.join(&demo_filename);
                let source_path_str = job.patched_demo_path.to_string_lossy().to_lowercase();
                let dest_path_str = dest_demo_path.to_string_lossy().to_lowercase();

                if source_path_str != dest_path_str {
                    #[cfg(target_os = "windows")]
                    {
                        use std::os::windows::fs::OpenOptionsExt;
                        let mut src_file = match std::fs::OpenOptions::new()
                            .read(true)
                            .share_mode(1) // FILE_SHARE_READ
                            .open(&job.patched_demo_path) {
                                Ok(f) => f,
                                Err(e) => {
                                    log_crash_abort!(tx, format!("Failed to open source demo for copy: {}", e));
                                    continue;
                                }
                            };

                        let mut dest_file_opt = None;
                        for i in 0..5 {
                            match std::fs::File::create(&dest_demo_path) {
                                Ok(f) => {
                                    dest_file_opt = Some(f);
                                    break;
                                }
                                Err(e) => {
                                    if e.raw_os_error() == Some(32) && i < 4 {
                                        std::thread::sleep(std::time::Duration::from_millis(150));
                                        continue;
                                    }
                                    break;
                                }
                            }
                        }

                        let mut dest_file = match dest_file_opt {
                            Some(f) => f,
                            None => {
                                log_crash_abort!(tx, format!("Failed to create dest demo file after retries. Source: {:?}, Dest: {:?}", job.patched_demo_path, dest_demo_path));
                                continue;
                            }
                        };

                        println!(">>> [DEBUG] Copying (Windows) from {}", source_path_str);
                        println!(">>> [DEBUG] Copying (Windows) to {}", dest_path_str);
                        match std::io::copy(&mut src_file, &mut dest_file) {
                            Ok(bytes) => println!(">>> [DEBUG] Copy SUCCESS! Bytes written: {}", bytes),
                            Err(e) => {
                                println!(">>> [DEBUG] Copy FAILED! Error: {}", e);
                                log_crash_abort!(tx, format!("Failed to copy demo to game folder: {}", e));
                                continue;
                            }
                        }
                    }

                    #[cfg(not(target_os = "windows"))]
                    {
                        println!(">>> [DEBUG] Copying (*nix) from {}", source_path_str);
                        println!(">>> [DEBUG] Copying (*nix) to {}", dest_path_str);
                        match std::fs::copy(&job.patched_demo_path, &dest_demo_path) {
                            Ok(bytes) => println!(">>> [DEBUG] Copy SUCCESS! Bytes written: {}", bytes),
                            Err(e) => {
                                println!(">>> [DEBUG] Copy FAILED! Error: {}", e);
                                log_crash_abort!(tx, format!("Failed to copy demo to game folder: {}", e));
                                continue;
                            }
                        }
                    }
                } else {
                    println!(">>> [DEBUG] Skipped copy: source and destination are identical.");
                }

                std::thread::sleep(std::time::Duration::from_millis(300));

                let demo_name_no_ext = match std::path::Path::new(&demo_filename).file_stem() {
                    Some(stem) => stem.to_string_lossy().to_string(),
                    None => demo_filename.clone(),
                };

                if tx.send(EngineEvent::Launching(demo_name_no_ext.clone())).is_err() {
                    log_crash_abort!(tx, "Failed to send Launching event (channel disconnected)");
                    let _ = std::fs::remove_file(&dest_demo_path);
                    return;
                }

                let cmd_line_str = format!(
                    "-game dod -insecure -windowed -w 1280 -h 720 +playdemo {} +playdemo {}",
                    demo_name_no_ext, demo_name_no_ext
                );

                let required_space = 10_000_000_000; // 10 GB safe estimate
                let valid_export_dir = if let Some(primary) = &config.primary_media_dir {
                    if native::sys::disk::get_available_bytes(primary) > required_space {
                        Some(primary.clone())
                    } else if let Some(backup) = &config.backup_media_dir {
                        if native::sys::disk::get_available_bytes(backup) > required_space {
                            Some(backup.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let active_export_dir = match valid_export_dir {
                    Some(dir) => dir,
                    None => {
                        log_crash_abort!(tx, "Capture aborted: Directories not configured or out of space");
                        return;
                    }
                };

                let width_str = config.resolution_width.to_string();
                let height_str = config.resolution_height.to_string();
                let active_export_dir_str = active_export_dir.to_string_lossy().to_string();
                let separate_hud_str = if config.separate_hud { "1" } else { "0" };

                let mut cmd = std::process::Command::new(hlae_path.as_ref());
                cmd.args(&[
                    "-customLoader",
                    "-noGui",
                    "-autoStart",
                    "-hookDllPath",
                    &dll_path.to_string_lossy(),
                    "-programPath",
                    &hl_path.to_string_lossy(),
                    "-cmdLine",
                    &cmd_line_str,
                    "-w",
                    &width_str,
                    "-h",
                    &height_str,
                    "-forceAlpha",
                    "true",
                    "+mirv_movie_filename",
                    &active_export_dir_str,
                    "+mirv_movie_separate_hud",
                    separate_hud_str,
                ]);
                cmd.env("SteamAppId", "30");

                if let Some(parent) = hlae_path.parent() {
                    cmd.current_dir(parent);
                }

                let mut child = match cmd.spawn() {
                    Ok(c) => c,
                    Err(e) => {
                        log_crash_abort!(tx, format!("Failed to spawn HLAE (OS Error): {}", e));
                        let _ = std::fs::remove_file(&dest_demo_path);
                        continue;
                    }
                };

                // ── Polling wait loop — checks cancel token every POLL_INTERVAL ──
                let exit_status = loop {
                    // Check cancellation first on every tick.
                    if cancel_token.load(Ordering::Relaxed) {
                        // Gracefully kill the child process before bailing.
                        let _ = child.kill();
                        let _ = child.wait(); // reap so we don't leak a zombie
                        if let Err(e) = std::fs::remove_file(&dest_demo_path) {
                            log::warn!("Failed to delete temporary demo upon cancellation: {}", e);
                        }
                        let _ = tx.send(EngineEvent::Cancelled);
                        return;
                    }

                    match child.try_wait() {
                        Ok(Some(status)) => break Ok(status),
                        Ok(None) => {
                            // Child still running — sleep and poll again.
                            std::thread::sleep(POLL_INTERVAL);
                        }
                        Err(e) => break Err(e),
                    }
                };

                match exit_status {
                    Ok(_) => {
                        let _ = tx.send(EngineEvent::Finished(demo_name_no_ext.clone()));
                    }
                    Err(e) => {
                        log_crash_abort!(tx, format!("Failed to wait for HLAE: {}", e));
                        let _ = std::fs::remove_file(&dest_demo_path);
                        continue;
                    }
                }

                // TODO: Re-enable temporary demo cleanup once the Phase 7 capture pipeline is fully verified.
                // if let Err(e) = std::fs::remove_file(&dest_demo_path) {
                //     log::warn!("Failed to delete temporary demo upon success: {}", e);
                // }


            }

            let _ = tx.send(EngineEvent::AllCompleted);
        })
        .unwrap();
}
