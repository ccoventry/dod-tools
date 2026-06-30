use std::path::PathBuf;
use std::sync::{Arc, mpsc::Sender, atomic::{AtomicBool, Ordering}};
use crate::types::{CaptureJob, EngineEvent};
use native::log_markdown;

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
                        let _ = (|| -> Result<(), Box<dyn std::error::Error>> {
                            let exe_path = std::env::current_exe()?;
                            let exe_dir = exe_path.parent().ok_or("Failed to get exe parent")?;
                            let local_dir = exe_dir.join("local");
                            std::fs::create_dir_all(&local_dir)?;
                            let log_path = local_dir.join("crash_log.md");
                            let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&log_path)?;
                            writeln!(file, "{}", $msg)?;
                            Ok(())
                        })();
                        let _ = $tx.send(EngineEvent::Error("Capture Engine Aborted - Check local/crash_log.md".to_string()));
                    }
                };
            }

            let total = jobs.len();
            if tx.send(EngineEvent::Starting(total)).is_err() {
                return;
            }

            let hl_exe_parent = match hl_path.parent() {
                Some(parent) => parent,
                None => {
                    log_crash_abort!(tx, "Invalid hl.exe path: hl_path has no parent");
                    return;
                }
            };
            let dod_dir = hl_exe_parent.join("dod");

            let dll_path = match hlae_path.parent() {
                Some(parent) => parent.join("AfxHookGoldSrc.dll"),
                None => {
                    log_crash_abort!(tx, "Invalid hlae.exe path: hlae_path has no parent");
                    return;
                }
            };

            let mut active_dest_paths = Vec::new();
            let dummy_path = hl_exe_parent.join("DOD_BATCH_DONE");
            std::fs::remove_dir_all(&dummy_path).ok();

            for job in jobs {
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

                        log_markdown(&format!("- [IO] Copying (Windows) from {}", source_path_str));
                        log_markdown(&format!("- [IO] Copying (Windows) to {}", dest_path_str));
                        match std::io::copy(&mut src_file, &mut dest_file) {
                            Ok(bytes) => log_markdown(&format!("- [IO] Copy SUCCESS! Bytes written: {}", bytes)),
                            Err(e) => {
                                log_markdown(&format!("- [IO] Copy FAILED! Error: {}", e));
                                log_crash_abort!(tx, format!("Failed to copy demo to game folder: {}", e));
                                continue;
                            }
                        }
                    }

                    #[cfg(not(target_os = "windows"))]
                    {
                        log_markdown(&format!("- [IO] Copying (*nix) from {}", source_path_str));
                        log_markdown(&format!("- [IO] Copying (*nix) to {}", dest_path_str));
                        match std::fs::copy(&job.patched_demo_path, &dest_demo_path) {
                            Ok(bytes) => log_markdown(&format!("- [IO] Copy SUCCESS! Bytes written: {}", bytes)),
                            Err(e) => {
                                log_markdown(&format!("- [IO] Copy FAILED! Error: {}", e));
                                log_crash_abort!(tx, format!("Failed to copy demo to game folder: {}", e));
                                continue;
                            }
                        }
                    }
                }

                if config.save_local_patched_copy {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let _ = std::fs::create_dir_all("demos");
                        let local_dest = std::path::Path::new("demos").join(&demo_filename);
                        match std::fs::copy(&job.patched_demo_path, &local_dest) {
                            Ok(_) => log_markdown(&format!("- [IO] Saved local copy to demos/{}", demo_filename)),
                            Err(e) => log::warn!("Failed to save local patched copy to {:?}: {}", local_dest, e),
                        }
                    }
                }

                active_dest_paths.push(dest_demo_path);
            }

            if active_dest_paths.is_empty() {
                let _ = tx.send(EngineEvent::AllCompleted);
                return;
            }

            if tx.send(EngineEvent::Launching("Batch Queue".into())).is_err() {
                log_crash_abort!(tx, "Failed to send Launching event (channel disconnected)");
                #[cfg(not(debug_assertions))]
                for path in &active_dest_paths {
                    let _ = std::fs::remove_file(path);
                }
                return;
            }

            let active_export_dir = config.primary_media_dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

            #[cfg(not(target_arch = "wasm32"))]
            {
                use sysinfo::{System, SystemExt, DiskExt};
                let mut sys = System::new_all();
                sys.refresh_disks_list();
                
                let mut available_space = u64::MAX;
                let mut disk_found = false;
                for disk in sys.disks() {
                    if active_export_dir.starts_with(disk.mount_point()) {
                        available_space = disk.available_space();
                        disk_found = true;
                        break;
                    }
                }

                if disk_found && available_space < 15_u64 * 1024 * 1024 * 1024 {
                    log_crash_abort!(tx, "Capture aborted: Target drive has less than 15GB free space.");
                    return;
                }
            }

            let width_str = config.resolution_width.to_string();
            let height_str = config.resolution_height.to_string();

            let cmd_line_str = format!(
                "-game dod -insecure -windowed -w {} -h {} +playdemo primer",
                width_str, height_str
            );

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
            ]);

            if !config.movie_config.trim().is_empty() {
                let mut cfg_name = config.movie_config.trim().to_string();
                if cfg_name.ends_with(".cfg") {
                    cfg_name.truncate(cfg_name.len() - 4);
                }
                cmd.arg("+exec");
                cmd.arg(format!("{}.cfg", cfg_name));
            }
            cmd.env("SteamAppId", "30");

            if let Some(parent) = hlae_path.parent() {
                cmd.current_dir(parent);
            }

            let cfg_path = dod_dir.join("dod_quit.cfg");
            std::fs::write(&cfg_path, "quit\n").ok();

            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    log_crash_abort!(tx, format!("Failed to spawn HLAE (OS Error): {}", e));
                    #[cfg(not(debug_assertions))]
                    for path in &active_dest_paths {
                        let _ = std::fs::remove_file(path);
                    }
                    std::fs::remove_file(&cfg_path).ok();
                    return;
                }
            };

            let mut last_log_check = std::time::Instant::now();
            while let Ok(None) = child.try_wait() {
                if cancel_token.load(Ordering::Relaxed) {
                    let _ = child.kill();
                    break;
                }
                if last_log_check.elapsed().as_millis() > 1000 {
                    if dummy_path.exists() {
                        let _ = child.kill();
                        break;
                    }
                    last_log_check = std::time::Instant::now();
                }
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
            let exit_status = child.wait(); // reap

            if cancel_token.load(Ordering::Relaxed) {
                #[cfg(not(debug_assertions))]
                for path in &active_dest_paths {
                    let _ = std::fs::remove_file(path);
                }
                std::fs::remove_file(&cfg_path).ok();
                std::fs::remove_dir_all(&dummy_path).ok();
                let _ = tx.send(EngineEvent::Cancelled);
                return;
            }

            std::fs::remove_file(&cfg_path).ok();
            std::fs::remove_dir_all(&dummy_path).ok();

            match exit_status {
                Ok(_) => {
                    let _ = tx.send(EngineEvent::Finished("Batch Queue".into()));
                }
                Err(e) => {
                    log_crash_abort!(tx, format!("Failed to wait for HLAE: {}", e));
                }
            }

            // TODO: Re-enable temporary demo cleanup once the Phase 7 capture pipeline is fully verified.
            // #[cfg(not(debug_assertions))]
            // {
            //     if !engine_aborted {
            //         for path in active_dest_paths {
            //             if let Err(e) = std::fs::remove_file(&path) {
            //                 log::warn!("Failed to delete temporary demo upon success: {}", e);
            //             }
            //         }
            //     }
            // }

            let _ = tx.send(EngineEvent::AllCompleted);
        })
        .unwrap();
}
