// ============================================================
// desktop-studio/src-tauri/src/capture_manager.rs
//
// Headless CaptureManager — bridges the native patch pipeline
// into Tauri's managed-state system.
//
// Isolation contract:
//   - MUST NOT import any egui / eframe symbols.
//   - All shared fields use Arc<Mutex<T>> for safe cross-thread access.
//   - Blocking I/O is offloaded to tokio::task::spawn_blocking so the
//     async Tauri command never parks the main thread.
// ============================================================

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use native::patch::{PatcherConfig, CaptureStreak, PatchJob, StreamPatcher, build_batch_queue, build_preview_patch_jobs, DriveAllocationStrategy};
use native::capture_engine::{spawn_capture_engine, CaptureJob, EngineEvent};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

// ── IPC payload type ───────────────────────────────────────────────────────────

/// Top-level payload from the frontend when the user triggers a capture batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturePayload {
    /// Absolute path to hlae.exe
    pub hlae_path: String,
    /// Absolute path to hl.exe
    pub game_path: String,
    /// Optional absolute path to ffmpeg.exe; falls back to bundled then PATH.
    #[serde(default)]
    pub ffmpeg_override_path: Option<String>,
    /// Explicit primary capture output folder; takes precedence over `drives[0]`.
    #[serde(default)]
    pub primary_media_dir: Option<String>,
    /// Explicit backup capture output folder; takes precedence over `drives[1]`.
    #[serde(default)]
    pub backup_media_dir: Option<String>,
    #[serde(default = "default_resolution_width")]
    pub resolution_width: i32,
    #[serde(default = "default_resolution_height")]
    pub resolution_height: i32,
    #[serde(default)]
    pub separate_hud: bool,
    #[serde(default)]
    pub save_local_patched_copy: bool,
    #[serde(default = "default_add_condebug")]
    pub add_condebug: bool,
    /// Highlight streaks to capture.
    pub streaks: Vec<SerializedStreak>,
    /// Pre-roll added before each streak (seconds). Converted → ticks at 100 Hz.
    pub pre_roll_seconds: f32,
    /// Post-roll padding after each streak (seconds).
    pub post_roll_seconds: f32,
    /// Directories used for dynamic drive failover and capture routing.
    pub capture_directories: Vec<String>,
    pub capture_fps: i32,
    pub expected_fps: f32,
    /// Output drives for AOT capacity simulation and media routing.
    pub drives: Vec<String>,
    /// Matches native `DriveAllocationStrategy`: "MaximizeSpace" | "Chronological".
    pub allocation_strategy: String,
    #[serde(default)]
    pub record_start_lead: f32,
    #[serde(default)]
    pub record_stop_trail: f32,
    #[serde(default = "default_initial_delay")]
    pub initial_delay: f32,
    #[serde(default = "default_fast_forward_speed")]
    pub fast_forward_speed: f32,
    #[serde(default)]
    pub auto_clear_logs: bool,
    #[serde(default)]
    pub auto_clear_previews: bool,
    #[serde(default)]
    pub auto_clear_temp_demos: bool,
}

fn default_initial_delay() -> f32 { 3.0 }
fn default_fast_forward_speed() -> f32 { 10.0 }
fn default_resolution_width() -> i32 { 1280 }
fn default_resolution_height() -> i32 { 720 }
fn default_add_condebug() -> bool { true }

/// One highlight streak — serialisable across the Tauri IPC boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedStreak {
    pub start_tick: i32,
    pub end_tick: i32,
    /// Absolute path to the source .dem file.
    pub source_demo: String,
    pub target_player: Option<String>,
    pub kill_count: usize,
    pub timeline_string: String,
    pub duration_string: String,
    pub player_index: usize,
    /// Raw kill events: (tick, abs_time_secs, weapon_name).
    pub kills: Vec<(i32, f32, String)>,
    pub start_index: usize,
    pub end_index: usize,
    pub total_demo_frames: i32,
    pub demo_fps: f32,
    pub viewdemo_times: Vec<f32>,
    /// Tick at which `svc_gametime` resets (i.e. the match clock zero point).
    /// `None` when the scanner could not locate the reset event.
    #[serde(default)]
    pub match_start_tick: Option<i32>,
    /// Per-frame absolute timestamps extracted from the demo binary.
    /// Transported as a plain `Vec` so it crosses the JSON IPC boundary;
    /// deserialization wraps it in `Arc<Vec<f32>>` before handing it to the
    /// native patch engine.
    #[serde(default)]
    pub frame_times: Vec<f32>,
}

impl From<SerializedStreak> for CaptureStreak {
    fn from(s: SerializedStreak) -> Self {
        CaptureStreak {
            start_tick: s.start_tick,
            end_tick: s.end_tick,
            source_demo: s.source_demo,
            target_player: s.target_player,
            kill_count: s.kill_count,
            timeline_string: s.timeline_string,
            duration_string: s.duration_string,
            player_index: s.player_index,
            kills: s.kills,
            start_index: s.start_index,
            end_index: s.end_index,
            total_demo_frames: s.total_demo_frames,
            demo_fps: s.demo_fps,
            viewdemo_times: s.viewdemo_times,
            // Restore the Arc wrapper that was flattened for JSON transport.
            frame_times: std::sync::Arc::new(s.frame_times),
            match_start_tick: s.match_start_tick,
            status: native::patch::types::HighlightStatus::Pending,
        }
    }
}

// ── Managed state ──────────────────────────────────────────────────────────────

/// Tauri managed state for the capture subsystem.
pub struct CaptureManager {
    /// Guards against duplicate batch launches.
    pub is_running: Arc<Mutex<bool>>,
    /// Raised to `true` to request mid-batch cancellation.
    pub cancel_token: Arc<std::sync::atomic::AtomicBool>,
    /// Cached config from the most recent run (used for status queries).
    pub last_config: Arc<Mutex<Option<PatcherConfig>>>,
}

impl CaptureManager {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(Mutex::new(false)),
            cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            last_config: Arc::new(Mutex::new(None)),
        }
    }

    /// Returns `true` if a batch is currently executing.
    pub fn is_running(&self) -> bool {
        *self.is_running.lock().unwrap_or_else(|p| p.into_inner())
    }
}

// ── Internal helpers ───────────────────────────────────────────────────────────

fn config_from_payload(payload: &CapturePayload) -> PatcherConfig {
    let mut cfg = PatcherConfig::default();
    cfg.hlae_path = payload.hlae_path.clone();
    cfg.game_path = payload.game_path.clone();
    cfg.pre_roll_seconds = payload.pre_roll_seconds;
    cfg.post_roll_seconds = payload.post_roll_seconds;
    cfg.pre_roll_ticks = (payload.pre_roll_seconds * payload.expected_fps) as i32;
    cfg.post_roll_ticks = (payload.post_roll_seconds * payload.expected_fps) as i32;
    cfg.capture_directories = payload.capture_directories.iter().map(std::path::PathBuf::from).collect();
    cfg.capture_fps = payload.capture_fps;
    cfg.tickrate = payload.expected_fps;
    cfg.record_start_lead = payload.record_start_lead;
    cfg.record_stop_trail = payload.record_stop_trail;
    cfg.initial_delay = payload.initial_delay;
    cfg.fast_forward_speed = payload.fast_forward_speed;
    cfg.ffmpeg_override_path = payload.ffmpeg_override_path.clone();
    cfg.resolution_width = payload.resolution_width;
    cfg.resolution_height = payload.resolution_height;
    cfg.separate_hud = payload.separate_hud;
    cfg.save_local_patched_copy = payload.save_local_patched_copy;
    cfg.add_condebug = payload.add_condebug;
    cfg.auto_clear_logs = payload.auto_clear_logs;
    cfg.auto_clear_previews = payload.auto_clear_previews;
    cfg.auto_clear_temp_demos = payload.auto_clear_temp_demos;
    // Drive routing: explicit Primary/Backup Media Dir fields (from their own
    // UI inputs) take precedence; fall back to the Target Output Drives list.
    cfg.primary_media_dir = payload.primary_media_dir.as_ref()
        .filter(|s| !s.trim().is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| payload.drives.first().map(std::path::PathBuf::from));
    cfg.backup_media_dir = payload.backup_media_dir.as_ref()
        .filter(|s| !s.trim().is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| payload.drives.get(1).map(std::path::PathBuf::from));
    cfg.allocation_strategy = match payload.allocation_strategy.as_str() {
        "Chronological" => DriveAllocationStrategy::Chronological,
        _ => DriveAllocationStrategy::MaximizeSpace,
    };
    cfg
}

/// Derives the expected HLCR capture output folder from a patched demo path.
fn expected_take_folder(job: &PatchJob, dod_dir: &PathBuf) -> PathBuf {
    let stem = job
        .output_demo
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    dod_dir.join("hlcr_captures").join(stem)
}

// ── Public command handler ─────────────────────────────────────────────────────

/// Async entry point called by the Tauri `start_capture_batch` command.
pub async fn start_capture_batch_impl(
    app_handle: tauri::AppHandle,
    manager: &CaptureManager,
    payload: CapturePayload,
) -> Result<(), String> {
    // ── Guard: reject concurrent batches ──────────────────────────────────────
    {
        let mut running = manager
            .is_running
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if *running {
            return Err("Capture batch already in progress".to_string());
        }
        *running = true;
    }

    // ── Reset cancel token from any prior run ──────────────────────────────────
    manager
        .cancel_token
        .store(false, std::sync::atomic::Ordering::Relaxed);

    // ── Build PatcherConfig from the IPC payload ───────────────────────────────
    let patcher_config = config_from_payload(&payload);
    {
        let mut slot = manager
            .last_config
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        *slot = Some(patcher_config.clone());
    }

    // ── Convert IPC streaks → native CaptureStreak ────────────────────────────
    let raw_streaks: Vec<CaptureStreak> = payload
        .streaks
        .into_iter()
        .map(CaptureStreak::from)
        .collect();

    if raw_streaks.is_empty() {
        let mut running = manager
            .is_running
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        *running = false;
        let _ = app_handle.emit("capture_status", serde_json::json!({
            "running": false,
            "error": true,
            "status": "No streaks in payload"
        }));
        return Err("No streaks in payload".to_string());
    }

    // ── Clone Arcs into the worker closure ────────────────────────────────────
    let is_running_arc = Arc::clone(&manager.is_running);
    let cancel_token_arc = Arc::clone(&manager.cancel_token);
    let hlae_path = PathBuf::from(&patcher_config.hlae_path);
    let hl_path = PathBuf::from(&patcher_config.game_path);
    let dod_dir = hl_path
        .parent()
        .map(|p| p.join("dod"))
        .unwrap_or_else(|| PathBuf::from("dod"));

    let app_handle_clone = app_handle.clone();

    // ── Offload blocking I/O to a dedicated thread ────────────────────────────
    tokio::task::spawn_blocking(move || {
        let patch_jobs = match build_batch_queue(raw_streaks, &patcher_config, &std::collections::HashMap::new()) {
            Ok(jobs) => jobs,
            Err(e) => {
                log::error!("build_batch_queue failed: {}", e);
                let mut running = is_running_arc.lock().unwrap_or_else(|p| p.into_inner());
                *running = false;
                let _ = app_handle_clone.emit("capture_status", serde_json::json!({
                    "running": false,
                    "error": true,
                    "status": format!("build_batch_queue failed: {}", e)
                }));
                return;
            }
        };

        if patch_jobs.is_empty() {
            log::warn!("build_batch_queue produced no jobs");
            let mut running = is_running_arc.lock().unwrap_or_else(|p| p.into_inner());
            *running = false;
            let _ = app_handle_clone.emit("capture_status", serde_json::json!({
                "running": false,
                "status": "No jobs produced"
            }));
            return;
        }

        let capture_jobs: Vec<CaptureJob> = patch_jobs
            .into_iter()
            .map(|job| {
                let expected = expected_take_folder(&job, &dod_dir);
                CaptureJob {
                    patched_demo_path: job.output_demo,
                    expected_take_folder: expected,
                }
            })
            .collect();

        let (engine_tx, engine_rx) = std::sync::mpsc::channel();

        // Spawn a listener to clear the running flag when the batch terminates and forward events to frontend
        let is_running_clone = Arc::clone(&is_running_arc);
        let app_emitter = app_handle_clone.clone();
        std::thread::spawn(move || {
            let mut total_jobs: u32 = 0;
            let mut current_idx: u32 = 0;
            while let Ok(event) = engine_rx.recv() {
                match event {
                    EngineEvent::Starting(total) => {
                        total_jobs = total as u32;
                        current_idx = 0;
                        let _ = app_emitter.emit("capture_status", serde_json::json!({
                            "running": true,
                            "index": current_idx,
                            "total": total_jobs,
                            "status": "Starting"
                        }));
                    }
                    EngineEvent::Launching(name) => {
                        current_idx += 1;
                        let _ = app_emitter.emit("capture_status", serde_json::json!({
                            "running": true,
                            "index": current_idx,
                            "total": total_jobs,
                            "name": name,
                            "status": "Launching"
                        }));
                    }
                    EngineEvent::Finished(name) => {
                        let _ = app_emitter.emit("capture_status", serde_json::json!({
                            "running": true,
                            "index": current_idx,
                            "total": total_jobs,
                            "name": name,
                            "status": "Finished"
                        }));
                    }
                    EngineEvent::Verified(name) => {
                        let _ = app_emitter.emit("capture_status", serde_json::json!({
                            "running": true,
                            "index": current_idx,
                            "total": total_jobs,
                            "name": name,
                            "status": "Verified"
                        }));
                    }
                    EngineEvent::Error(msg) => {
                        let mut running = is_running_clone.lock().unwrap_or_else(|p| p.into_inner());
                        *running = false;
                        let _ = app_emitter.emit("capture_status", serde_json::json!({
                            "running": false,
                            "error": true,
                            "status": msg
                        }));
                        break;
                    }
                    EngineEvent::AllCompleted => {
                        let mut running = is_running_clone.lock().unwrap_or_else(|p| p.into_inner());
                        *running = false;
                        let _ = app_emitter.emit("capture_status", serde_json::json!({
                            "running": false,
                            "status": "Complete",
                            "index": total_jobs,
                            "total": total_jobs
                        }));
                        break;
                    }
                    EngineEvent::Cancelled => {
                        let mut running = is_running_clone.lock().unwrap_or_else(|p| p.into_inner());
                        *running = false;
                        let _ = app_emitter.emit("capture_status", serde_json::json!({
                            "running": false,
                            "status": "Cancelled"
                        }));
                        break;
                    }
                }
            }
        });

        // Delegate to the promoted library engine
        spawn_capture_engine(
            capture_jobs,
            Arc::new(hlae_path),
            Arc::new(hl_path),
            engine_tx,
            cancel_token_arc,
            patcher_config,
        );
    });

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedDemo {
    pub path: String,
    pub name: String,
    pub tickrate: f32,
    pub is_pov: bool,
    pub local_player_index: Option<usize>,
    pub playback_frames: i32,
    pub streaks: Vec<SerializedStreak>,
}

impl From<CaptureStreak> for SerializedStreak {
    fn from(c: CaptureStreak) -> Self {
        Self {
            start_tick: c.start_tick,
            end_tick: c.end_tick,
            source_demo: c.source_demo,
            target_player: c.target_player,
            kill_count: c.kill_count,
            timeline_string: c.timeline_string,
            duration_string: c.duration_string,
            player_index: c.player_index,
            kills: c.kills,
            start_index: c.start_index,
            end_index: c.end_index,
            total_demo_frames: c.total_demo_frames,
            demo_fps: c.demo_fps,
            viewdemo_times: c.viewdemo_times,
            // Flatten Arc<Vec<f32>> → Vec<f32> for JSON transport.
            // The inbound From<SerializedStreak> impl re-wraps it in Arc::new().
            frame_times: (*c.frame_times).clone(),
            match_start_tick: c.match_start_tick,
        }
    }
}

pub async fn scan_directory_impl(
    app_handle: tauri::AppHandle,
    is_scanning: Arc<std::sync::atomic::AtomicBool>,
    cancel_token: Arc<std::sync::atomic::AtomicBool>,
    paths: Vec<String>,
) -> Result<Vec<SerializedDemo>, String> {
    // ── Reset state flags ─────────────────────────────────────────────────────
    cancel_token.store(false, std::sync::atomic::Ordering::SeqCst);
    is_scanning.store(true, std::sync::atomic::Ordering::SeqCst);

    let is_scanning_end = Arc::clone(&is_scanning);

    let result = tokio::task::spawn_blocking(move || {
        use native::patch::scan_demo_for_highlights;
        use tauri::Emitter;

        let mut list = Vec::new();
        let mut dir_stack = Vec::new();

        // ── Phase 1: collect all .dem file paths ─────────────────────────────
        for path_str in paths {
            let path_buf = PathBuf::from(path_str);
            if path_buf.is_dir() {
                dir_stack.push(path_buf);
            } else if path_buf.is_file()
                && path_buf.extension().map(|ext| ext == "dem").unwrap_or(false)
            {
                let insert_idx = list
                    .binary_search_by(|p: &PathBuf| {
                        p.file_name()
                            .unwrap_or_default()
                            .cmp(&path_buf.file_name().unwrap_or_default())
                    })
                    .unwrap_or_else(|pos| pos);
                list.insert(insert_idx, path_buf);
            }
        }

        while let Some(dir) = dir_stack.pop() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        dir_stack.push(path);
                    } else if path.is_file()
                        && path.extension().map(|ext| ext == "dem").unwrap_or(false)
                    {
                        let insert_idx = list
                            .binary_search_by(|p: &PathBuf| {
                                p.file_name()
                                    .unwrap_or_default()
                                    .cmp(&path.file_name().unwrap_or_default())
                            })
                            .unwrap_or_else(|pos| pos);
                        list.insert(insert_idx, path);
                    }
                }
            }
        }

        let total_files = list.len() as u32;

        // ── Phase 2: parse each .dem file ────────────────────────────────────
        let mut results = Vec::new();
        let mut scanned: u32 = 0;

        for file in list {
            // Honour cancellation before each parse (I/O can be slow)
            if cancel_token.load(std::sync::atomic::Ordering::SeqCst) {
                let _ = app_handle.emit(
                    "scan_progress",
                    serde_json::json!({
                        "scanned": scanned,
                        "found": results.len() as u32,
                        "status": "Cancelled",
                        "cancelled": true
                    }),
                );
                return Ok(results);
            }

            scanned += 1;
            let file_name = file
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let _ = app_handle.emit(
                "scan_progress",
                serde_json::json!({
                    "scanned": scanned,
                    "found": results.len() as u32,
                    "status": format!("Scanning {} / {} — {}", scanned, total_files, file_name),
                    "cancelled": false
                }),
            );

            if let Ok((
                tickrate,
                streaks,
                is_pov,
                local_player_index,
                playback_frames,
                match_start_tick,
                frame_times_arc,
            )) = scan_demo_for_highlights(&file)
            {
                let serialized_streaks: Vec<SerializedStreak> = streaks
                    .into_iter()
                    .map(|mut s| {
                        s.match_start_tick = match_start_tick;
                        s.frame_times = frame_times_arc.clone();
                        SerializedStreak::from(s)
                    })
                    .collect();

                results.push(SerializedDemo {
                    path: file.to_string_lossy().to_string(),
                    name: file
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    tickrate,
                    is_pov,
                    local_player_index,
                    playback_frames,
                    streaks: serialized_streaks,
                });
            }
        }

        // ── Final progress event (complete) ───────────────────────────────────
        let _ = app_handle.emit(
            "scan_progress",
            serde_json::json!({
                "scanned": scanned,
                "found": results.len() as u32,
                "status": "Complete",
                "cancelled": false
            }),
        );

        Ok(results)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    is_scanning_end.store(false, std::sync::atomic::Ordering::SeqCst);
    result
}

pub fn simulate_aot_capacity(streaks: Vec<f32>, fps: u32, bytes_per_frame: u64, available_bytes: u64) -> (u64, bool) {
    let mut total_projected_bytes: u64 = 0;
    for duration in streaks {
        let frames = (duration * fps as f32).ceil() as u64;
        total_projected_bytes += frames * bytes_per_frame;
    }
    let has_enough_space = total_projected_bytes <= available_bytes;
    (total_projected_bytes, has_enough_space)
}

// ── On-the-fly Preview (.dodtools_preview) ───────────────────────────────────
//
// Ports `dev`'s native/src/bin/gui/views/capture/workspace.rs::launch_preview,
// which was GUI-binary-only and was never promoted to the library crate during
// the Tauri migration (that whole binary target was dropped). The pieces it
// depended on that DID survive in `native::patch` (`build_preview_patch_jobs`,
// `StreamPatcher`, `PatchJob`, `PatcherConfig::build_hlae_process`) are reused
// as-is here; only the primer-demo orchestration below is new.
//
// Two demos get patched:
//   1. `<stem>_preview.dem` — the highlight itself (via build_preview_patch_jobs),
//      marked with a hidden `.dodtools_preview` sidecar so it's never mistaken
//      for a real recorded demo.
//   2. `primer_preview.dem` — a copy of the ORIGINAL source demo carrying one
//      scheduled console command, `viewdemo <stem>_preview`, fired at tick 500.
//      HLAE is launched against this primer (`+playdemo primer_preview`); the
//      primer's only job is to hand off into the real preview demo in-engine.
//
// `StreamPatcher::patch`'s `PatcherConfig` argument is unused by the function
// itself (see native/src/patch/engine.rs) — it only matters here because
// `build_hlae_process` reads `hlae_path`/`game_path`/`resolution_*` off it.

fn write_hidden_sidecar(path: &std::path::Path) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00000002); // FILE_ATTRIBUTE_HIDDEN
    }
    options.open(path)?;
    Ok(())
}

#[tauri::command]
pub async fn launch_live_preview(
    demo_path: String,
    hlae_path: String,
    game_path: String,
    streak: SerializedStreak,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        if hlae_path.trim().is_empty() || game_path.trim().is_empty() {
            return Err("Configure the HLAE and Half-Life executable paths before previewing.".to_string());
        }
        let hlae_p = std::path::Path::new(&hlae_path);
        let hl_p = std::path::Path::new(&game_path);
        if !hlae_p.is_file() {
            return Err("HLAE executable not found at the configured path.".to_string());
        }
        if !hl_p.is_file() {
            return Err("Half-Life executable not found at the configured path.".to_string());
        }

        let dod_dir = hl_p
            .parent()
            .map(|p| p.join("dod"))
            .ok_or_else(|| "Could not resolve the 'dod' directory next to hl.exe".to_string())?;
        std::fs::create_dir_all(&dod_dir)
            .map_err(|e| format!("Failed to create dod directory: {}", e))?;

        let mut capture_streak = CaptureStreak::from(streak);
        capture_streak.source_demo = demo_path.clone();

        let patcher_config = PatcherConfig {
            hlae_path: hlae_path.clone(),
            game_path: game_path.clone(),
            ..PatcherConfig::default()
        };
        let cancel_token = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // 1. Patch the trimmed highlight into its own preview demo.
        let preview_jobs = build_preview_patch_jobs(vec![capture_streak], Some(dod_dir.as_path()));
        let preview_job = preview_jobs
            .first()
            .ok_or_else(|| "Failed to build the preview patch job".to_string())?;

        StreamPatcher::new(&preview_job.source_demo, &preview_job.output_demo)
            .patch(preview_job, &patcher_config, &cancel_token)
            .map_err(|e| format!("Failed to patch preview demo: {}", e))?;

        // 2. Mark it as a preview artifact via a hidden sidecar.
        let sidecar_path = preview_job.output_demo.with_extension("dodtools_preview");
        write_hidden_sidecar(&sidecar_path)
            .map_err(|e| format!("Failed to write preview sidecar: {}", e))?;

        // 3. Patch the primer demo and launch HLAE against it.
        let preview_stem = preview_job
            .output_demo
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| "Could not resolve the preview demo's file stem".to_string())?
            .to_string();

        let mut primer_init = patcher_config.init_commands.clone();
        primer_init.push(format!(
            "mirv_movie_separate_hud {}",
            if patcher_config.separate_hud { "1" } else { "0" }
        ));

        let primer_job = PatchJob {
            source_demo: demo_path.clone(),
            output_demo: dod_dir.join("primer_preview.dem"),
            streaks: vec![],
            target_player: None,
            init_commands: primer_init,
            scheduled_commands: vec![(500, format!("viewdemo {}", preview_stem))],
            director_events: vec![],
            block_routes: vec![],
        };

        StreamPatcher::new(&primer_job.source_demo, &primer_job.output_demo)
            .patch(&primer_job, &patcher_config, &cancel_token)
            .map_err(|e| format!("Failed to patch primer demo: {}", e))?;

        let mut cmd = patcher_config.build_hlae_process("+playdemo primer_preview");
        cmd.spawn()
            .map_err(|e| format!("Failed to launch HLAE for preview: {}", e))?;

        Ok(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}
