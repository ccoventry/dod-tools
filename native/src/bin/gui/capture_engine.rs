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
            let total = jobs.len();
            if tx.send(EngineEvent::Starting(total)).is_err() {
                return;
            }

            let dod_dir = match hl_path.parent() {
                Some(parent) => parent.join("dod"),
                None => {
                    let _ = tx.send(EngineEvent::Error("Invalid hl.exe path".to_string()));
                    return;
                }
            };

            let dll_path = match hlae_path.parent() {
                Some(parent) => parent.join("AfxHookGoldSrc.dll"),
                None => {
                    let _ = tx.send(EngineEvent::Error("Invalid hlae.exe path".to_string()));
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
                    Some(name) => name,
                    None => {
                        let _ = tx.send(EngineEvent::Error(format!("Invalid demo path: {:?}", job.patched_demo_path)));
                        continue;
                    }
                };

                let dest_demo_path = dod_dir.join(demo_filename);
                if let Err(e) = std::fs::copy(&job.patched_demo_path, &dest_demo_path) {
                    let _ = tx.send(EngineEvent::Error(format!("Failed to copy demo to game folder: {}", e)));
                    continue;
                }

                let demo_name_no_ext = match std::path::Path::new(demo_filename).file_stem() {
                    Some(stem) => stem.to_string_lossy().to_string(),
                    None => demo_filename.to_string_lossy().to_string(),
                };

                if tx.send(EngineEvent::Launching(demo_name_no_ext.clone())).is_err() {
                    let _ = std::fs::remove_file(&dest_demo_path);
                    return;
                }

                let cmd_line_str = format!(
                    "-game dod -insecure -windowed -w 1280 -h 720 +map dod_donner +playdemo {}",
                    demo_name_no_ext
                );

                let required_space = 10_000_000_000; // 10 GB safe estimate
                let valid_export_dir = if let Some(primary) = &config.primary_media_dir {
                    if native::sys::disk::get_available_bytes(primary) > required_space {
                        primary.clone()
                    } else if let Some(backup) = &config.backup_media_dir {
                        backup.clone()
                    } else {
                        primary.clone()
                    }
                } else if let Some(out) = &config.output_dir {
                    out.clone()
                } else {
                    dod_dir.clone()
                };

                let width_str = config.resolution_width.to_string();
                let height_str = config.resolution_height.to_string();
                let active_export_dir_str = valid_export_dir.to_string_lossy().to_string();
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

                let mut child = match cmd.spawn() {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(EngineEvent::Error(format!("Failed to spawn HLAE: {}", e)));
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
                        let _ = tx.send(EngineEvent::Error(format!("Failed to wait for HLAE: {}", e)));
                        let _ = std::fs::remove_file(&dest_demo_path);
                        continue;
                    }
                }

                // TODO: Re-enable temporary demo cleanup once the Phase 7 capture pipeline is fully verified.
                if let Err(e) = std::fs::remove_file(&dest_demo_path) {
                    log::warn!("Failed to delete temporary demo upon success: {}", e);
                }

                // Output Verification
                let expected_wav = job.expected_take_folder.join("sound.wav");
                if expected_wav.exists() && expected_wav.is_file() {
                    let _ = tx.send(EngineEvent::Verified(demo_name_no_ext.clone()));
                } else {
                    let _ = tx.send(EngineEvent::Error(format!(
                        "Verification failed: sound.wav not found in {:?}",
                        job.expected_take_folder
                    )));
                }
            }

            let _ = tx.send(EngineEvent::AllCompleted);
        })
        .unwrap();
}
