// desktop-studio/src-tauri/src/render_manager.rs

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use native::hlcr::scanner::{scan_folder_background, ClipData};
use tauri::Emitter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedRenderJob {
    pub take_folder: String,
    pub clip_type: String,
    pub img_folder: String,
    pub wav_file: String,
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

pub struct RenderManager {
    pub is_running: Arc<AtomicBool>,
    pub cancel_token: Arc<AtomicBool>,
    pub active_job_index: Arc<AtomicU32>,
    pub total_jobs_count: Arc<AtomicU32>,
    pub current_status: Arc<Mutex<String>>,
}

impl RenderManager {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(AtomicBool::new(false)),
            cancel_token: Arc::new(AtomicBool::new(false)),
            active_job_index: Arc::new(AtomicU32::new(0)),
            total_jobs_count: Arc::new(AtomicU32::new(0)),
            current_status: Arc::new(Mutex::new("Idle".to_string())),
        }
    }
}

#[tauri::command]
pub async fn scan_render_directories(paths: Vec<String>) -> Result<Vec<SerializedRenderJob>, String> {
    tokio::task::spawn_blocking(move || {
        let source_folders: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        let (clip_tx, clip_rx) = std::sync::mpsc::channel();
        let (status_tx, _status_rx) = std::sync::mpsc::channel();

        // scan_folder_background is blocking and exhausts sends before returning.
        // try_recv is therefore race-free here.
        scan_folder_background(source_folders, clip_tx, status_tx);

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

/// Expanded payload that carries all render-configuration fields the legacy
/// `RenderConfig` / `get_codec_preset()` path requires.  The old three-field
/// payload (`output_format`, `crf`, `preset`) is superseded by this struct;
/// `ipc_bridge.js` and `render_pane.js` must be updated in parallel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderBatchPayload {
    /// Root directories to scan for HLCR take folders.
    pub render_directories: Vec<String>,
    /// Target codec: "prores" | "dnxhr" | "h264"  (maps to get_codec_preset)
    pub codec: String,
    /// Capture / source framerate (default 300 to match capture FPS).
    pub fps: u32,
    /// Optional absolute path to ffmpeg.exe; falls back to bundled then PATH.
    pub ffmpeg_path: Option<String>,
    /// Optional absolute path to write finished files; defaults to take_folder parent.
    pub export_directory: Option<String>,
}

/// Resolve the FFmpeg binary path using the same fallback chain as the legacy
/// `settings::resolve_ffmpeg_path()`: override → bundled local → system PATH.
fn resolve_ffmpeg(override_path: Option<&String>) -> PathBuf {
    if let Some(p) = override_path {
        let pb = PathBuf::from(p);
        if !p.trim().is_empty() && pb.exists() {
            return pb;
        }
    }
    // Bundled local binary adjacent to the Tauri executable.
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

/// Returns the codec CLI arguments and output file extension for each
/// codec name string — mirrors `native::hlcr::config::get_codec_preset()`.
fn codec_args_and_ext(codec: &str) -> (Vec<String>, &'static str) {
    match codec {
        "dnxhr" | "dnxhd" => (
            vec![
                "-c:v".into(), "dnxhd".into(),
                "-profile:v".into(), "dnxhr_hq".into(),
                "-pix_fmt".into(), "yuv422p".into(),
            ],
            ".mov",
        ),
        "h264" => (
            vec![
                "-c:v".into(), "libx264".into(),
                "-preset".into(), "fast".into(),
                "-crf".into(), "16".into(),
                "-pix_fmt".into(), "yuv420p".into(),
            ],
            ".mp4",
        ),
        // Default: ProRes 422 HQ (matches dev RenderCodec::ProRes)
        _ => (
            vec![
                "-c:v".into(), "prores".into(),
                "-profile:v".into(), "3".into(),
                "-pix_fmt".into(), "yuv422p10le".into(),
            ],
            ".mov",
        ),
    }
}

#[tauri::command]
pub async fn execute_render_batch(
    app: tauri::AppHandle,
    state: tauri::State<'_, RenderManager>,
    payload: RenderBatchPayload,
) -> Result<(), String> {
    if state.is_running.swap(true, Ordering::SeqCst) {
        return Err("Render batch already in progress".to_string());
    }
    
    state.cancel_token.store(false, Ordering::SeqCst);
    state.active_job_index.store(0, Ordering::SeqCst);
    state.total_jobs_count.store(0, Ordering::SeqCst);
    *state.current_status.lock().unwrap() = "Scanning for takes...".to_string();

    let cancel_token = Arc::clone(&state.cancel_token);
    let is_running = Arc::clone(&state.is_running);
    let active_job_index = Arc::clone(&state.active_job_index);
    let total_jobs_count = Arc::clone(&state.total_jobs_count);
    let current_status = Arc::clone(&state.current_status);
    let app_emitter = app.clone();

    tokio::spawn(async move {
        let _ = app_emitter.emit("render_status", serde_json::json!({ "progress": 5, "status": "Scanning for takes..." }));

        let (clip_tx, clip_rx) = std::sync::mpsc::channel();
        let (status_tx, _status_rx) = std::sync::mpsc::channel();
        let source_folders: Vec<PathBuf> = payload.render_directories.iter().map(PathBuf::from).collect();

        scan_folder_background(source_folders, clip_tx, status_tx);

        let mut jobs: Vec<ClipData> = Vec::new();
        while let Ok(clip) = clip_rx.try_recv() {
            jobs.push(clip);
        }

        let total = jobs.len() as u32;
        total_jobs_count.store(total, Ordering::SeqCst);
        if total == 0 {
            is_running.store(false, Ordering::SeqCst);
            let msg = "No takes found to render".to_string();
            *current_status.lock().unwrap() = msg.clone();
            let _ = app_emitter.emit("render_status", serde_json::json!({ "progress": 0, "status": msg }));
            return;
        }

        let ffmpeg_bin = resolve_ffmpeg(payload.ffmpeg_path.as_ref());
        let fps_str = payload.fps.max(1).to_string();
        let (codec_args, codec_ext) = codec_args_and_ext(&payload.codec);

        for (idx, clip) in jobs.into_iter().enumerate() {
            if cancel_token.load(Ordering::SeqCst) {
                break;
            }

            active_job_index.store((idx + 1) as u32, Ordering::SeqCst);
            // Reserve the final 10% for the completion tick.
            let pct = (((idx + 1) as f32 / total as f32) * 90.0) as u32;
            let status_msg = format!("Rendering {}/{} ({})", idx + 1, total, clip.base_name);
            *current_status.lock().unwrap() = status_msg.clone();
            
            let progress_payload = serde_json::json!({
                "progress": pct,
                "status": status_msg,
                "current_frame": idx + 1,
                "total_frames": total
            });
            let _ = app_emitter.emit("render_status", progress_payload);

            let take_folder_path = PathBuf::from(&clip.take_folder);
            let img_folder_path = take_folder_path.join(&clip.img_folder);
            let input_img_pattern = img_folder_path.join("%05d.bmp");
            let wav_path = take_folder_path.join(&clip.wav_file);

            let output_dir = if let Some(ref dir) = payload.export_directory {
                PathBuf::from(dir)
            } else {
                take_folder_path.parent().unwrap_or(&take_folder_path).to_path_buf()
            };
            let output_file = output_dir.join(format!("{}{}", clip.base_name, codec_ext));

            let mut cmd = std::process::Command::new(&ffmpeg_bin);
            cmd.arg("-y")
               .arg("-probesize").arg("32")
               .arg("-analyzeduration").arg("0")
               .arg("-thread_queue_size").arg("512")
               .arg("-framerate").arg(&fps_str)
               .arg("-i").arg(&input_img_pattern)
               .arg("-thread_queue_size").arg("512")
               .arg("-i").arg(&wav_path);

            for arg in &codec_args {
                cmd.arg(arg);
            }

            cmd.arg("-c:a").arg("pcm_s16le")
               .arg("-shortest")
               .arg("-progress").arg("pipe:1")
               .arg("-loglevel").arg("error")
               .arg(&output_file);

            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x08000000);
            }

            match cmd.spawn() {
                Ok(mut child) => {
                    loop {
                        if cancel_token.load(Ordering::SeqCst) {
                            let _ = child.kill();
                            let _ = child.wait();
                            let cancelled_msg = format!("FFmpeg cancelled on clip {}", clip.base_name);
                            *current_status.lock().unwrap() = cancelled_msg.clone();
                            let _ = app_emitter.emit("render_status", serde_json::json!({ "progress": pct, "status": cancelled_msg }));
                            break;
                        }
                        match child.try_wait() {
                            Ok(Some(exit_status)) => {
                                if !exit_status.success() {
                                    let code = exit_status.code().unwrap_or(-1);
                                    let err_msg = format!("FFmpeg failed (exit {}) on clip {}", code, clip.base_name);
                                    log::error!("{}", err_msg);
                                    *current_status.lock().unwrap() = err_msg.clone();
                                    let _ = app_emitter.emit("render_status", serde_json::json!({ "progress": pct, "status": err_msg }));
                                }
                                break;
                            }
                            Ok(None) => {
                                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                            }
                            Err(e) => {
                                log::error!("Error waiting for FFmpeg on {}: {}", clip.base_name, e);
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to spawn FFmpeg for {}: {}", clip.base_name, e);
                    let err_msg = if e.kind() == std::io::ErrorKind::NotFound {
                        "FFmpeg not found. Install FFmpeg or set a custom path in Settings.".to_string()
                    } else {
                        format!("FFmpeg spawn error on clip {}: {}", clip.base_name, e)
                    };
                    *current_status.lock().unwrap() = err_msg.clone();
                    let _ = app_emitter.emit("render_status", serde_json::json!({ "progress": pct, "status": err_msg }));
                    continue;
                }
            }
        }

        is_running.store(false, Ordering::SeqCst);
        let is_cancelled = cancel_token.load(Ordering::SeqCst);
        let final_status = if is_cancelled { "Cancelled".to_string() } else { "Finished".to_string() };
        *current_status.lock().unwrap() = final_status.clone();
        let final_pct = if is_cancelled { 0u32 } else { 100u32 };
        let _ = app_emitter.emit("render_status", serde_json::json!({ "progress": final_pct, "status": final_status }));
    });

    Ok(())
}

#[tauri::command]
pub fn render_status(state: tauri::State<'_, RenderManager>) -> String {
    if state.is_running.load(Ordering::SeqCst) {
        let active = state.active_job_index.load(Ordering::SeqCst);
        let total = state.total_jobs_count.load(Ordering::SeqCst);
        if total > 0 {
            format!("Rendering {}/{}", active, total)
        } else {
            state.current_status.lock().unwrap().clone()
        }
    } else {
        state.current_status.lock().unwrap().clone()
    }
}

#[tauri::command]
pub async fn cancel_render_batch(state: tauri::State<'_, RenderManager>) -> Result<(), String> {
    state.cancel_token.store(true, Ordering::SeqCst);
    *state.current_status.lock().unwrap() = "Cancelling...".to_string();
    Ok(())
}
