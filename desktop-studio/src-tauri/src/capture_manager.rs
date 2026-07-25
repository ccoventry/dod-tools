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

use native::patch::{PatcherConfig, CaptureStreak, PatchJob, build_batch_queue, DriveAllocationStrategy};
use native::capture_engine::{spawn_capture_engine, CaptureJob, EngineEvent};
use serde::{Deserialize, Serialize};

// ── IPC payload type ───────────────────────────────────────────────────────────

/// Top-level payload from the frontend when the user triggers a capture batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturePayload {
    /// Absolute path to hlae.exe
    pub hlae_path: String,
    /// Absolute path to hl.exe
    pub game_path: String,
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
            frame_times: Arc::new(Vec::new()),
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
    // Drive routing
    cfg.primary_media_dir = payload.drives.first().map(std::path::PathBuf::from);
    cfg.backup_media_dir = payload.drives.get(1).map(std::path::PathBuf::from);
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

    // ── Offload blocking I/O to a dedicated thread ────────────────────────────
    tokio::task::spawn_blocking(move || {
        let patch_jobs = match build_batch_queue(raw_streaks, &patcher_config) {
            Ok(jobs) => jobs,
            Err(e) => {
                log::error!("build_batch_queue failed: {}", e);
                let mut running = is_running_arc.lock().unwrap_or_else(|p| p.into_inner());
                *running = false;
                return;
            }
        };

        if patch_jobs.is_empty() {
            log::warn!("build_batch_queue produced no jobs");
            let mut running = is_running_arc.lock().unwrap_or_else(|p| p.into_inner());
            *running = false;
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

        // Spawn a listener to clear the running flag when the batch terminates
        let is_running_clone = Arc::clone(&is_running_arc);
        std::thread::spawn(move || {
            while let Ok(event) = engine_rx.recv() {
                match event {
                    EngineEvent::AllCompleted | EngineEvent::Cancelled | EngineEvent::Error(_) => {
                        let mut running = is_running_clone.lock().unwrap_or_else(|p| p.into_inner());
                        *running = false;
                        break;
                    }
                    _ => {}
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
        }
    }
}

pub async fn scan_directory_impl(paths: Vec<String>) -> Result<Vec<SerializedDemo>, String> {
    tokio::task::spawn_blocking(move || {
        use native::patch::{scan_demo_for_highlights, HighlightRules};

        let mut list = Vec::new();
        let mut dir_stack = Vec::new();
        
        for path_str in paths {
            let path_buf = PathBuf::from(path_str);
            if path_buf.is_dir() {
                dir_stack.push(path_buf);
            } else if path_buf.is_file() && path_buf.extension().map(|ext| ext == "dem").unwrap_or(false) {
                let insert_idx = list.binary_search_by(|p: &PathBuf| p.file_name().unwrap_or_default().cmp(&path_buf.file_name().unwrap_or_default()))
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
                        let insert_idx = list.binary_search_by(|p: &PathBuf| p.file_name().unwrap_or_default().cmp(&path.file_name().unwrap_or_default()))
                            .unwrap_or_else(|pos| pos);
                        list.insert(insert_idx, path);
                    }
                }
            }
        }

        let rules = HighlightRules {
            max_time_gap: None,
        };

        let mut results = Vec::new();
        for file in list {
            if let Ok((tickrate, streaks, is_pov, local_player_index, playback_frames)) = scan_demo_for_highlights(&file, &rules) {
                let serialized_streaks: Vec<SerializedStreak> = streaks.into_iter().map(SerializedStreak::from).collect();
                results.push(SerializedDemo {
                    path: file.to_string_lossy().to_string(),
                    name: file.file_name().unwrap_or_default().to_string_lossy().to_string(),
                    tickrate,
                    is_pov,
                    local_player_index,
                    playback_frames,
                    streaks: serialized_streaks,
                });
            }
        }

        Ok(results)
    }).await.map_err(|e| format!("Task failed: {}", e))?
}
