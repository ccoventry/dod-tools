use std::path::PathBuf;
use std::sync::{Arc, mpsc::Sender, atomic::{AtomicBool, Ordering}};
use crate::types::{CaptureJob, EngineEvent};
use native::log_markdown;

struct CaptureCleanupGuard {
    exit_trigger: PathBuf,
    session_junction: PathBuf,
    auto_clear_logs: bool,
    auto_clear_temp_demos: bool,
    auto_clear_previews: bool,
    save_local_patched_copy: bool,
    _wake_lock: Option<keepawake::KeepAwake>,
}

impl CaptureCleanupGuard {
    fn new(
        exit_trigger: PathBuf,
        session_junction: PathBuf,
        auto_clear_logs: bool,
        auto_clear_temp_demos: bool,
        auto_clear_previews: bool,
        save_local_patched_copy: bool,
    ) -> Self {
        // Pre-clean any stale signal dirs/junctions from a previous aborted run.
        // exit_trigger is a directory signal — never use remove_file on it.
        if let Err(e) = std::fs::remove_dir_all(&exit_trigger) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("[GC::new] Failed to pre-clean exit_trigger {:?}: {}", exit_trigger, e);
            }
        }
        // session_junction is a directory junction link — remove_dir unlinks the
        // junction itself without recursing into the target directory.
        if let Err(e) = std::fs::remove_dir(&session_junction) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("[GC::new] Failed to pre-clean session_junction {:?}: {}", session_junction, e);
            }
        }
        Self {
            exit_trigger,
            session_junction,
            auto_clear_logs,
            auto_clear_temp_demos,
            auto_clear_previews,
            save_local_patched_copy,
            _wake_lock: keepawake::Builder::default()
                .display(false)
                .idle(true)
                .sleep(true)
                .create()
                .ok(),
        }
    }
}

impl Drop for CaptureCleanupGuard {
    fn drop(&mut self) {
        // Signal dirs (DOD_TOOLS_EXIT_TRIGGER) are directories, not files.
        // Use remove_dir_all; silently ignore NotFound, log anything else.
        if let Err(e) = std::fs::remove_dir_all(&self.exit_trigger) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("[GC::drop] Failed to remove exit_trigger {:?}: {}", self.exit_trigger, e);
            }
        }
        // Junction link: remove_dir unlinks without touching the junction target.
        if let Err(e) = std::fs::remove_dir(&self.session_junction) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("[GC::drop] Failed to remove session_junction {:?}: {}", self.session_junction, e);
            }
        }

        if let Some(parent) = self.exit_trigger.parent() {
            let dod_dir = parent.join("dod");
            
            if self.auto_clear_logs {
                let _ = std::fs::remove_file(dod_dir.join("qconsole.log"));
                let _ = std::fs::remove_file(dod_dir.join("dodtools_helper.cfg"));
                let _ = std::fs::remove_file(dod_dir.join("dodtools_capture_done.cfg"));
                let _ = std::fs::remove_file(dod_dir.join("dod_quit.cfg"));
                if let Ok(entries) = std::fs::read_dir(&dod_dir) {
                    for entry in entries.flatten() {
                        let filename = entry.file_name().to_string_lossy().to_string();
                        if filename.starts_with("dodtools_chain_") && filename.ends_with(".cfg") {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    }
                }
            }
            
            if self.auto_clear_temp_demos && !self.save_local_patched_copy {
                let _ = std::fs::remove_file(dod_dir.join("primer.dem"));
                if let Ok(entries) = std::fs::read_dir(&dod_dir) {
                    for entry in entries.flatten() {
                        let filename = entry.file_name().to_string_lossy().to_string();
                        if filename.starts_with("dodtools_chain_") && filename.ends_with(".dem") {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    }
                }
            }

            if self.auto_clear_previews {
                let scan_dirs = vec![dod_dir.clone(), parent.to_path_buf()];
                for scan_dir in scan_dirs {
                    if let Ok(entries) = std::fs::read_dir(scan_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_file() {
                                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                                    if filename.ends_with("_preview.dem") {
                                        let sidecar = path.with_extension("dodtools_preview");
                                        if sidecar.exists() {
                                            let _ = std::fs::remove_file(&path);
                                            let _ = std::fs::remove_file(sidecar);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn spawn_capture_engine(
    jobs: Vec<CaptureJob>,
    _hlae_path: Arc<PathBuf>,
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
                            let macro_docs_session_dir = dirs::document_dir().unwrap().join("dod-tools").join("Capture_Sessions").join(&config.session_id);
                            std::fs::create_dir_all(&macro_docs_session_dir)?;
                            let log_path = macro_docs_session_dir.join("session_log.txt");
                            let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&log_path)?;
                            writeln!(file, "{}", $msg)?;
                            Ok(())
                        })();
                        let _ = $tx.send(EngineEvent::Error("Capture Engine Aborted - Check Capture_Sessions/session_log.txt".to_string()));
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



            let mut active_dest_paths = Vec::new();
            let dummy_path = hl_exe_parent.join("DOD_BATCH_DONE");
            std::fs::remove_dir_all(&dummy_path).ok();

            let exit_trigger = hl_exe_parent.join("DOD_TOOLS_EXIT_TRIGGER");
            let session_junction = hl_exe_parent.join("dodtools_session");
            
            let _cleanup_guard = CaptureCleanupGuard::new(
                exit_trigger.clone(),
                session_junction.clone(),
                config.auto_clear_logs,
                config.auto_clear_temp_demos,
                config.auto_clear_previews,
                config.save_local_patched_copy,
            );

            let active_export_dir = config.primary_media_dir.clone().unwrap_or_else(|| {
                let exe_path = std::env::current_exe().expect("Failed to resolve absolute exe path");
                exe_path.parent().expect("Exe has no parent directory").to_path_buf()
            });
            let session_dir = if !config.session_id.is_empty() {
                active_export_dir.join(&config.session_id)
            } else {
                active_export_dir
            };
            let docs_session_dir = dirs::document_dir().unwrap().join("dod-tools").join("Capture_Sessions").join(&config.session_id);
            let _ = std::fs::create_dir_all(&docs_session_dir);
            let manifest_payload = format!("Session ID: {}\nQueued Demos: {}", config.session_id, jobs.len());
            let _ = std::fs::write(docs_session_dir.join("manifest.txt"), manifest_payload);

            let session_junction_str = session_junction.to_str().unwrap_or_default();
            let session_dir_str = session_dir.to_str().unwrap_or_default();
            if session_junction_str.is_empty() || session_dir_str.is_empty() {
                log_crash_abort!(tx, "Invalid UTF-8 in session paths");
                return;
            }

            match std::process::Command::new("cmd").args(&["/C", "mklink", "/J", session_junction_str, session_dir_str]).output() {
                Ok(out) if !out.status.success() => {
                    log_crash_abort!(tx, format!("mklink failed for session_junction: {}", String::from_utf8_lossy(&out.stderr)));
                    return;
                }
                Err(e) => {
                    log_crash_abort!(tx, format!("mklink command failed: {}", e));
                    return;
                }
                _ => {}
            }

            // ── Pool junction pre-generation ────────────────────────────────────
            // For each capture_directory, create a short NTFS junction named
            // `dod_pool_N` inside the game's working directory (beside dod/).
            // This gives the GoldSrc engine a fixed, short path to write BMPs to
            // without exceeding the 64-byte Cbuf string limit.
            let mut pool_junctions: Vec<std::path::PathBuf> = Vec::new();
            for (idx, target_dir) in config.capture_directories.iter().enumerate() {
                let junction_path = hl_exe_parent.join(format!("dod_pool_{}", idx));
                // Pre-clean any stale junction from a previous run.
                let _ = std::fs::remove_dir(&junction_path);
                // Ensure the target directory exists before linking.
                if let Err(e) = std::fs::create_dir_all(target_dir) {
                    log_crash_abort!(tx, format!("Failed to create capture directory {:?}: {}", target_dir, e));
                    return;
                }
                let junction_str = junction_path.to_str().unwrap_or_default();
                let target_str = target_dir.to_str().unwrap_or_default();
                if junction_str.is_empty() || target_str.is_empty() {
                    log_crash_abort!(tx, "Invalid UTF-8 in pool junction paths");
                    return;
                }
                let status = std::process::Command::new("cmd")
                    .args(&[
                        "/C", "mklink", "/J",
                        junction_str,
                        target_str,
                    ])
                    .output();
                match status {
                    Ok(out) if out.status.success() => {
                        log_markdown(&format!("[pool] Junction created: {:?} -> {:?}", junction_path, target_dir));
                        pool_junctions.push(junction_path);
                    }
                    Ok(out) => {
                        let err_msg = String::from_utf8_lossy(&out.stderr);
                        log_crash_abort!(tx, format!("[pool] mklink failed for dod_pool_{}: {}", idx, err_msg));
                        return;
                    }
                    Err(e) => {
                        log_crash_abort!(tx, format!("[pool] Failed to run mklink for dod_pool_{}: {}", idx, e));
                        return;
                    }
                }
            }

            let _guard = native::patch::WorkspaceGuard {
                session_junction: session_junction.clone(),
                exit_trigger: exit_trigger.clone(),
                pool_junctions: pool_junctions.clone(),
                auto_clear_logs: config.auto_clear_logs,
                auto_clear_temp_demos: config.auto_clear_temp_demos,
                auto_clear_previews: config.auto_clear_previews,
                save_local_patched_copy: config.save_local_patched_copy,
            };

            // ── Autosave lockfile ─────────────────────────────────────────────────
            // Write a session snapshot immediately before the batch executes so
            // that a hard crash or power loss can be detected on the next startup.
            // Uses the same SessionData format as the manual "Export Session" button.
            {
                let queued_demos_arc = crate::views::capture::get_queued_demos();
                let guard = match queued_demos_arc.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                let entries: Vec<crate::session::DemoEntry> = guard.iter().map(|d| {
                    let highlights = d.streaks.iter().map(|s| crate::session::HighlightMetadata {
                        is_selected: s.is_selected,
                        start_kill: s.start_index as i32,
                        end_kill: s.end_index as i32,
                        status: s.status,
                        notes: s.notes.clone(),
                    }).collect();
                    crate::session::DemoEntry {
                        path: d.path.clone(),
                        key: native::utils::demo_hasher::calculate_demo_key(&d.path),
                        highlights,
                    }
                }).collect();
                let session_data = crate::session::SessionData { entries };
                match serde_json::to_string_pretty(&session_data) {
                    Ok(json) => {
                        let autosave_path = native::shared::paths::get_appdata_dir().join(".autosave.json");
                        if let Err(e) = std::fs::write(&autosave_path, &json) {
                            log::warn!("[autosave] Failed to write .autosave.json: {}", e);
                        } else {
                            log::info!("[autosave] Lockfile written (.autosave.json)");
                        }
                    }
                    Err(e) => log::warn!("[autosave] Failed to serialize session: {}", e),
                }
            }

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
                        let is_chain = demo_filename.starts_with("chain_");
                        let local_dest = if is_chain {
                            docs_session_dir.join(&demo_filename)
                        } else {
                            let exe_path = std::env::current_exe().expect("Failed to resolve absolute exe path");
                            let base_dir = exe_path.parent().expect("Exe has no parent directory").to_path_buf();
                            let demos_dir = base_dir.join("demos");
                            let _ = std::fs::create_dir_all(&demos_dir);
                            demos_dir.join(&demo_filename)
                        };
                        match std::fs::copy(&job.patched_demo_path, &local_dest) {
                            Ok(_) => {
                                if is_chain {
                                    log_markdown(&format!("- [IO] Routed chain copy to session folder: {}", demo_filename));
                                } else {
                                    log_markdown(&format!("- [IO] Saved local copy to demos/{}", demo_filename));
                                }
                            }
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

            let active_export_dir = config.primary_media_dir.clone().unwrap_or_else(|| {
                let exe_path = std::env::current_exe().expect("Failed to resolve absolute exe path");
                exe_path.parent().expect("Exe has no parent directory").to_path_buf()
            });

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

            let condebug_flag = if config.add_condebug { "-condebug " } else { "" };
            let extra_args = format!("{}+exec dodtools_helper.cfg +playdemo primer", condebug_flag);

            let primary_dir = config.primary_media_dir.clone().unwrap_or_else(|| {
                let exe_path = std::env::current_exe().expect("Failed to resolve absolute exe path");
                exe_path.parent().expect("Exe has no parent directory").to_path_buf()
            });
            let dummy_path = primary_dir.join("DOD_BATCH_DONE");
            let _ = std::fs::remove_dir_all(&dummy_path);

            let mut cmd = config.build_hlae_process(&extra_args);

            let width_str = config.resolution_width.to_string();
            let height_str = config.resolution_height.to_string();
            cmd.args([
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

            let cfg_path = dod_dir.join("dod_quit.cfg");
            std::fs::write(&cfg_path, "quit\n").ok();

            let _child = match cmd.spawn() {
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

            let start_time = std::time::Instant::now();
            loop {
                if cancel_token.load(Ordering::Relaxed) || (start_time.elapsed().as_secs() > 10 && (dummy_path.exists() || exit_trigger.exists())) {
                    std::process::Command::new("taskkill").args(&["/F", "/IM", "hl.exe"]).output().ok();
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }

            if cancel_token.load(Ordering::Relaxed) {
                #[cfg(not(debug_assertions))]
                for path in &active_dest_paths {
                    let _ = std::fs::remove_file(path);
                }
                std::fs::remove_file(&cfg_path).ok();
                std::fs::remove_dir_all(&dummy_path).ok();
                std::fs::remove_dir_all(&exit_trigger).ok();
                let _ = std::fs::remove_dir(&session_junction);
                for junction in &pool_junctions {
                    let _ = std::fs::remove_dir(junction);
                }
                if let Ok(mut entries) = std::fs::read_dir(&session_dir) {
                    if entries.next().is_none() {
                        let _ = std::fs::remove_dir(&session_dir);
                        let _ = std::fs::remove_dir_all(&docs_session_dir);
                    }
                }
                let _ = tx.send(EngineEvent::Cancelled);
                return;
            }

            std::fs::remove_file(&cfg_path).ok();
            std::fs::remove_dir_all(&dummy_path).ok();
            std::fs::remove_dir_all(&exit_trigger).ok();
            let _ = std::fs::remove_dir(&session_junction);
            for junction in &pool_junctions {
                let _ = std::fs::remove_dir(junction);
            }

            let _ = tx.send(EngineEvent::Finished("Batch Queue".into()));

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

            // Remove the autosave lockfile on a clean completion so that the
            // recovery modal is not shown on the next startup.
            let autosave_path = native::shared::paths::get_appdata_dir().join(".autosave.json");
            if let Err(e) = std::fs::remove_file(&autosave_path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("[autosave] Failed to remove .autosave.json: {}", e);
                }
            } else {
                log::info!("[autosave] Lockfile removed after clean completion");
            }
            if let Ok(mut entries) = std::fs::read_dir(&session_dir) {
                if entries.next().is_none() {
                    let _ = std::fs::remove_dir(&session_dir);
                    let _ = std::fs::remove_dir_all(&docs_session_dir);
                }
            }

            let _ = tx.send(EngineEvent::AllCompleted);
        })
        .unwrap();
}
