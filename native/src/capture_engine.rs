use std::path::PathBuf;
use std::sync::{Arc, mpsc::Sender, atomic::{AtomicBool, Ordering}};
use crate::log_markdown;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CaptureJob {
    pub patched_demo_path: std::path::PathBuf,
}

#[derive(Clone, Debug)]
pub enum EngineEvent {
    Starting(usize),
    Launching(String),
    Finished(String),
    Error(String),
    AllCompleted,
    /// Posted when the cancellation token is raised mid-batch.
    /// Signals the GUI to reset the running flag and show a cancelled message.
    Cancelled,
}

/// Longest a batch may go without a console marker before OBS mode calls it
/// stalled.
///
/// Every other guard in the capture loop keys off hl.exe being gone or a file
/// appearing, so none of them notices the engine being alive and well but no
/// longer playing the demo: `disconnect` typed into the console, a demo that
/// failed to load, a frozen engine. On the frame-sequence path that wastes
/// time. In OBS mode it records until the disk fills, which is why the
/// watchdog lives here.
///
/// The floor has to clear the longest *legitimate* silence, which is one
/// breadcrumb interval of real-time playback. That is not a fixed number of
/// seconds — demo ticks are frames, so their wall-clock spacing depends on the
/// fps the demo was recorded at. And an unfocused game stops fast-forwarding
/// altogether (GoldSrc throttles without focus), so alt-tabbing stretches gaps
/// that are normally seconds into minutes. Five minutes clears both.
///
/// Tripping this kills the game and loses the batch, so it is deliberately
/// biased towards waiting too long over firing on a slow one.
const MARKER_STALL_FLOOR: std::time::Duration = std::time::Duration::from_secs(300);

/// How long to wait for the next marker, given the longest gap this batch has
/// already shown.
///
/// The adaptive term covers a demo whose breadcrumbs really are further apart
/// than the floor allows: a batch that has already demonstrated a four-minute
/// gap is not stalled at five.
fn marker_stall_deadline(longest_gap: std::time::Duration) -> std::time::Duration {
    std::cmp::max(MARKER_STALL_FLOOR, longest_gap * 3)
}

struct CaptureCleanupGuard {
    exit_trigger: PathBuf,
    session_junction: PathBuf,
    auto_clear_logs: bool,
    auto_clear_temp_demos: bool,
    auto_clear_previews: bool,
    save_local_patched_copy: bool,
    /// OBS's connection, when this batch is driving one.
    ///
    /// Here rather than in the batch loop because this guard is the only thing
    /// that runs on every exit path the process survives — normal completion,
    /// cancel, a mid-batch crash of the game, an early return. Anywhere else
    /// and a cancelled batch leaves OBS recording indefinitely, filling the
    /// user's drive with no indication that anything is wrong.
    ///
    /// Not a panic, though: release builds are `panic = "abort"`, so nothing
    /// unwinds and no `Drop` runs. That gap, a hard kill and a power cut are
    /// all covered by `obs::recover` on the next start instead.
    obs: Option<std::sync::Arc<std::sync::Mutex<Option<crate::obs::ObsClient>>>>,
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
        if let Err(e) = std::fs::remove_dir_all(&exit_trigger) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("[GC::new] Failed to pre-clean exit_trigger {:?}: {}", exit_trigger, e);
            }
        }
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
            obs: None,
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
        // First, before anything else can fail: OBS must not be left
        // recording. Best-effort and silent by design — there is nobody left
        // to report to, and a cleanup path that panics is worse than one that
        // quietly does nothing.
        if let Some(obs) = self.obs.take() {
            if let Ok(mut guard) = obs.lock() {
                if let Some(client) = guard.as_mut() {
                    client.stop_record_quietly();
                }
                *guard = None;
            }
        }

        if let Err(e) = std::fs::remove_dir_all(&self.exit_trigger) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("[GC::drop] Failed to remove exit_trigger {:?}: {}", self.exit_trigger, e);
            }
        }
        if let Err(e) = std::fs::remove_dir(&self.session_junction) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("[GC::drop] Failed to remove session_junction {:?}: {}", self.session_junction, e);
            }
        }

        if let Some(parent) = self.exit_trigger.parent() {
            let dod_dir = parent.join("dod");
            
            if self.auto_clear_logs {
                crate::shared::paths::remove_console_log(parent);
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
    config: crate::patch::PatcherConfig,
    // Final per-drive headroom `build_batch_queue` already computed for every
    // drive this batch touches (see `native/src/patch/builder.rs`) — the
    // pre-launch check below re-validates these same numbers instead of
    // re-querying disk space itself for just the primary export dir.
    drive_headroom: Vec<(PathBuf, u64)>,
    // Each planned block's take folder, in the order the batch will record
    // them — the same flattened order the capture manifest uses.
    //
    // Only read in `CaptureMode::Obs`, where the engine has to know where a
    // block's output belongs *before* the recording starts, so OBS can be
    // pointed straight at it. In every other mode HLAE decides that from
    // `mirv_movie_filename` and this is empty.
    obs_take_folders: Vec<PathBuf>,
) {
    std::thread::Builder::new()
        .name("capture_engine".into())
        .spawn(move || {
            macro_rules! log_crash_abort {
                ($tx:expr, $msg:expr) => {
                    {
                        log::error!("{}", $msg);
                        crate::log_markdown(&$msg);
                        let _ = $tx.send(EngineEvent::Error(
                            format!("Capture Engine Aborted — {} (see View Logs for details)", $msg)
                        ));
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
            
            let mut _cleanup_guard = CaptureCleanupGuard::new(
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
                active_export_dir.clone()
            };

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

            let mut pool_junctions: Vec<std::path::PathBuf> = Vec::new();
            // `build_batch_queue` creates a `_route_N` junction beside hl.exe for
            // every drive it routes blocks to. It has no way to clean them up —
            // it returns long before the capture ends — so they are listed here
            // by the same indices and unlinked with everything else. Indices this
            // batch never routed to simply are not there, which `remove_dir`
            // reports as NotFound and the guard ignores.
            let route_junctions: Vec<std::path::PathBuf> = (0..config.capture_directories.len())
                .map(|idx| hl_exe_parent.join(format!("_route_{}", idx)))
                .collect();
            for (idx, target_dir) in config.capture_directories.iter().enumerate() {
                let junction_path = hl_exe_parent.join(format!("dod_pool_{}", idx));
                let _ = std::fs::remove_dir(&junction_path);
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

            let _guard = crate::patch::WorkspaceGuard {
                session_junction: session_junction.clone(),
                exit_trigger: exit_trigger.clone(),
                pool_junctions: pool_junctions.clone(),
                route_junctions: route_junctions.clone(),
                auto_clear_logs: config.auto_clear_logs,
                auto_clear_temp_demos: config.auto_clear_temp_demos,
                auto_clear_previews: config.auto_clear_previews,
                save_local_patched_copy: config.save_local_patched_copy,
            };

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
                            .share_mode(1)
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
                    let exe_path = std::env::current_exe().expect("Failed to resolve absolute exe path");
                    let base_dir = exe_path.parent().expect("Exe has no parent directory").to_path_buf();
                    let demos_dir = base_dir.join("demos");
                    let _ = std::fs::create_dir_all(&demos_dir);
                    let local_dest = demos_dir.join(&demo_filename);
                    match std::fs::copy(&job.patched_demo_path, &local_dest) {
                        Ok(_) => log_markdown(&format!("- [IO] Saved local copy to demos/{}", demo_filename)),
                        Err(e) => log::warn!("Failed to save local patched copy to {:?}: {}", local_dest, e),
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
                for path in &active_dest_paths {
                    let _ = std::fs::remove_file(path);
                }
                return;
            }

            for (drive_path, free_bytes) in &drive_headroom {
                if *free_bytes < crate::sys::disk::MIN_DRIVE_HEADROOM_BYTES {
                    let required_gb = crate::sys::disk::MIN_DRIVE_HEADROOM_BYTES as f64 / (1024.0 * 1024.0 * 1024.0);
                    log_crash_abort!(tx, format!(
                        "Capture aborted: {:?} has less than {:.1} GB free space.",
                        drive_path, required_gb
                    ));
                    return;
                }
            }

            // ── OBS capture mode ──────────────────────────────────────────────
            // Everything here happens *before* the game is spawned, on purpose.
            // Every failure below otherwise produces a batch that runs to
            // completion and captures nothing — the worst outcome this path has,
            // because it looks exactly like success until someone opens the
            // folder.
            let obs_mode = config.capture_mode == crate::patch::CaptureMode::Obs;
            let mut obs_session: Option<crate::obs::ObsSession> = None;
            let (marker_tx, marker_rx) = std::sync::mpsc::channel::<crate::obs::Marker>();
            let tail_cancel = Arc::new(AtomicBool::new(false));

            if obs_mode {
                if !config.add_condebug {
                    log_crash_abort!(
                        tx,
                        "OBS capture needs the engine's console log, which requires -condebug. \
                         Enable \"Add condebug\" in settings, or choose another capture mode."
                    );
                    return;
                }
                if obs_take_folders.is_empty() {
                    log_crash_abort!(tx, "OBS capture was requested but no blocks were planned");
                    return;
                }
                match crate::obs::ObsSession::start(
                    &config.obs,
                    obs_take_folders.clone(),
                    config.resolution_width,
                    config.resolution_height,
                ) {
                    Ok((session, preflight)) => {
                        log_markdown(&format!(
                            "🎥 **OBS capture** — connected to OBS {} (obs-websocket {}) at {}. \
                             Canvas {}x{} @ {:.0}fps, scene `{}`. HLAE will record nothing; OBS is \
                             driven from the engine's own console markers.",
                            preflight.obs_version,
                            preflight.websocket_version,
                            config.obs.redacted(),
                            preflight.canvas_width,
                            preflight.canvas_height,
                            preflight.fps,
                            preflight.current_scene,
                        ));
                        // Hand the connection to the guard first, so that from
                        // this point on every exit path stops the recorder.
                        _cleanup_guard.obs = Some(session.stop_handle());
                        obs_session = Some(session);
                    }
                    Err(e) => {
                        log_crash_abort!(tx, format!("OBS capture could not start — {}", e));
                        return;
                    }
                }

                // The log is deleted at the end of a batch, not the start, so a
                // file left by a previous run is normal and its markers are
                // history. `LogTailer::at_end` is what stops a stale
                // `START_RECORD` firing a recording the moment this begins.
                let log_path = hl_exe_parent.join("qconsole.log");
                let tailer = crate::obs::LogTailer::at_end(&log_path);
                let cancel = Arc::clone(&tail_cancel);
                if let Err(e) = std::thread::Builder::new()
                    .name("obs_log_tail".into())
                    .spawn(move || tailer.run(marker_tx, cancel))
                {
                    log_crash_abort!(tx, format!("could not start the console log reader: {}", e));
                    return;
                }
            }

            let condebug_flag = if config.add_condebug { "-condebug " } else { "" };
            // `-afxForceAlpha8` is read by AfxHookGoldSrc.dll off the *game's*
            // command line, not by HLAE.exe. HLAE's own Launch GoldSrc dialog
            // appends it when its alpha box is ticked — but that dialog is the
            // path we do not use. Under `-customLoader` we build the game
            // command line ourselves, so nothing appends it and the hook never
            // sees it. The `-forceAlpha true` passed to HLAE.exe below is a
            // Launcher-mode switch and does not reach the hook from here.
            //
            // `-afxForceAlpha8` TAKES A VALUE. From HLAE's own Launcher.cs, the
            // dialog builds it as:
            //
            //     " -afxForceAlpha8 " + (cfg.ForceAlpha ? 1 : 0).ToString()
            //
            // Passing it bare — which is what two earlier attempts did — makes
            // the hook read the following token as its argument, find something
            // that is not `1`, and leave the alpha channel off. That is exactly
            // the observed behaviour: the flag parses, and the captured hudAlpha
            // bitmaps come out byte-for-byte identical to a run without it.
            //
            // `-afxRenderMode` takes one of standard|fBO|memoryDC the same way.
            // Under `-customLoader` HLAE composes nothing for us — the dialog is
            // what normally assembles these — so the whole set has to be built
            // here, values included.
            //
            // `-32bpp` because a framebuffer that is not 32-bit has no alpha
            // bits to force in the first place. `-afxOptimizeCaptureVis` is left
            // out: it is a visibility optimisation unrelated to alpha, and would
            // be one more variable over the capture itself.
            //
            // Gated on separate_hud deliberately: only the HUD pair needs the
            // alpha buffer, and the single-stream `all` capture is a known-good
            // path not worth perturbing to fix something it does not use.
            let alpha_flags = if config.separate_hud {
                "-gl -32bpp -afxRenderMode standard -afxForceAlpha8 1 "
            } else {
                ""
            };
            let extra_args = format!(
                "{}{}+exec dodtools_helper.cfg +playdemo primer",
                condebug_flag, alpha_flags
            );

            let dummy_path = active_export_dir.join("DOD_BATCH_DONE");
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

            log_markdown(&format!("[HLAE] Spawning: {:?} {:?}", cmd.get_program(), cmd.get_args().collect::<Vec<_>>()));

            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    log_crash_abort!(tx, format!("Failed to spawn HLAE (OS Error): {}", e));
                    for path in &active_dest_paths {
                        let _ = std::fs::remove_file(path);
                    }
                    std::fs::remove_file(&cfg_path).ok();
                    return;
                }
            };
            log_markdown(&format!("[HLAE] Spawned (PID {})", child.id()));

            // The HLAE launcher process (`_child` above) injects into `hl.exe` and then
            // exits on its own under `-noGui -autoStart` — that is expected handoff
            // behaviour, not a crash. The thing that actually matters is whether
            // `hl.exe` itself is still alive, so liveness is tracked separately via
            // process name rather than via the launcher's own exit status.
            let start_time = std::time::Instant::now();
            let mut launcher_exit_logged = false;
            let mut hl_seen_alive = false;
            let mut failure_reason: Option<&'static str> = None;
            // Stall detection state. Both stay untouched outside OBS mode,
            // where no markers are drained and `last_marker_at` never leaves
            // `None` — the watchdog below is inert as a result.
            let mut last_marker_at: Option<std::time::Instant> = None;
            let mut longest_marker_gap = std::time::Duration::ZERO;
            // Armed from hl.exe coming up, not from the first marker. A batch
            // where markers never start at all — a demo that fails to load, a
            // game sitting at the menu — would otherwise never be watched,
            // because the deadline had nothing to count from. Seen in a real
            // run: 398s with hl.exe alive and not one marker.
            let mut hl_first_seen: Option<std::time::Instant> = None;
            let mut sys = {
                use sysinfo::SystemExt;
                sysinfo::System::new_all()
            };
            loop {
                // Drain whatever the engine has echoed since the last pass.
                // This is the whole synchronisation mechanism: markers reach
                // the log 21-40 ms after the tick that emitted them, measured
                // over a 17-block batch, so a 500 ms poll below would be far
                // too coarse to act on them — hence the shorter sleep chosen
                // for OBS mode at the end of this loop.
                if let Some(session) = obs_session.as_mut() {
                    for marker in marker_rx.try_iter() {
                        let now = std::time::Instant::now();
                        if let Some(previous) = last_marker_at {
                            longest_marker_gap = longest_marker_gap.max(now - previous);
                        }
                        last_marker_at = Some(now);
                        session.on_marker(&marker);
                    }
                }

                if !launcher_exit_logged {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            launcher_exit_logged = true;
                            log_markdown(&format!(
                                "[HLAE] Launcher exited after {:.1}s (status: {:?}) — expected handoff behaviour, not a crash by itself",
                                start_time.elapsed().as_secs_f32(),
                                status
                            ));
                        }
                        Ok(None) => {}
                        Err(e) => log_markdown(&format!("[HLAE] try_wait() failed: {}", e)),
                    }
                }

                let hl_alive = {
                    use sysinfo::{SystemExt, ProcessExt};
                    sys.refresh_processes();
                    let alive = sys.processes().values().any(|p| p.name().eq_ignore_ascii_case("hl.exe"));
                    if alive {
                        hl_seen_alive = true;
                        hl_first_seen.get_or_insert_with(std::time::Instant::now);
                    }
                    alive
                };

                if cancel_token.load(Ordering::Relaxed) {
                    log_markdown(&format!("[HLAE] Cancelled by user after {:.1}s", start_time.elapsed().as_secs_f32()));
                    std::process::Command::new("taskkill").args(&["/F", "/IM", "hl.exe"]).output().ok();
                    break;
                }
                // Counts from the last marker, or from hl.exe first appearing
                // when none has arrived yet — the second case is a demo that
                // never started playing, which produces no markers to count
                // from at all. Both mean the same thing: the game is alive and
                // the batch is not advancing.
                if obs_mode {
                    if let Some(since) = last_marker_at.or(hl_first_seen) {
                        if hl_alive && since.elapsed() > marker_stall_deadline(longest_marker_gap) {
                            let never_started = last_marker_at.is_none();
                            log_markdown(&format!(
                                "[HLAE] No console markers for {:.0}s with hl.exe still running — treating the batch as stalled. {}",
                                since.elapsed().as_secs_f32(),
                                if never_started {
                                    "Playback never produced a single marker, so the demo most likely failed to load."
                                } else {
                                    "`disconnect` typed in the console, a demo that ended early, and a frozen engine all look like this from out here."
                                }
                            ));
                            failure_reason = Some(if never_started {
                                "the demo never started playing — hl.exe came up but produced no console markers at all"
                            } else {
                                "the batch stopped progressing while hl.exe was still running — no console markers arrived for several minutes"
                            });
                            std::process::Command::new("taskkill").args(&["/F", "/IM", "hl.exe"]).output().ok();
                            break;
                        }
                    }
                }
                // OBS gone for good, after a reconnect was already tried.
                // Continuing would play the demo to the end capturing nothing
                // and then report the batch as finished.
                if obs_session.as_ref().is_some_and(|s| s.is_dead()) {
                    log_markdown("[HLAE] OBS is unreachable and could not be reconnected — aborting rather than finishing the batch with nothing recorded.");
                    failure_reason = Some("lost contact with OBS mid-batch and could not reconnect");
                    std::process::Command::new("taskkill").args(&["/F", "/IM", "hl.exe"]).output().ok();
                    break;
                }
                if start_time.elapsed().as_secs() > 10 && (dummy_path.exists() || exit_trigger.exists()) {
                    log_markdown(&format!(
                        "[HLAE] Exit trigger detected after {:.1}s (done marker: {}, exit trigger: {}) — taskkilling hl.exe",
                        start_time.elapsed().as_secs_f32(),
                        dummy_path.exists(),
                        exit_trigger.exists()
                    ));
                    std::process::Command::new("taskkill").args(&["/F", "/IM", "hl.exe"]).output().ok();
                    break;
                }
                // Only treat this as a real failure once the launcher has handed off
                // (or failed to) AND hl.exe itself is confirmed not running AND no
                // exit trigger has appeared — i.e. nothing is left that could ever
                // finish the batch. The 5s grace period covers the brief window
                // between the launcher exiting and hl.exe's own process becoming
                // visible to sysinfo.
                if launcher_exit_logged
                    && !hl_seen_alive
                    && start_time.elapsed().as_secs() > 5
                    && !dummy_path.exists()
                    && !exit_trigger.exists()
                {
                    log_markdown(&format!(
                        "[HLAE] hl.exe never came up after the launcher exited ({:.1}s elapsed) — treating as failure",
                        start_time.elapsed().as_secs_f32()
                    ));
                    failure_reason = Some("hl.exe never started after the HLAE launcher exited");
                    break;
                }
                // hl.exe was running and has now disappeared without ever writing
                // an exit trigger/done marker, unlike the normal quit-cfg-driven
                // exit which writes the trigger before the process goes away.
                //
                // Not necessarily a crash, and it used to say it was: `quit` in
                // the console, ALT+F4 and End Process all land here too, and all
                // of them are the user closing the game deliberately. Nothing
                // visible from out here separates them from an access violation
                // — the exit status belongs to the launcher, which handed off
                // long ago — so the wording covers both rather than guessing.
                if hl_seen_alive && !hl_alive && !dummy_path.exists() && !exit_trigger.exists() {
                    log_markdown(&format!(
                        "[HLAE] hl.exe is gone with no exit trigger after {:.1}s — either the game was closed (quit / ALT+F4 / End Process) or it crashed",
                        start_time.elapsed().as_secs_f32()
                    ));
                    failure_reason = Some("hl.exe ended before the batch finished — the game was either closed manually or crashed (no exit trigger was written)");
                    break;
                }
                // 500 ms is fine for watching a process; it is far too coarse
                // for acting on stage markers, where the engine's own flush
                // granularity is ~21 ms and the pre-roll budget being spent is
                // one second. The process checks above tolerate the faster
                // cadence — `refresh_processes` is the only real cost, and it
                // is cheap next to a capture.
                let poll = if obs_mode { 16 } else { 500 };
                std::thread::sleep(std::time::Duration::from_millis(poll));
            }

            // Stop anything still recording and put the scene back before the
            // teardown below starts removing junctions.
            if let Some(mut session) = obs_session.take() {
                tail_cancel.store(true, Ordering::Relaxed);
                for marker in marker_rx.try_iter() {
                    session.on_marker(&marker);
                }
                session.finish();
                log_markdown(&format!(
                    "🎥 **OBS capture** — {} block(s) recorded, {} skipped.",
                    session.recorded.len(),
                    session.skipped.len()
                ));
                for problem in &session.skipped {
                    log_markdown(&format!("⚠️ **OBS** — {problem}"));
                }
                for block in &session.recorded {
                    // The duration is wall-clock, which tracks the file on a
                    // clean stop and overstates it on a salvaged one — the
                    // recording was cut off, so the seconds after that are not
                    // in the file. Said out loud rather than silently printed
                    // as if it were a length.
                    log_markdown(&format!(
                        "- [obs] {} ({:.1}s{}) -> {}",
                        block.take_folder.display(),
                        block.seconds,
                        if block.salvaged { " of recording, salvaged — the file is shorter" } else { "" },
                        block.video.display()
                    ));
                }
            }
            tail_cancel.store(true, Ordering::Relaxed);

            if let Some(reason) = failure_reason.filter(|_| !cancel_token.load(Ordering::Relaxed)) {
                log_crash_abort!(tx, format!("{} — see [HLAE] lines above in this log for timing.", reason));
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
                return;
            }

            if cancel_token.load(Ordering::Relaxed) {
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

            let autosave_path = crate::shared::paths::get_appdata_dir().join(".autosave.json");
            if let Err(e) = std::fs::remove_file(&autosave_path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("[autosave] Failed to remove .autosave.json: {}", e);
                }
            } else {
                log::info!("[autosave] Lockfile removed after clean completion");
            }

            let _ = tx.send(EngineEvent::AllCompleted);
        })
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A batch that has shown nothing unusual gets the floor. Five minutes is
    /// the number that has to clear an unfocused game's stalled fast-forward.
    #[test]
    fn quiet_batch_waits_the_floor() {
        assert_eq!(marker_stall_deadline(Duration::ZERO), MARKER_STALL_FLOOR);
        assert_eq!(
            marker_stall_deadline(Duration::from_secs(30)),
            MARKER_STALL_FLOOR
        );
    }

    /// A demo whose breadcrumbs are genuinely further apart than the floor
    /// must not be killed for being slow — it has already demonstrated the gap
    /// is legitimate.
    #[test]
    fn adapts_to_a_batch_with_long_legitimate_gaps() {
        let observed = Duration::from_secs(240);
        assert_eq!(
            marker_stall_deadline(observed),
            Duration::from_secs(720),
            "three times the longest gap already survived"
        );
    }

    /// The adaptive term only ever widens the window. Narrowing it would make
    /// a fast batch trip on its first slow stretch.
    #[test]
    fn never_shortens_below_the_floor() {
        for secs in [0, 1, 10, 99, 100, 101] {
            assert!(marker_stall_deadline(Duration::from_secs(secs)) >= MARKER_STALL_FLOOR);
        }
    }
}
