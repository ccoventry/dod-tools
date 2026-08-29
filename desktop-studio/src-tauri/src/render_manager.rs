// desktop-studio/src-tauri/src/render_manager.rs
//
// Calls into native::hlcr's real render pipeline (renderer.rs/config.rs/
// autosave.rs/scanner.rs — byte-identical to dev, previously orphaned, see
// docs/tauri_parity_audit.md Area 5) instead of the from-scratch
// reimplementation this module used to carry. Owns the Tauri-side
// orchestration dev's now-deleted `hlcr/ui.rs` used to provide: concurrent
// job scheduling, a JIT multi-drive export pool, per-job progress/state,
// a `.render_autosave.json` crash-recovery lockfile, and a wake lock held
// for the batch's duration.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use serde::{Deserialize, Serialize};
use native::hlcr::autosave::{RenderJob as AutosaveJob, RenderJobStatus as AutosaveJobStatus, RenderSessionData};
use native::hlcr::config::{RenderCodec, RenderConfig};
use native::hlcr::renderer::{hold_render_wake_lock, run_render_job, RenderUpdate, RenderWakeLock};
use native::hlcr::scanner::{scan_folder_background, clip_is_skip_eligible, ClipData};
use native::shared::paths::take_key;
use native::log_markdown;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedRenderJob {
    pub take_folder: String,
    pub clip_type: String,
    pub img_folder: String,
    /// `None` for an OBS take — its audio is already muxed into its video.
    /// The frontend uses this to decide whether "Skip (keep original)" is an
    /// offerable per-job option: only an OBS-shaped clip has anything to skip.
    pub wav_file: Option<String>,
    pub base_name: String,
    pub frame_count: usize,
    pub date: String,
}

impl From<ClipData> for SerializedRenderJob {
    fn from(c: ClipData) -> Self {
        Self {
            take_folder: c.take_folder,
            clip_type: c.clip_type,
            img_folder: c.img_folder,
            wav_file: c.wav_file,
            base_name: c.base_name,
            frame_count: c.frame_count,
            date: c.date,
        }
    }
}

#[tauri::command]
pub async fn scan_render_directories(app: AppHandle, paths: Vec<String>) -> Result<Vec<SerializedRenderJob>, String> {
    tokio::task::spawn_blocking(move || {
        let source_folders: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        let (clip_tx, clip_rx) = std::sync::mpsc::channel();
        let (status_tx, status_rx) = std::sync::mpsc::channel::<String>();

        // scan_folder_background sends one status line per take it finds
        // (blocking, on this thread); drain it concurrently on its own
        // thread so the frontend gets live progress instead of the whole
        // batch arriving at once when the scan finishes. Exits on its own
        // once status_tx drops (scan_folder_background returns below).
        let status_app = app.clone();
        let status_thread = std::thread::spawn(move || {
            while let Ok(msg) = status_rx.recv() {
                let _ = status_app.emit("render_scan_status", msg);
            }
        });

        // scan_folder_background is blocking and exhausts sends before returning.
        // try_recv is therefore race-free here.
        scan_folder_background(source_folders, clip_tx, status_tx);
        let _ = status_thread.join();

        let mut results = Vec::new();
        while let Ok(clip) = clip_rx.try_recv() {
            results.push(SerializedRenderJob::from(clip));
        }

        results.sort_by(|a, b| {
            a.take_folder.cmp(&b.take_folder)
                .then_with(|| a.img_folder.cmp(&b.img_folder))
                .then_with(|| a.clip_type.cmp(&b.clip_type))
        });

        Ok(results)
    })
    .await
    .map_err(|e| format!("Task join failed: {}", e))?
}

/// Codec is a string id ("prores" | "dnxhr" | "h264" | "h264_nvenc") mapped
/// to `RenderCodec` via `RenderCodec::from_str_id` — see
/// docs/tauri_parity_audit.md Area 5 for why `h264`/software libx264 exists
/// alongside dev's NVENC-only H.264 variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderBatchPayload {
    pub render_directories: Vec<String>,
    pub codec: String,
    pub fps: u32,
    pub ffmpeg_path: Option<String>,
    /// JIT multi-drive export pool, priority order — `run_render_job` picks
    /// the first entry with 20 GiB+ free.
    pub export_directories: Vec<String>,
    pub max_concurrent_renders: usize,
}

/// Resolve the FFmpeg binary path using the same fallback chain as the
/// legacy `settings::resolve_ffmpeg_path()`: override → bundled local →
/// system PATH.
fn resolve_ffmpeg(override_path: Option<&String>) -> PathBuf {
    if let Some(p) = override_path {
        let pb = PathBuf::from(p);
        if !p.trim().is_empty() && pb.exists() {
            return pb;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let local = parent.join("local/tools/ffmpeg.exe");
            if local.exists() {
                return local;
            }
        }
    }
    PathBuf::from("ffmpeg")
}

fn autosave_path() -> PathBuf {
    native::shared::paths::get_appdata_dir().join(".render_autosave.json")
}

// ── Per-job runtime state ───────────────────────────────────────────────────

struct RenderJobRuntime {
    id: String,
    clip: ClipData,
    status: String, // "Queued" | "Rendering" | "Finished" | "Error" | "Cancelled"
    speed: String,
    progress: u32,
    error_log: Option<String>,
    /// Absolute path to the encoded file, populated once the job finishes
    /// successfully (empty until then, and for recovered jobs — the autosave
    /// snapshot doesn't persist it).
    output_path: String,
    cancel_flag: Arc<AtomicBool>,
    // Snapshotted onto the job itself (VirtualDub-style job queue, not one
    // shared live config) so Reset to Queued always re-renders with exactly
    // what this job was queued with — never silently picks up a codec/fps
    // change made to the panel afterward, and never silently ignores one
    // either: the Settings column shows precisely what's about to run.
    codec: RenderCodec,
    fps: u32,
}

#[derive(Clone, Serialize)]
pub struct RenderJobView {
    pub id: String,
    pub name: String,
    pub stream: String,
    pub frames: usize,
    pub date: String,
    pub status: String,
    pub speed: String,
    pub progress: u32,
    pub error_log: Option<String>,
    /// e.g. "ProRes @ 300fps" — this job's own settings, not the panel's
    /// current ones. A single summary string rather than separate
    /// codec/fps columns so a future new setting doesn't need its own
    /// column too.
    pub settings_summary: String,
    /// Encoded output file, once finished. Empty until then.
    pub output_path: String,
    /// Source take folder (captured BMPs/WAV) — always known, even before
    /// this job has rendered, so "reveal in Explorer" works pre- and
    /// post-render.
    pub take_folder: String,
    /// This job's codec as a `RenderCodec::to_str_id()` value — what
    /// `set_render_job_codec` expects back, and what the frontend checks
    /// against `"source_copy"` to show the Skip toggle as checked.
    pub codec_id: String,
    /// Whether "Skip (keep original)" is an offerable choice for this job —
    /// only true for an OBS-shaped clip (its own muxed-in audio, not HUD/alpha,
    /// a captured video). `set_render_job_codec` enforces the same rule; this
    /// is what lets the frontend decide whether to show the toggle at all.
    pub skip_available: bool,
}

impl RenderJobRuntime {
    fn to_view(&self) -> RenderJobView {
        let is_source_copy = self.codec == RenderCodec::SourceCopy;
        RenderJobView {
            id: self.id.clone(),
            name: self.clip.base_name.clone(),
            stream: if self.clip.clip_type == "hud_only" { "HUD ONLY".to_string() } else { self.clip.img_folder.clone() },
            frames: self.clip.frame_count,
            date: self.clip.date.clone(),
            status: self.status.clone(),
            speed: self.speed.clone(),
            progress: self.progress,
            error_log: self.error_log.clone(),
            // Skip mode never reads `fps` — showing it would imply a setting
            // that has no effect on a plain file copy.
            settings_summary: if is_source_copy {
                self.codec.label().to_string()
            } else {
                format!("{} @ {}fps", self.codec.label(), self.fps)
            },
            output_path: self.output_path.clone(),
            take_folder: self.clip.take_folder.clone(),
            codec_id: self.codec.to_str_id().to_string(),
            skip_available: clip_is_skip_eligible(&self.clip),
        }
    }
}

// ── Manager state ────────────────────────────────────────────────────────────

pub struct RenderManager {
    jobs: Arc<Mutex<Vec<RenderJobRuntime>>>,
    is_rendering: Arc<AtomicBool>,
    global_cancel: Arc<AtomicBool>,
    wake_lock: Arc<Mutex<Option<RenderWakeLock>>>,
    render_session: Arc<Mutex<Option<RenderSessionData>>>,
    /// Persisted so Reset can resume the scheduler on the existing job list
    /// without a fresh scan+payload (mirrors dev's `start_rendering()`
    /// operating on `self.jobs` directly, independent of the scan step).
    last_config: Arc<Mutex<Option<RenderConfig>>>,
}

struct SchedulerHandles {
    jobs: Arc<Mutex<Vec<RenderJobRuntime>>>,
    is_rendering: Arc<AtomicBool>,
    global_cancel: Arc<AtomicBool>,
    wake_lock: Arc<Mutex<Option<RenderWakeLock>>>,
    render_session: Arc<Mutex<Option<RenderSessionData>>>,
}

impl RenderManager {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(Mutex::new(Vec::new())),
            is_rendering: Arc::new(AtomicBool::new(false)),
            global_cancel: Arc::new(AtomicBool::new(false)),
            wake_lock: Arc::new(Mutex::new(None)),
            render_session: Arc::new(Mutex::new(None)),
            last_config: Arc::new(Mutex::new(None)),
        }
    }

    fn handles(&self) -> SchedulerHandles {
        SchedulerHandles {
            jobs: self.jobs.clone(),
            is_rendering: self.is_rendering.clone(),
            global_cancel: self.global_cancel.clone(),
            wake_lock: self.wake_lock.clone(),
            render_session: self.render_session.clone(),
        }
    }
}

fn write_autosave(render_session: &Arc<Mutex<Option<RenderSessionData>>>, jobs: &[RenderJobRuntime], config: &RenderConfig) {
    let session = RenderSessionData {
        source_folder: config.source_folder.clone(),
        fps: config.fps,
        target_codec: config.target_codec.to_str_id().to_string(),
        jobs: jobs.iter().map(|j| AutosaveJob {
            take_folder: j.clip.take_folder.clone(),
            output_path: j.output_path.clone(),
            status: AutosaveJobStatus::Pending,
            name: j.clip.base_name.clone(),
        }).collect(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&session) {
        if let Err(e) = std::fs::write(autosave_path(), json) {
            log::warn!("[render_autosave] Failed to write lockfile: {}", e);
        }
    }
    *render_session.lock().unwrap() = Some(session);
}

fn emit_jobs_snapshot(app: &AppHandle, jobs: &Arc<Mutex<Vec<RenderJobRuntime>>>) {
    let views: Vec<RenderJobView> = jobs.lock().unwrap().iter().map(RenderJobRuntime::to_view).collect();
    let _ = app.emit("render_jobs_snapshot", views);
}

fn apply_render_update(
    app: &AppHandle,
    jobs: &Arc<Mutex<Vec<RenderJobRuntime>>>,
    render_session: &Arc<Mutex<Option<RenderSessionData>>>,
    update: RenderUpdate,
) {
    match update {
        RenderUpdate::Progress(id, pct) => {
            if let Some(job) = jobs.lock().unwrap().iter_mut().find(|j| j.id == id) {
                job.progress = pct;
            }
        }
        RenderUpdate::Speed(id, speed) => {
            if let Some(job) = jobs.lock().unwrap().iter_mut().find(|j| j.id == id) {
                job.speed = speed;
            }
        }
        RenderUpdate::Status(id, status) => {
            if let Some(job) = jobs.lock().unwrap().iter_mut().find(|j| j.id == id) {
                job.status = status;
            }
        }
        RenderUpdate::OutputPath(id, path) => {
            if let Some(job) = jobs.lock().unwrap().iter_mut().find(|j| j.id == id) {
                job.output_path = path;
            }
        }
        RenderUpdate::Finished(id, success, err_log) => {
            // `just_finished` is keyed on `success` alone, deliberately not
            // on the `job.status == "Rendering"` guard below: on the real
            // completion path run_render_job sends Status("Finished") *then*
            // Finished(true, None), so by the time this arrives job.status
            // already reads "Finished" and that guard has already tripped —
            // it exists to stop this generic transition from clobbering a
            // more specific status a preceding Status update already set
            // (e.g. never overwrite "Cancelled" with "Error"), not to gate
            // "is this the one authoritative completion". Each run_render_job
            // invocation sends exactly one Finished, at its single return
            // point, so success=true can't double-fire this within one run;
            // recover_render_batch separately can't trigger it at all, since
            // it repopulates already-Finished jobs by constructing them
            // directly rather than replaying this update.
            let mut just_finished = None;
            let mut just_failed = None;
            {
                let mut guard = jobs.lock().unwrap();
                if let Some(job) = guard.iter_mut().find(|j| j.id == id) {
                    if job.status == "Rendering" {
                        job.status = if err_log.is_some() { "Error".to_string() } else { "Finished".to_string() };
                        if job.status == "Finished" {
                            job.progress = 100;
                        }
                    }
                    if success {
                        just_finished = Some((
                            job.clip.take_folder.clone(),
                            job.clip.base_name.clone(),
                            job.clip.clip_type.clone(),
                            job.output_path.clone(),
                        ));
                    } else {
                        just_failed = Some((job.clip.base_name.clone(), err_log.clone()));
                    }
                    job.error_log = err_log;
                }
            }
            if let Some((take_folder, base_name, clip_type, _)) = &just_finished {
                let key = take_key(std::path::Path::new(take_folder));
                log_markdown(&format!(
                    "[render-take-finished] job {} take_key={:?} take_folder={} base_name={} clip_type={}",
                    id, key, take_folder, base_name, clip_type
                ));
                let _ = app.emit("render_take_finished", serde_json::json!({
                    "job_id": id.clone(),
                    "take_key": key,
                    "take_folder": take_folder,
                    "base_name": base_name,
                    "clip_type": clip_type,
                }));
            }
            if let Some((base_name, err_log)) = &just_failed {
                log_markdown(&format!(
                    "[render-take-failed] job {} base_name={} error={}",
                    id, base_name, err_log.as_deref().unwrap_or("(no error log)")
                ));
            }
            // Autosave only tracks success — matches dev's ui.rs exactly.
            if success {
                if let Some(session) = render_session.lock().unwrap().as_mut() {
                    if let Ok(idx) = id.parse::<usize>() {
                        if let Some(rj) = session.jobs.get_mut(idx) {
                            rj.status = AutosaveJobStatus::Completed;
                            if let Some((_, _, _, output_path)) = &just_finished {
                                rj.output_path = output_path.clone();
                            }
                        }
                    }
                    if let Ok(json) = serde_json::to_string_pretty(session) {
                        let _ = std::fs::write(autosave_path(), json);
                    }
                }
            }
        }
    }
}

/// Concurrent job scheduler — ports dev's `HlcrState::update_channels`
/// per-frame scheduling loop (`hlcr/ui.rs`, deleted at HEAD) to a periodic
/// async poll: starts queued jobs up to `max_concurrent_renders`, giving a
/// lone job all available CPU threads via the same `effective_concurrent`
/// calculation dev used, and tears down (wake lock, autosave lockfile) once
/// nothing is Rendering or Queued.
fn spawn_scheduler(app: AppHandle, handles: SchedulerHandles, config: RenderConfig) {
    tokio::spawn(async move {
        let (tx, rx) = std::sync::mpsc::channel::<RenderUpdate>();
        let max_concurrent = config.max_concurrent_renders.max(1);

        loop {
            if handles.global_cancel.load(Ordering::SeqCst) {
                let mut guard = handles.jobs.lock().unwrap();
                for job in guard.iter_mut() {
                    if job.status == "Rendering" || job.status == "Queued" {
                        job.cancel_flag.store(true, Ordering::Relaxed);
                        if job.status == "Queued" {
                            job.status = "Cancelled".to_string();
                        }
                    }
                }
            }

            let mut dirty = false;
            while let Ok(update) = rx.try_recv() {
                apply_render_update(&app, &handles.jobs, &handles.render_session, update);
                dirty = true;
            }

            let started_any = {
                let mut guard = handles.jobs.lock().unwrap();
                let active_count = guard.iter().filter(|j| j.status == "Rendering").count();
                let mut started = 0usize;
                if active_count < max_concurrent {
                    let limit = max_concurrent - active_count;
                    let queued_count = guard.iter().filter(|j| j.status == "Queued").count();
                    let jobs_starting = queued_count.min(limit);
                    // Real concurrent count (already-running + newly starting), capped —
                    // gives a lone job all available threads instead of only 1/max of them.
                    let effective_concurrent = (active_count + jobs_starting).min(max_concurrent).max(1);

                    for job in guard.iter_mut() {
                        if started >= limit {
                            break;
                        }
                        if job.status == "Queued" {
                            job.status = "Rendering".to_string();
                            let job_id = job.id.clone();
                            let clip = job.clip.clone();
                            let cancel_flag = job.cancel_flag.clone();
                            // Codec/fps come from the job itself, not the
                            // scheduler's shared config — see the comment on
                            // RenderJobRuntime. Everything else (ffmpeg path,
                            // export pool, concurrency) genuinely is
                            // batch-wide, so still comes from config.
                            let mut job_config = config.clone();
                            job_config.target_codec = job.codec;
                            job_config.fps = job.fps;
                            job_config.max_concurrent_renders = effective_concurrent;
                            let tx2 = tx.clone();
                            tokio::spawn(async move {
                                run_render_job(job_id, clip, job_config, tx2, cancel_flag).await;
                            });
                            started += 1;
                        }
                    }
                }
                started > 0
            };

            if dirty || started_any {
                emit_jobs_snapshot(&app, &handles.jobs);
            }

            let has_active_or_queued = {
                let guard = handles.jobs.lock().unwrap();
                guard.iter().any(|j| j.status == "Rendering" || j.status == "Queued")
            };
            if !has_active_or_queued {
                break;
            }

            tokio::time::sleep(Duration::from_millis(150)).await;
        }

        log_markdown("[render] scheduler loop exiting: nothing left Rendering or Queued");
        handles.is_rendering.store(false, Ordering::SeqCst);
        handles.global_cancel.store(false, Ordering::SeqCst);
        *handles.wake_lock.lock().unwrap() = None; // Drop releases the wake lock.

        let _ = std::fs::remove_file(autosave_path());
        *handles.render_session.lock().unwrap() = None;

        emit_jobs_snapshot(&app, &handles.jobs);
        let _ = app.emit("render_batch_finished", serde_json::json!({}));
    });
}

#[tauri::command]
pub async fn execute_render_batch(
    app: AppHandle,
    state: tauri::State<'_, RenderManager>,
    payload: RenderBatchPayload,
) -> Result<(), String> {
    if state.is_rendering.swap(true, Ordering::SeqCst) {
        log_markdown("[render] execute_render_batch rejected: a batch is already in progress");
        return Err("Render batch already in progress".to_string());
    }
    log_markdown(&format!(
        "[render] execute_render_batch starting: {} source dir(s), codec={}, fps={}, max_concurrent={}",
        payload.render_directories.len(), payload.codec, payload.fps, payload.max_concurrent_renders
    ));
    state.global_cancel.store(false, Ordering::SeqCst);

    let scan_result = tokio::task::spawn_blocking({
        let dirs = payload.render_directories.clone();
        move || {
            let source_folders: Vec<PathBuf> = dirs.into_iter().map(PathBuf::from).collect();
            let (clip_tx, clip_rx) = std::sync::mpsc::channel();
            let (status_tx, _status_rx) = std::sync::mpsc::channel();
            scan_folder_background(source_folders, clip_tx, status_tx);
            let mut clips: Vec<ClipData> = Vec::new();
            while let Ok(clip) = clip_rx.try_recv() {
                clips.push(clip);
            }
            clips
        }
    })
    .await
    .map_err(|e| format!("Task join failed: {}", e))?;

    if scan_result.is_empty() {
        log_markdown("[render] execute_render_batch: no takes found in the scanned directories, aborting");
        state.is_rendering.store(false, Ordering::SeqCst);
        let _ = app.emit("render_jobs_snapshot", Vec::<RenderJobView>::new());
        let _ = app.emit("render_batch_finished", serde_json::json!({ "status": "No takes found to render" }));
        return Ok(());
    }
    log_markdown(&format!("[render] execute_render_batch: {} take(s) found, dispatching to scheduler", scan_result.len()));

    let ffmpeg_path = resolve_ffmpeg(payload.ffmpeg_path.as_ref()).to_string_lossy().into_owned();
    let export_directories: Vec<PathBuf> = payload.export_directories.iter().map(PathBuf::from).collect();
    let config = RenderConfig {
        ffmpeg_path,
        source_folder: payload.render_directories.first().cloned().unwrap_or_default(),
        export_directories,
        fps: payload.fps.max(1),
        target_codec: RenderCodec::from_str_id(&payload.codec),
        max_concurrent_renders: payload.max_concurrent_renders.max(1),
    };

    let jobs: Vec<RenderJobRuntime> = scan_result.into_iter().enumerate().map(|(i, clip)| RenderJobRuntime {
        id: i.to_string(),
        clip,
        status: "Queued".to_string(),
        speed: String::new(),
        progress: 0,
        error_log: None,
        output_path: String::new(),
        cancel_flag: Arc::new(AtomicBool::new(false)),
        codec: config.target_codec,
        fps: config.fps,
    }).collect();

    write_autosave(&state.render_session, &jobs, &config);
    *state.jobs.lock().unwrap() = jobs;
    *state.last_config.lock().unwrap() = Some(config.clone());
    *state.wake_lock.lock().unwrap() = hold_render_wake_lock();

    emit_jobs_snapshot(&app, &state.jobs);
    spawn_scheduler(app, state.handles(), config);

    Ok(())
}

#[tauri::command]
pub async fn cancel_render_batch(state: tauri::State<'_, RenderManager>) -> Result<(), String> {
    log_markdown("[render] cancel_render_batch requested");
    state.global_cancel.store(true, Ordering::SeqCst);
    Ok(())
}

/// Changes one Queued job's codec — the VirtualDub-style per-job setting
/// `RenderJobRuntime` already carries, exposed here specifically so a job can
/// be flipped to/from `SourceCopy` ("Skip, keep original") before it starts.
/// Refused once a job is no longer Queued: a running or finished job's
/// settings are fixed, matching `reset_render_job`'s own doc comment on why
/// per-job settings never change out from under a job in flight.
#[tauri::command]
pub async fn set_render_job_codec(app: AppHandle, state: tauri::State<'_, RenderManager>, job_id: String, codec: String) -> Result<(), String> {
    let requested = RenderCodec::from_str_id(&codec);
    let mut jobs = state.jobs.lock().unwrap();
    let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) else {
        return Err(format!("No such job: {}", job_id));
    };
    if job.status != "Queued" {
        return Err(format!("Job {} is {} — only a Queued job's codec can be changed", job_id, job.status));
    }
    if requested == RenderCodec::SourceCopy && !clip_is_skip_eligible(&job.clip) {
        return Err("Skip (keep original) is only available for a captured OBS take (its own audio, not a HUD/alpha clip).".to_string());
    }
    job.codec = requested;
    log_markdown(&format!("[render] set_render_job_codec {} -> {}", job_id, requested.to_str_id()));
    drop(jobs);
    emit_jobs_snapshot(&app, &state.jobs);
    Ok(())
}

#[tauri::command]
pub async fn cancel_render_job(state: tauri::State<'_, RenderManager>, job_id: String) -> Result<(), String> {
    log_markdown(&format!("[render] cancel_render_job requested for {}", job_id));
    let mut jobs = state.jobs.lock().unwrap();
    if let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) {
        job.cancel_flag.store(true, Ordering::Relaxed);
        if job.status == "Queued" {
            job.status = "Cancelled".to_string();
        }
    }
    Ok(())
}

/// Resets a Finished/Error/Cancelled job back to Queued. If the batch has
/// already fully drained (nothing else Rendering/Queued), immediately
/// resumes the scheduler on the existing job list — a small UX improvement
/// over dev, which required a separate "Start Render" click after Reset to
/// notice the re-queued job at all.
///
/// Deliberately does not touch `codec`/`fps` — a reset job re-renders with
/// exactly the settings it already carries (VirtualDub-style: each job owns
/// its settings, the panel is only ever a template for *new* jobs), never
/// whatever the panel currently shows. The `last_config` used below to
/// resume the scheduler only supplies batch-wide infrastructure (ffmpeg
/// path, export pool, concurrency) — never per-job creative settings.
#[tauri::command]
pub async fn reset_render_job(app: AppHandle, state: tauri::State<'_, RenderManager>, job_id: String) -> Result<(), String> {
    let mut previous_status = None;
    {
        let mut jobs = state.jobs.lock().unwrap();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) {
            previous_status = Some(job.status.clone());
            job.status = "Queued".to_string();
            job.progress = 0;
            job.speed = String::new();
            job.error_log = None;
            job.cancel_flag = Arc::new(AtomicBool::new(false));
        }
    }
    log_markdown(&format!(
        "[render] reset_render_job {} (was {:?}) -> Queued",
        job_id, previous_status
    ));
    emit_jobs_snapshot(&app, &state.jobs);

    if !state.is_rendering.swap(true, Ordering::SeqCst) {
        let config = state.last_config.lock().unwrap().clone();
        match config {
            Some(config) => {
                log_markdown(&format!("[render] reset_render_job {} resumed the scheduler", job_id));
                state.global_cancel.store(false, Ordering::SeqCst);
                if state.wake_lock.lock().unwrap().is_none() {
                    *state.wake_lock.lock().unwrap() = hold_render_wake_lock();
                }
                spawn_scheduler(app, state.handles(), config);
            }
            None => {
                log_markdown(&format!(
                    "[render] reset_render_job {} could not resume: no last_config to schedule against",
                    job_id
                ));
                state.is_rendering.store(false, Ordering::SeqCst);
            }
        }
    } else {
        log_markdown(&format!(
            "[render] reset_render_job {} left as Queued — a scheduler is already running and will pick it up",
            job_id
        ));
    }

    Ok(())
}

#[tauri::command]
pub fn get_export_pool_free_gb(directories: Vec<String>) -> f64 {
    let mut total: u64 = 0;
    for dir in &directories {
        let free = native::sys::disk::get_available_bytes(&PathBuf::from(dir));
        if free != u64::MAX {
            total += free;
        }
    }
    total as f64 / 1_073_741_824.0
}

#[derive(Serialize)]
pub struct RenderAutosaveSummary {
    pub source_folder: String,
    pub pending_count: usize,
    pub completed_count: usize,
}

#[tauri::command]
pub fn check_render_autosave(state: tauri::State<'_, RenderManager>) -> Option<RenderAutosaveSummary> {
    // A render can legitimately still be running when this fires: pressing
    // F5 reloads the frontend only, not this Rust process, so the autosave
    // file (written continuously as jobs finish, not just at batch end)
    // genuinely exists on disk even though nothing was actually interrupted
    // — the "recovery" prompt was firing on every reload of an active batch.
    // Only treat it as a real interruption when nothing is actively running.
    if state.is_rendering.load(Ordering::SeqCst) {
        return None;
    }
    let json = std::fs::read_to_string(autosave_path()).ok()?;
    let session: RenderSessionData = serde_json::from_str(&json).ok()?;
    let pending_count = session.jobs.iter().filter(|j| j.status == AutosaveJobStatus::Pending).count();
    let completed_count = session.jobs.iter().filter(|j| j.status == AutosaveJobStatus::Completed).count();
    Some(RenderAutosaveSummary { source_folder: session.source_folder, pending_count, completed_count })
}

#[tauri::command]
pub fn discard_render_autosave() -> Result<(), String> {
    match std::fs::remove_file(autosave_path()) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// Repopulates the job table from `.render_autosave.json` — Completed jobs
/// show as "Finished"/100%, Pending ones as "Queued"/0%, both via stub
/// `ClipData` (the snapshot only stores take_folder/name/status, not the
/// full scanner output). Matches dev's own recovery flow exactly
/// (`main.rs`'s render-recovery modal rebuilds the same kind of stub with
/// blank stream/frames/date — dev never had richer data to recover either).
/// Does not auto-start rendering; the user still clicks Start Render.
#[tauri::command]
pub fn recover_render_batch(state: tauri::State<'_, RenderManager>) -> Result<Vec<RenderJobView>, String> {
    let json = std::fs::read_to_string(autosave_path()).map_err(|e| e.to_string())?;
    let session: RenderSessionData = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    // The autosave snapshot only ever recorded one session-wide codec/fps
    // (written once at batch start, before per-job settings existed) — every
    // recovered job gets that same pair. A job individually changed before a
    // crash — via `reset_render_job`'s codec/fps carry-over, or via
    // `set_render_job_codec`'s "Skip" toggle — won't recover with that
    // override; a known, pre-existing recovery-fidelity gap (see the doc
    // comment below), not a regression from per-job settings. A recovered
    // "Skip" job simply comes back as whatever `session.target_codec` was
    // for the batch, and needs re-toggling by hand if that was Skip.
    let recovered_codec = RenderCodec::from_str_id(&session.target_codec);
    let recovered_fps = session.fps;

    let jobs: Vec<RenderJobRuntime> = session.jobs.iter().enumerate().map(|(i, rj)| {
        let (status, progress) = if rj.status == AutosaveJobStatus::Completed {
            ("Finished".to_string(), 100u32)
        } else {
            ("Queued".to_string(), 0u32)
        };
        RenderJobRuntime {
            id: i.to_string(),
            clip: ClipData {
                take_folder: rj.take_folder.clone(),
                clip_type: "single".to_string(),
                img_folder: String::new(),
                // The autosave snapshot stores take_folder/name/status only, so
                // this reconstruction cannot know which kind of take it was —
                // wav-and-frames or OBS-shaped — the same reason `img_folder`
                // and `video_file` are blank below. Guessing `Some("sound.wav")`
                // here used to be harmless when every take had one; now that an
                // OBS take legitimately has none, guessing would misrepresent
                // it. `None` matches the rest of this stub's "unknown, a
                // re-scan is what fills it in" treatment, and fails at
                // `run_render_job`'s clear "no audio source" guard instead of a
                // misleading "sound.wav not found" if resumed without a rescan.
                wav_file: None,
                base_name: rj.name.clone(),
                frame_count: 0,
                date: String::new(),
                video_file: None,
                alpha_folder: None,
            },
            status,
            speed: String::new(),
            progress,
            error_log: None,
            output_path: rj.output_path.clone(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            codec: recovered_codec,
            fps: recovered_fps,
        }
    }).collect();

    let views: Vec<RenderJobView> = jobs.iter().map(RenderJobRuntime::to_view).collect();

    *state.jobs.lock().unwrap() = jobs;
    *state.render_session.lock().unwrap() = Some(session);
    let _ = std::fs::remove_file(autosave_path());

    Ok(views)
}
