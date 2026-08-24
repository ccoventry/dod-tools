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

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use native::patch::{PatcherConfig, CaptureStreak, CaptureBlock, PatchJob, StreamPatcher, build_batch_queue, build_preview_patch_jobs, CustomCommand, CommandRelation};
use native::capture_engine::{spawn_capture_engine, CaptureJob, EngineEvent};
use native::log_markdown;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

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
    /// Output drives for AOT capacity simulation and media routing.
    pub drives: Vec<String>,
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
    /// Timestamped batch id (e.g. `session_20260813_142233`) stamped by the
    /// frontend before dispatch; routes capture output into its own
    /// subfolder instead of colliding in the export root.
    #[serde(default)]
    pub session_id: String,
    /// Raw console commands injected once at demo-load time, before any
    /// scheduled/highlight commands.
    #[serde(default)]
    pub init_commands: Vec<String>,
    /// User-defined commands scheduled relative to each highlight's bounds.
    #[serde(default)]
    pub custom_commands: Vec<CustomCommandPayload>,
}

fn default_initial_delay() -> f32 { 3.0 }
fn default_fast_forward_speed() -> f32 { 10.0 }
fn default_resolution_width() -> i32 { 1280 }
fn default_resolution_height() -> i32 { 720 }
fn default_add_condebug() -> bool { true }

/// One custom command row — serialisable across the Tauri IPC boundary.
/// `relation` is a plain string ("Before" | "After") rather than
/// `CommandRelation` directly so an unrecognised value fails safe (defaults
/// to `Before` in `config_from_payload`) instead of rejecting the whole
/// batch payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomCommandPayload {
    pub command: String,
    pub relation: String,
    pub offset_seconds: f32,
}

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
    /// Recording blocks the most recent batch planned, used to verify takes
    /// against disk once the batch ends.
    pub last_manifest: Arc<Mutex<Option<CaptureManifest>>>,
}

impl CaptureManager {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(Mutex::new(false)),
            cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            last_config: Arc::new(Mutex::new(None)),
            last_manifest: Arc::new(Mutex::new(None)),
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
    cfg.capture_directories = payload.capture_directories.iter().map(std::path::PathBuf::from).collect();
    cfg.capture_fps = payload.capture_fps;
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
    cfg.session_id = payload.session_id.clone();
    cfg.init_commands = payload.init_commands.clone();
    cfg.custom_commands = payload.custom_commands.iter().map(|c| CustomCommand {
        command: c.command.clone(),
        offset: c.offset_seconds,
        relation: match c.relation.as_str() {
            "After" => CommandRelation::After,
            _ => CommandRelation::Before,
        },
    }).collect();
    // Capture Output is the sole (required) source of output directories —
    // the frontend already blocks the batch if `drives` is empty.
    cfg.primary_media_dir = payload.drives.first().map(std::path::PathBuf::from);
    cfg
}

// ── Take verification ──────────────────────────────────────────────────────────

/// Every recording block a dispatched batch planned, flattened across jobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureManifest {
    pub session_id: String,
    pub blocks: Vec<CaptureBlock>,
}

/// One block's post-batch verdict, checked against what's actually on disk.
#[derive(Debug, Clone, Serialize)]
pub struct VerifiedBlock {
    pub take_key: String,
    pub take_folder: String,
    pub demo_name: String,
    pub block_index: usize,
    /// Positions in the dispatched payload's `streaks` array that this block
    /// covers — several when overlapping highlights merged into one recording.
    pub source_streak_indices: Vec<usize>,
    /// Tier 1: the take folder exists and isn't empty.
    pub captured: bool,
    /// Tier 2: Render Studio's scanner would actually admit this take.
    pub renderable: bool,
}

fn take_folder_has_content(path: &Path) -> bool {
    std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

/// Checks each planned block against disk after a batch ends.
///
/// Two tiers on purpose: "captured" is the loose check that drives status, so
/// an unanticipated HLAE output layout degrades to a warning rather than
/// silently marking nothing; "renderable" reuses Render Studio's own admission
/// predicate so the two views can't disagree without saying so.
fn verify_capture_takes(manifest: &CaptureManifest) -> Vec<VerifiedBlock> {
    let mut verified: Vec<VerifiedBlock> = manifest
        .blocks
        .iter()
        .map(|block| VerifiedBlock {
            take_key: block.take_key.clone(),
            take_folder: block.take_folder.to_string_lossy().into_owned(),
            demo_name: block.demo_name.clone(),
            block_index: block.block_index,
            source_streak_indices: block.source_streak_indices.clone(),
            captured: take_folder_has_content(&block.take_folder),
            renderable: native::hlcr::scanner::is_renderable_take(&block.take_folder),
        })
        .collect();

    // HLAE is force-killed the moment the exit trigger appears, so a take can
    // still be mid-flush when we first look. Re-check only the misses once.
    if verified.iter().any(|v| !v.captured) {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        for (v, block) in verified.iter_mut().zip(manifest.blocks.iter()) {
            if !v.captured {
                v.captured = take_folder_has_content(&block.take_folder);
            }
            if !v.renderable {
                v.renderable = native::hlcr::scanner::is_renderable_take(&block.take_folder);
            }
        }
    }

    verified
}

/// Runs verification for the batch that just ended and reports it to the
/// frontend. Phase 1 is observe-only — nothing consumes this to change a
/// highlight's status yet.
fn emit_take_verification(app: &tauri::AppHandle, manifest_slot: &Arc<Mutex<Option<CaptureManifest>>>) {
    let manifest = {
        let guard = manifest_slot.lock().unwrap_or_else(|p| p.into_inner());
        match guard.as_ref() {
            Some(m) if !m.blocks.is_empty() => m.clone(),
            _ => return,
        }
    };

    let blocks = verify_capture_takes(&manifest);
    let captured_count = blocks.iter().filter(|b| b.captured).count();
    let renderable_count = blocks.iter().filter(|b| b.renderable).count();

    log_markdown(&format!(
        "[take-verify] session {}: {}/{} takes on disk, {} renderable",
        manifest.session_id, captured_count, blocks.len(), renderable_count
    ));
    for block in &blocks {
        log_markdown(&format!(
            "[take-verify] {} captured={} renderable={} at {}",
            block.take_key, block.captured, block.renderable, block.take_folder
        ));
    }

    let _ = app.emit("capture_takes_verified", serde_json::json!({
        "session_id": manifest.session_id,
        "total_count": blocks.len(),
        "captured_count": captured_count,
        "renderable_count": renderable_count,
        "blocks": blocks,
    }));
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
    let manifest_arc = Arc::clone(&manager.last_manifest);
    let hlae_path = PathBuf::from(&patcher_config.hlae_path);
    let hl_path = PathBuf::from(&patcher_config.game_path);

    let app_handle_clone = app_handle.clone();

    // ── Offload blocking I/O to a dedicated thread ────────────────────────────
    tokio::task::spawn_blocking(move || {
        let (patch_jobs, drive_headroom) = match build_batch_queue(raw_streaks, &patcher_config, &std::collections::HashMap::new()) {
            Ok(v) => v,
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

        {
            let manifest = CaptureManifest {
                session_id: patcher_config.session_id.clone(),
                blocks: patch_jobs.iter().flat_map(|j| j.blocks.iter().cloned()).collect(),
            };
            let mut slot = manifest_arc.lock().unwrap_or_else(|p| p.into_inner());
            *slot = Some(manifest);
        }

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

        // ── Write each job's patched demo before the capture engine copies it ───
        // build_batch_queue only plans where each patched demo should land
        // (output_demo paths, drive routing, scheduled commands) — it never
        // writes the bytes. Do that here, one StreamPatcher pass per job,
        // before spawn_capture_engine tries to copy patched_demo_path into
        // the game's dod/ directory (it would otherwise fail with "file not
        // found" since output_demo never existed).
        let total_patch_jobs = patch_jobs.len() as u32;
        for (idx, job) in patch_jobs.iter().enumerate() {
            if cancel_token_arc.load(std::sync::atomic::Ordering::Relaxed) {
                let mut running = is_running_arc.lock().unwrap_or_else(|p| p.into_inner());
                *running = false;
                let _ = app_handle_clone.emit("capture_status", serde_json::json!({
                    "running": false,
                    "status": "Cancelled"
                }));
                return;
            }
            let _ = app_handle_clone.emit("capture_status", serde_json::json!({
                "running": true,
                "index": idx as u32,
                "total": total_patch_jobs,
                "status": format!("Patching {} / {}", idx + 1, total_patch_jobs)
            }));
            if let Err(e) = StreamPatcher::new(&job.source_demo, &job.output_demo)
                .patch(job, &patcher_config, &cancel_token_arc)
            {
                if e.kind() == std::io::ErrorKind::Interrupted {
                    let _ = std::fs::remove_file(&job.output_demo);
                    let mut running = is_running_arc.lock().unwrap_or_else(|p| p.into_inner());
                    *running = false;
                    let _ = app_handle_clone.emit("capture_status", serde_json::json!({
                        "running": false,
                        "status": "Cancelled"
                    }));
                    return;
                }
                log::error!("Failed to patch {}: {}", job.source_demo, e);
                let mut running = is_running_arc.lock().unwrap_or_else(|p| p.into_inner());
                *running = false;
                let _ = app_handle_clone.emit("capture_status", serde_json::json!({
                    "running": false,
                    "error": true,
                    "status": format!("Failed to patch {}: {}", job.source_demo, e)
                }));
                return;
            }
        }

        let capture_jobs: Vec<CaptureJob> = patch_jobs
            .into_iter()
            .map(|job| CaptureJob { patched_demo_path: job.output_demo })
            .collect();

        let (engine_tx, engine_rx) = std::sync::mpsc::channel();

        // Spawn a listener to clear the running flag when the batch terminates and forward events to frontend
        let is_running_clone = Arc::clone(&is_running_arc);
        let manifest_for_listener = Arc::clone(&manifest_arc);
        let app_emitter = app_handle_clone.clone();
        let is_running_for_panic = Arc::clone(&is_running_arc);
        let app_emitter_for_panic = app_handle_clone.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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
                        emit_take_verification(&app_emitter, &manifest_for_listener);
                        break;
                    }
                    EngineEvent::Cancelled => {
                        let mut running = is_running_clone.lock().unwrap_or_else(|p| p.into_inner());
                        *running = false;
                        let _ = app_emitter.emit("capture_status", serde_json::json!({
                            "running": false,
                            "status": "Cancelled"
                        }));
                        // A cancelled batch still leaves real finished takes on
                        // disk — verify anyway rather than discarding them.
                        emit_take_verification(&app_emitter, &manifest_for_listener);
                        break;
                    }
                }
            }
            }));

            if let Err(panic_payload) = result {
                let msg = panic_payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| panic_payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown panic payload".to_string());
                log_markdown(&format!("[capture] Event listener thread panicked: {}", msg));
                let mut running = is_running_for_panic.lock().unwrap_or_else(|p| p.into_inner());
                *running = false;
                let _ = app_emitter_for_panic.emit("capture_status", serde_json::json!({
                    "running": false,
                    "error": true,
                    "status": format!("Internal error in capture event listener: {}", msg)
                }));
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
            drive_headroom,
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
        use native::patch::scan_demo_for_highlights_with_analysis;
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
                (
                    tickrate,
                    streaks,
                    is_pov,
                    local_player_index,
                    playback_frames,
                    match_start_tick,
                    frame_times_arc,
                ),
                analysis,
            )) = scan_demo_for_highlights_with_analysis(&file)
            {
                // Pre-warm the analyzer cache with the Analysis this scan already
                // computed, so opening this demo in the Demo Analyzer afterward
                // hits the cache path instead of re-parsing. Best-effort/silent.
                native::warm_analyzer_cache(&file, &analysis);

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

// ── Bookmark Previews (.dodtools_preview) ─────────────────────────────────────
//
// `build_preview_patch_jobs` (native/src/patch/builder.rs) groups a flat list
// of streaks by `source_demo` and, per demo, injects one `svc_director`
// STUFFTEXT "bookmark" event per selected highlight (plus MATCH_START/DEMO_END)
// into a copy of the original saved as `<stem>_preview.dem` — the events show
// up as named markers when the file is loaded through GoldSrc's `viewdemo` VCR
// UI. Each output file is marked with a hidden `.dodtools_preview` sidecar so
// it's never mistaken for a real recorded demo.
//
// Two entry points share this core:
//   - `launch_demo_preview`   — single demo, launches HLAE via `+viewdemo`
//     directly against the patched preview once it's on disk.
//   - `generate_all_previews` — arbitrary demos in one flat streak list (the
//     grouping above splits them back out), patched to disk only; the user
//     loads them manually afterwards.

fn write_hidden_sidecar(path: &Path) -> std::io::Result<()> {
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

/// Validates the HLAE/hl.exe paths, ensures `<hl_parent>/dod` exists, and
/// builds a minimal `PatcherConfig` carrying just the fields
/// `build_hlae_process` reads (hlae_path/game_path/resolution/separate_hud).
fn resolve_preview_env(hlae_path: &str, game_path: &str) -> Result<(PatcherConfig, PathBuf), String> {
    if hlae_path.trim().is_empty() || game_path.trim().is_empty() {
        return Err("Configure the HLAE and Half-Life executable paths before previewing.".to_string());
    }
    let hlae_p = Path::new(hlae_path);
    let hl_p = Path::new(game_path);
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

    let patcher_config = PatcherConfig {
        hlae_path: hlae_path.to_string(),
        game_path: game_path.to_string(),
        ..PatcherConfig::default()
    };
    Ok((patcher_config, dod_dir))
}

/// Builds and patches one bookmark-preview `PatchJob` per source demo present
/// in `streaks`, writing the hidden `.dodtools_preview` sidecar for each.
fn patch_bookmark_previews(
    streaks: Vec<SerializedStreak>,
    dod_dir: &Path,
    patcher_config: &PatcherConfig,
) -> Result<Vec<PatchJob>, String> {
    if streaks.is_empty() {
        return Err("No highlights selected to preview.".to_string());
    }
    let capture_streaks: Vec<CaptureStreak> = streaks.into_iter().map(CaptureStreak::from).collect();
    let jobs = build_preview_patch_jobs(capture_streaks, Some(dod_dir));
    if jobs.is_empty() {
        return Err("Failed to build any preview patch jobs.".to_string());
    }

    let cancel_token = Arc::new(std::sync::atomic::AtomicBool::new(false));
    for job in &jobs {
        StreamPatcher::new(&job.source_demo, &job.output_demo)
            .patch(job, patcher_config, &cancel_token)
            .map_err(|e| format!("Failed to patch preview demo for {}: {}", job.source_demo, e))?;

        let sidecar_path = job.output_demo.with_extension("dodtools_preview");
        write_hidden_sidecar(&sidecar_path)
            .map_err(|e| format!("Failed to write preview sidecar: {}", e))?;
    }
    Ok(jobs)
}

/// Patches the given demo's selected highlights into a single bookmarked
/// `<stem>_preview.dem` and immediately launches HLAE against it via
/// `+viewdemo <stem>_preview`.
#[tauri::command]
pub async fn launch_demo_preview(
    hlae_path: String,
    game_path: String,
    streaks: Vec<SerializedStreak>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let (patcher_config, dod_dir) = resolve_preview_env(&hlae_path, &game_path)?;
        let jobs = patch_bookmark_previews(streaks, &dod_dir, &patcher_config)?;
        let job = jobs.first().ok_or_else(|| "Failed to build the preview patch job".to_string())?;

        let preview_stem = job
            .output_demo
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| "Could not resolve the preview demo's file stem".to_string())?;

        let mut cmd = patcher_config.build_hlae_process(&format!("+viewdemo {}", preview_stem));
        cmd.spawn()
            .map_err(|e| format!("Failed to launch HLAE for preview: {}", e))?;

        Ok(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Patches every demo represented in `streaks` into its own bookmarked
/// `<stem>_preview.dem` without launching HLAE. Resolves to the number of
/// preview demos generated so the frontend can report a completion toast.
#[tauri::command]
pub async fn generate_all_previews(
    hlae_path: String,
    game_path: String,
    streaks: Vec<SerializedStreak>,
) -> Result<usize, String> {
    tokio::task::spawn_blocking(move || {
        let (patcher_config, dod_dir) = resolve_preview_env(&hlae_path, &game_path)?;
        let jobs = patch_bookmark_previews(streaks, &dod_dir, &patcher_config)?;
        Ok(jobs.len())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

// ── Standalone Game Launch ──────────────────────────────────────────────────────
//
// Boots the GoldSrc engine environment through HLAE without a demo loaded —
// same `build_hlae_process` plumbing as the preview launchers above, minus
// any `+playdemo`/`+viewdemo` engine arg. Reads the persisted `AppSettings`
// (not an IPC payload) since this is a standalone action triggered from the
// global actions area rather than the per-demo detail pane.

/// Builds the `+`-prefixed startup console command string from the user's
/// configured init commands. Demo-time injection (STUFFTEXT frames patched
/// into the .dem, see `build_preview_patch_jobs`) isn't available here since
/// there's no demo — this is the standalone-launch equivalent, so any
/// `playdemo`/`viewdemo` command is stripped defensively even though
/// `init_commands` shouldn't carry one by convention.
fn build_standalone_extra_args(init_commands: &[String]) -> String {
    init_commands
        .iter()
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
        .filter(|c| {
            let lower = c.to_lowercase();
            !lower.starts_with("playdemo") && !lower.starts_with("viewdemo")
        })
        .map(|c| format!("+{}", c))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Launches HLAE directly against `hl.exe` with no demo loaded, applying the
/// persisted resolution/HUD/init-command configuration from `AppSettings`.
#[tauri::command]
pub async fn launch_standalone_game(app: tauri::AppHandle) -> Result<(), String> {
    let settings_state = app.state::<crate::settings_manager::SettingsManager>();
    let settings = {
        let guard = settings_state
            .inner
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        guard.clone()
    };

    tokio::task::spawn_blocking(move || {
        if settings.hlae_path.trim().is_empty() || settings.hl_path.trim().is_empty() {
            return Err("Configure the HLAE and Half-Life executable paths before launching.".to_string());
        }
        if !Path::new(&settings.hlae_path).is_file() {
            return Err("HLAE executable not found at the configured path.".to_string());
        }
        if !Path::new(&settings.hl_path).is_file() {
            return Err("Half-Life executable not found at the configured path.".to_string());
        }

        let patcher_config = PatcherConfig {
            hlae_path: settings.hlae_path.clone(),
            game_path: settings.hl_path.clone(),
            resolution_width: settings.resolution_width,
            resolution_height: settings.resolution_height,
            separate_hud: settings.separate_hud,
            ..PatcherConfig::default()
        };

        let extra_args = build_standalone_extra_args(&settings.init_commands);

        let mut cmd = patcher_config.build_hlae_process(&extra_args);
        cmd.spawn()
            .map_err(|e| format!("Failed to launch HLAE: {}", e))?;

        Ok(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

// ── Running Process Guard ───────────────────────────────────────────────────────
//
// Launching a fresh HLAE Game Capture preview against a demo while a prior
// `hl.exe`/`hlae.exe` instance is still alive corrupts the new session (the
// old instance holds the game's `dod` directory and console). These two
// commands back the pre-flight "Half-Life Preview Detector" modal: check
// before launching, and let the user force-kill stragglers instead of
// hunting them down in Task Manager.

fn is_engine_process_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower == "hl.exe" || lower == "hlae.exe"
}

/// True if any `hl.exe` or `hlae.exe` process is currently running.
#[tauri::command]
pub fn check_engine_processes() -> bool {
    let sys = sysinfo::System::new_all();
    sys.processes()
        .values()
        .any(|p| is_engine_process_name(p.name()))
}

/// Aggressively terminates every running `hl.exe`/`hlae.exe` instance.
#[tauri::command]
pub fn kill_engine_processes() -> Result<(), String> {
    let sys = sysinfo::System::new_all();
    for process in sys.processes().values() {
        if is_engine_process_name(process.name()) {
            if !process.kill() {
                log::warn!("Failed to kill engine process pid={}", process.pid());
            }
        }
    }
    Ok(())
}

// ── Clear Previews audit (orphaned *_preview.dem sweep) ────────────────────────
//
// Bookmark previews (see the block comment above `patch_bookmark_previews`)
// pile up in `<hl>/dod` across capture sessions when `auto_clear_previews`
// is off. These two commands back the "Clear Previews" audit modal: scan for
// leftovers and let the user purge them on confirmation. A file only counts
// as an orphaned preview if it still carries its hidden `.dodtools_preview`
// sidecar — the same marker `patch_bookmark_previews` stamps on every file
// it generates — so a real demo that happens to end in `_preview.dem` is
// never swept up.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewFileSummary {
    pub demo_path: String,
    pub sidecar_path: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub modified_unix_secs: f64,
}

/// Resolves the `dod` directory to sweep, accepting either an `hl.exe` path
/// or the `dod` directory itself (mirrors `resolve_preview_env`'s
/// hl.exe-relative resolution, but tolerates being pointed at the folder
/// directly since the frontend may pass either).
fn resolve_dod_dir_for_sweep(game_dir: &str) -> Result<PathBuf, String> {
    let p = Path::new(game_dir);
    if p.is_file() {
        return p
            .parent()
            .map(|parent| parent.join("dod"))
            .ok_or_else(|| "Could not resolve the 'dod' directory next to hl.exe".to_string());
    }
    if p.is_dir() {
        let is_dod_dir = p
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase() == "dod")
            .unwrap_or(false);
        if is_dod_dir {
            return Ok(p.to_path_buf());
        }
        return Ok(p.join("dod"));
    }
    Err(format!("Game directory not found: {}", game_dir))
}

/// Sweeps `<hl>/dod` for orphaned bookmark-preview demos and reports them
/// (with combined demo + sidecar size) for the audit modal.
#[tauri::command]
pub async fn scan_orphaned_previews(game_dir: String) -> Result<Vec<PreviewFileSummary>, String> {
    tokio::task::spawn_blocking(move || {
        let dod_dir = resolve_dod_dir_for_sweep(&game_dir)?;
        if !dod_dir.is_dir() {
            return Ok(Vec::new());
        }

        let entries = std::fs::read_dir(&dod_dir)
            .map_err(|e| format!("Failed to read dod directory: {}", e))?;

        let mut results = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            if !file_name.to_lowercase().ends_with("_preview.dem") {
                continue;
            }

            let sidecar_path = path.with_extension("dodtools_preview");
            if !sidecar_path.is_file() {
                continue;
            }

            let demo_meta = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let sidecar_size = std::fs::metadata(&sidecar_path).map(|m| m.len()).unwrap_or(0);
            let modified_unix_secs = demo_meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);

            results.push(PreviewFileSummary {
                demo_path: path.to_string_lossy().to_string(),
                sidecar_path: sidecar_path.to_string_lossy().to_string(),
                file_name,
                size_bytes: demo_meta.len() + sidecar_size,
                modified_unix_secs,
            });
        }

        results.sort_by(|a, b| a.file_name.cmp(&b.file_name));
        Ok(results)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Deletes the given orphaned preview demos and their `.dodtools_preview`
/// sidecars. `file_paths` must be `demo_path` values as reported by
/// `scan_orphaned_previews` — anything not ending in `_preview.dem` is
/// skipped rather than deleted. Returns the count of demo files removed; a
/// missing sidecar does not fail the entry.
#[tauri::command]
pub async fn delete_orphaned_previews(file_paths: Vec<String>) -> Result<u32, String> {
    tokio::task::spawn_blocking(move || {
        let mut deleted: u32 = 0;
        for demo_path in file_paths {
            let path = PathBuf::from(&demo_path);
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if !file_name.to_lowercase().ends_with("_preview.dem") {
                continue;
            }

            let sidecar_path = path.with_extension("dodtools_preview");
            let demo_removed = std::fs::remove_file(&path).is_ok();
            let _ = std::fs::remove_file(&sidecar_path);

            if demo_removed {
                deleted += 1;
            }
        }
        Ok(deleted)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload() -> CapturePayload {
        CapturePayload {
            hlae_path: "C:/hlae/hlae.exe".to_string(),
            game_path: "C:/dod/hl.exe".to_string(),
            ffmpeg_override_path: None,
            resolution_width: 1920,
            resolution_height: 1080,
            separate_hud: true,
            save_local_patched_copy: false,
            add_condebug: true,
            streaks: Vec::new(),
            pre_roll_seconds: 2.0,
            post_roll_seconds: 0.6,
            capture_directories: vec!["D:/capture".to_string()],
            capture_fps: 300,
            drives: vec!["D:/capture".to_string(), "E:/capture".to_string()],
            record_start_lead: 0.0,
            record_stop_trail: 0.0,
            initial_delay: 3.0,
            fast_forward_speed: 0.05,
            auto_clear_logs: false,
            auto_clear_previews: false,
            auto_clear_temp_demos: false,
            session_id: "session_test".to_string(),
            init_commands: vec!["exec autoexec".to_string()],
            custom_commands: vec![
                CustomCommandPayload { command: "say after".to_string(), relation: "After".to_string(), offset_seconds: 1.0 },
                CustomCommandPayload { command: "say unrecognized".to_string(), relation: "Sideways".to_string(), offset_seconds: 1.0 },
            ],
        }
    }

    #[test]
    fn test_config_from_payload_maps_scalar_fields() {
        let payload = sample_payload();
        let cfg = config_from_payload(&payload);

        assert_eq!(cfg.hlae_path, payload.hlae_path);
        assert_eq!(cfg.game_path, payload.game_path);
        assert_eq!(cfg.resolution_width, 1920);
        assert_eq!(cfg.resolution_height, 1080);
        assert_eq!(cfg.separate_hud, true);
        assert_eq!(cfg.capture_fps, 300);
        assert_eq!(cfg.session_id, "session_test");
        assert_eq!(cfg.init_commands, vec!["exec autoexec".to_string()]);
        assert_eq!(cfg.capture_directories, vec![PathBuf::from("D:/capture")]);
    }

    #[test]
    fn test_config_from_payload_primary_media_dir_is_first_drive() {
        let payload = sample_payload();
        let cfg = config_from_payload(&payload);
        // Capture Output's first entry is the sole source of primary_media_dir —
        // there's no separate "Primary Media Dir" field anymore (removed 2026-08-17).
        assert_eq!(cfg.primary_media_dir, Some(PathBuf::from("D:/capture")));
    }

    #[test]
    fn test_config_from_payload_no_drives_leaves_primary_media_dir_none() {
        let mut payload = sample_payload();
        payload.drives = Vec::new();
        let cfg = config_from_payload(&payload);
        assert_eq!(cfg.primary_media_dir, None);
    }

    #[test]
    fn test_config_from_payload_custom_command_relation_falls_back_to_before() {
        let payload = sample_payload();
        let cfg = config_from_payload(&payload);

        assert_eq!(cfg.custom_commands.len(), 2);
        assert_eq!(cfg.custom_commands[0].command, "say after");
        assert_eq!(cfg.custom_commands[0].relation, CommandRelation::After);
        // An unrecognised relation string must fail safe to Before rather than
        // rejecting the whole batch payload (see CustomCommandPayload's doc comment).
        assert_eq!(cfg.custom_commands[1].relation, CommandRelation::Before);
    }
}
