// desktop-studio/src-tauri/src/render_manager.rs

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use native::hlcr::scanner::{scan_folder_background, ClipData};

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
    pub is_running: Arc<Mutex<bool>>,
    pub cancel_token: Arc<AtomicBool>,
    pub current_status: Arc<Mutex<String>>,
}

impl RenderManager {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(Mutex::new(false)),
            cancel_token: Arc::new(AtomicBool::new(false)),
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

#[tauri::command]
pub async fn execute_render_batch(
    state: tauri::State<'_, RenderManager>,
    jobs: Vec<SerializedRenderJob>,
) -> Result<(), String> {
    {
        let mut running = state.is_running.lock().unwrap();
        if *running {
            return Err("Render batch already in progress".to_string());
        }
        *running = true;
    }
    
    state.cancel_token.store(false, Ordering::Relaxed);
    *state.current_status.lock().unwrap() = "Executing...".to_string();

    let cancel_token = Arc::clone(&state.cancel_token);
    let is_running = Arc::clone(&state.is_running);
    let current_status = Arc::clone(&state.current_status);

    tokio::spawn(async move {
        let (update_tx, update_rx) = std::sync::mpsc::channel();
        let config = native::hlcr::config::RenderConfig::default();

        let total_jobs = jobs.len();

        for (idx, job) in jobs.into_iter().enumerate() {
            if cancel_token.load(Ordering::Relaxed) {
                break;
            }

            let clip = ClipData {
                take_folder: job.take_folder.clone(),
                clip_type: job.clip_type.clone(),
                img_folder: job.img_folder.clone(),
                wav_file: job.wav_file.clone(),
                base_name: job.base_name.clone(),
                frame_count: job.frame_count,
                date: job.date.clone(),
            };

            let job_id = format!("job_{}", idx);
            let job_cancel = Arc::new(AtomicBool::new(false));

            let update_tx_clone = update_tx.clone();
            let job_cancel_clone = Arc::clone(&job_cancel);
            let cancel_token_clone = Arc::clone(&cancel_token);

            // Spawn a monitor thread to handle per-job cancellation if the global cancel token is set
            tokio::spawn(async move {
                while !job_cancel_clone.load(Ordering::Relaxed) {
                    if cancel_token_clone.load(Ordering::Relaxed) {
                        job_cancel_clone.store(true, Ordering::Relaxed);
                        break;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            });

            {
                let mut status = current_status.lock().unwrap();
                *status = format!("Rendering {}/{} ({})", idx + 1, total_jobs, job.base_name);
            }

            // Execute the single render job (this async fn runs command in background)
            native::hlcr::renderer::run_render_job(job_id, clip, config.clone(), update_tx_clone, job_cancel).await;

            // Wait for completion or errors for this job
            loop {
                if let Ok(update) = update_rx.try_recv() {
                    match update {
                        native::hlcr::renderer::RenderUpdate::Finished(_, _, _) => {
                            break;
                        }
                        _ => {}
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
        }

        let mut running = is_running.lock().unwrap();
        *running = false;
        let mut status = current_status.lock().unwrap();
        *status = if cancel_token.load(Ordering::Relaxed) {
            "Cancelled".to_string()
        } else {
            "Finished".to_string()
        };
    });

    Ok(())
}

#[tauri::command]
pub fn render_status(state: tauri::State<'_, RenderManager>) -> String {
    state.current_status.lock().unwrap().clone()
}

#[tauri::command]
pub async fn cancel_render_batch(state: tauri::State<'_, RenderManager>) -> Result<(), String> {
    state.cancel_token.store(true, Ordering::Relaxed);
    *state.current_status.lock().unwrap() = "Cancelling...".to_string();
    Ok(())
}
