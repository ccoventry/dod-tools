use std::sync::{Arc, Mutex, OnceLock, atomic::{AtomicBool, Ordering}};
use native::patch::{CaptureWorker, PatchEvent, PatcherConfig, CaptureStreak, build_batch_queue, spawn_patch_batch};
use crate::types::QueuedStreakExport;

static WORKER_STATE: OnceLock<Mutex<Option<CaptureWorker>>> = OnceLock::new();
static PROGRESS_MSG: OnceLock<Mutex<String>> = OnceLock::new();
static PROGRESS_PCT: OnceLock<Mutex<f32>> = OnceLock::new();
static SUCCESS_STATE: OnceLock<Mutex<bool>> = OnceLock::new();
static ERROR_STATE: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn get_worker() -> &'static Mutex<Option<CaptureWorker>> {
    WORKER_STATE.get_or_init(|| Mutex::new(None))
}

fn get_progress_msg() -> &'static Mutex<String> {
    PROGRESS_MSG.get_or_init(|| Mutex::new("Idle".to_string()))
}

fn get_progress_pct() -> &'static Mutex<f32> {
    PROGRESS_PCT.get_or_init(|| Mutex::new(0.0))
}

fn get_success() -> &'static Mutex<bool> {
    SUCCESS_STATE.get_or_init(|| Mutex::new(false))
}

fn get_error() -> &'static Mutex<Option<String>> {
    ERROR_STATE.get_or_init(|| Mutex::new(None))
}

pub fn render_patch_ui(ui: &mut egui::Ui, ctx: &egui::Context, export_queue: &mut Vec<QueuedStreakExport>) {
    let mut worker_lock = get_worker().lock().unwrap();
    let mut progress_msg = get_progress_msg().lock().unwrap();
    let mut progress_pct = get_progress_pct().lock().unwrap();
    let mut success = get_success().lock().unwrap();
    let mut error = get_error().lock().unwrap();

    // 3c: Poll the receiver using try_recv()
    let mut should_drop_worker = false;
    if let Some(ref worker) = *worker_lock {
        while let Ok(event) = worker.receiver.try_recv() {
            match event {
                PatchEvent::Starting(total) => {
                    *progress_msg = format!("Starting batch patch of {} jobs...", total);
                    *progress_pct = 0.0;
                    *success = false;
                    *error = None;
                    ctx.request_repaint();
                }
                PatchEvent::Progress(file, pct) => {
                    *progress_msg = format!("Processing: {} ({:.1}%)", file, pct);
                    // Update overall completion estimate or single file completion
                    if pct >= 100.0 {
                        *progress_pct = 1.0;
                    } else {
                        *progress_pct = pct / 100.0;
                    }
                    ctx.request_repaint();
                }
                PatchEvent::Completed => {
                    *progress_msg = "Completed successfully!".to_string();
                    *progress_pct = 1.0;
                    *success = true;
                    *error = None;
                    should_drop_worker = true;
                    ctx.request_repaint();
                }
                PatchEvent::Cancelled => {
                    *progress_msg = "Batch Cancelled".to_string();
                    *progress_pct = 0.0;
                    *success = false;
                    *error = None;
                    should_drop_worker = true;
                    ctx.request_repaint();
                }
                PatchEvent::Error(err_msg) => {
                    *progress_msg = format!("Error occurred: {}", err_msg);
                    *error = Some(err_msg);
                    should_drop_worker = true;
                    ctx.request_repaint();
                }
            }
        }
    }

    if should_drop_worker {
        if let Some(mut worker) = worker_lock.take() {
            if let Some(handle) = worker.handle.take() {
                let _ = handle.join();
            }
        }
    }

    ui.group(|ui| {
        ui.vertical(|ui| {
            ui.heading("⚡ Fast Streaming Patcher");
            ui.add_space(4.0);
            ui.label("Directly patch demo files without deep parsing to bypass long waiting times.");

            ui.add_space(8.0);

            let is_running = worker_lock.as_ref().map(|w| w.is_running).unwrap_or(false);

            // 4a: Start Batch Button
            ui.horizontal(|ui| {
                let btn = egui::Button::new("🎬 Start Direct Batch Patch");
                if ui.add_enabled(!is_running, btn).clicked() {
                    let config = PatcherConfig::default();
                    let raw_streaks: Vec<CaptureStreak> = export_queue.iter()
                        .filter(|item| item.enabled)
                        .map(|item| CaptureStreak {
                            start_tick: (item.start_time * 100.0) as i32,
                            end_tick: (item.stop_time * 100.0) as i32,
                            source_demo: item.input_path.to_string_lossy().to_string(),
                        })
                        .collect();

                    let jobs = build_batch_queue(raw_streaks, &config);
                    if !jobs.is_empty() {
                        let cancel_token = Arc::new(AtomicBool::new(false));
                        let worker = spawn_patch_batch(jobs, config, cancel_token);
                        *worker_lock = Some(worker);
                        *progress_msg = "Spawning worker...".to_string();
                        *progress_pct = 0.0;
                        *success = false;
                        *error = None;
                    } else {
                        *progress_msg = "No enabled items in the queue.".to_string();
                    }
                }

                if is_running {
                    ui.spinner();
                }
            });

            ui.add_space(8.0);

            // 4b: ProgressBar using tracked float state
            ui.add(egui::ProgressBar::new(*progress_pct).text(&*progress_msg));

            if is_running {
                ui.add_space(4.0);
                if ui.button("⏹ Cancel Batch").clicked() {
                    if let Some(ref worker) = *worker_lock {
                        worker.cancel_token.store(true, Ordering::Relaxed);
                    }
                }
            }

            if let Some(ref err_msg) = *error {
                ui.colored_label(egui::Color32::RED, format!("⚠ {}", err_msg));
            }

            if *success {
                ui.colored_label(egui::Color32::GREEN, "✅ Batch patching finished successfully!");
            }
        });
    });
}
