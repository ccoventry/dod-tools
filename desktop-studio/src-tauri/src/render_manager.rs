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

        // Perform the scan using the updated scanner
        scan_folder_background(source_folders, clip_tx, status_tx);

        let mut results = Vec::new();
        while let Ok(clip) = clip_rx.try_recv() {
            results.push(SerializedRenderJob::from(clip));
        }

        // Apply deterministic sorting to the final collection
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderBatchPayload {
    pub render_directories: Vec<String>,
    pub output_format: String,
    pub crf: u8,
    pub preset: String,
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
        let source_folders: Vec<PathBuf> = payload.render_directories.into_iter().map(PathBuf::from).collect();

        scan_folder_background(source_folders, clip_tx, status_tx);

        let mut jobs = Vec::new();
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

        for (idx, clip) in jobs.into_iter().enumerate() {
            if cancel_token.load(Ordering::SeqCst) {
                break;
            }

            active_job_index.store((idx + 1) as u32, Ordering::SeqCst);
            let pct = (((idx + 1) as f32 / total as f32) * 100.0) as u32;
            let status_msg = format!("Rendering {}/{} ({})", idx + 1, total, clip.base_name);
            {
                let mut status = current_status.lock().unwrap();
                *status = status_msg.clone();
            }
            let progress_payload = serde_json::json!({
                "progress": pct,
                "status": status_msg,
                "current_frame": idx + 1,
                "total_frames": total
            });
            let _ = app_emitter.emit("render_status", progress_payload);

            let input_img_pattern = PathBuf::from(&clip.img_folder).join("%05d.tga");
            let output_file = PathBuf::from(&clip.take_folder).with_extension(&payload.output_format);

            let mut cmd = std::process::Command::new("ffmpeg");
            cmd.arg("-y")
               .arg("-framerate").arg("60")
               .arg("-i").arg(input_img_pattern)
               .arg("-i").arg(&clip.wav_file)
               .arg("-c:v").arg("libx264")
               .arg("-crf").arg(payload.crf.to_string())
               .arg("-preset").arg(&payload.preset)
               .arg("-c:a").arg("aac")
               .arg("-pix_fmt").arg("yuv420p")
               .arg(&output_file);

            match cmd.spawn() {
                Ok(mut child) => {
                    match child.wait() {
                        Ok(status) => {
                            if !status.success() {
                                log::error!("FFmpeg process exited with non-zero status for {}", clip.base_name);
                            }
                        }
                        Err(e) => {
                            log::error!("Error waiting for FFmpeg process on {}: {}", clip.base_name, e);
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to spawn FFmpeg process for {}: {}", clip.base_name, e);
                    let err_msg = format!("FFmpeg spawn error on clip {}: {}", clip.base_name, e);
                    {
                        let mut status = current_status.lock().unwrap();
                        *status = err_msg.clone();
                    }
                    let _ = app_emitter.emit("render_status", serde_json::json!({ "progress": pct, "status": err_msg }));
                    continue;
                }
            }
        }

        is_running.store(false, Ordering::SeqCst);
        let is_cancelled = cancel_token.load(Ordering::SeqCst);
        let final_status = if is_cancelled {
            "Cancelled".to_string()
        } else {
            "Finished".to_string()
        };
        {
            let mut status = current_status.lock().unwrap();
            *status = final_status.clone();
        }
        let final_pct = if is_cancelled { 0 } else { 100 };
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
