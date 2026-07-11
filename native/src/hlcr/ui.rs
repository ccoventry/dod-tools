#![cfg(not(target_arch = "wasm32"))]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::path::PathBuf;
use egui::{Ui, Color32, Layout, Align, Grid};
use egui_extras::{TableBuilder, Column};

use super::config::{RenderConfig, load_config, save_config};
use super::scanner::{ClipData, scan_folder_background};
use super::renderer::{run_render_job, RenderUpdate};

pub use super::autosave::{RenderJob, RenderJobStatus, RenderSessionData};

pub struct RenderJobState {
    pub id: String,
    pub name: String,
    pub stream: String,
    pub frames: usize,
    pub date: String,
    pub status: String,
    pub speed: String,
    pub progress: u32,
    pub error_log: Option<String>,
    pub cancel_flag: Arc<AtomicBool>,
    /// Output file path, populated once FFmpeg finishes successfully.
    /// Used to update the `.render_autosave.json` lockfile.
    pub resolved_output_path: Option<String>,
}

pub struct HlcrState {
    pub config: RenderConfig,
    pub clips: Vec<ClipData>,
    pub jobs: Vec<RenderJobState>,
    pub is_scanning: bool,
    pub is_rendering: bool,
    pub status_message: String,
    pub active_modal_job_id: Option<String>,
    pub auto_render: bool,

    // Render autosave state — written at batch start, updated per-job,
    // deleted on clean completion.
    pub render_session: Option<RenderSessionData>,

    // Scanner channels
    pub clip_rx: Option<mpsc::Receiver<ClipData>>,
    pub status_rx: Option<mpsc::Receiver<String>>,
    pub scan_thread: Option<std::thread::JoinHandle<usize>>,

    // Render channels
    pub render_tx: mpsc::Sender<RenderUpdate>,
    pub render_rx: mpsc::Receiver<RenderUpdate>,

    // File pickers
    pub ffmpeg_picker: egui_file_dialog::FileDialog,
    pub source_picker: egui_file_dialog::FileDialog,
    pub output_picker: egui_file_dialog::FileDialog,

    pub cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub wake_lock: Option<keepawake::KeepAwake>,
}

impl Default for HlcrState {
    fn default() -> Self {
        Self::new(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)))
    }
}

impl HlcrState {
    pub fn new(cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
        let (render_tx, render_rx) = mpsc::channel();
        Self {
            config: load_config(),
            clips: Vec::new(),
            jobs: Vec::new(),
            is_scanning: false,
            is_rendering: false,
            status_message: "Idle / Waiting for Scan".to_string(),
            active_modal_job_id: None,
            auto_render: false,
            render_session: None,
            clip_rx: None,
            status_rx: None,
            scan_thread: None,
            render_tx,
            render_rx,
            ffmpeg_picker: egui_file_dialog::FileDialog::default().add_file_filter_extensions("Executables", vec!["exe"]),
            source_picker: egui_file_dialog::FileDialog::default(),
            output_picker: egui_file_dialog::FileDialog::default(),
            cancel_flag,
            wake_lock: None,
        }
    }
}

impl HlcrState {
    pub fn start_scan(&mut self) {
        if self.is_scanning {
            return;
        }

        let source_dir = PathBuf::from(&self.config.source_folder);
        if !source_dir.exists() || !source_dir.is_dir() {
            self.status_message = "Error: Invalid source folder".to_string();
            return;
        }

        self.clips.clear();
        self.jobs.clear();
        self.is_scanning = true;
        self.status_message = "Scanning source folder...".to_string();

        let (clip_tx, clip_rx) = mpsc::channel();
        let (status_tx, status_rx) = mpsc::channel();

        self.clip_rx = Some(clip_rx);
        self.status_rx = Some(status_rx);

        let source_dir_clone = source_dir.clone();
        let handle = std::thread::spawn(move || {
            scan_folder_background(source_dir_clone, clip_tx, status_tx)
        });
        self.scan_thread = Some(handle);
    }

    pub fn start_rendering(&mut self) {
        if self.jobs.is_empty() {
            return;
        }

        // Reset completed status values
        for job in &mut self.jobs {
            if job.status == "Finished" || job.status == "Error" {
                job.status = "Queued".to_string();
                job.progress = 0;
                job.speed = "".to_string();
                job.error_log = None;
                job.cancel_flag = Arc::new(AtomicBool::new(false));
                job.resolved_output_path = None;
            }
        }

        let _ = save_config(&self.config);
        self.is_rendering = true;
        self.status_message = "Starting parallel render queue...".to_string();

        // ── Write render autosave lockfile ────────────────────────────────────
        // Pre-collect take_folder by job index to avoid double-borrow on self.
        let take_folders: Vec<String> = self.clips.iter()
            .map(|c| c.take_folder.clone())
            .collect();

        let session = RenderSessionData {
            source_folder: self.config.source_folder.clone(),
            fps: self.config.fps,
            target_codec: format!("{:?}", self.config.target_codec),
            jobs: self.jobs.iter().enumerate().map(|(i, j)| RenderJob {
                take_folder: take_folders.get(i).cloned().unwrap_or_default(),
                output_path: String::new(), // resolved when FFmpeg exits successfully
                status: RenderJobStatus::Pending,
                name: j.name.clone(),
            }).collect(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&session) {
            let path = crate::shared::paths::get_appdata_dir().join(".render_autosave.json");
            if let Err(e) = std::fs::write(&path, &json) {
                log::warn!("[render_autosave] Failed to write lockfile: {}", e);
            } else {
                log::info!("[render_autosave] Lockfile written");
            }
        }
        self.render_session = Some(session);

        self.wake_lock = keepawake::Builder::default()
            .display(false) // We only need the system to stay awake, not the monitors
            .idle(true)
            .sleep(true)
            .create()
            .ok();
    }

    pub fn cancel_all(&mut self) {
        self.is_rendering = false;
        self.status_message = "Render queue cancelled.".to_string();
        self.wake_lock = None;

        for job in &mut self.jobs {
            if job.status == "Rendering" || job.status == "Queued" {
                job.cancel_flag.store(true, Ordering::Relaxed);
                if job.status == "Queued" {
                    job.status = "Cancelled".to_string();
                }
            }
        }
    }

    pub fn cancel_job(&mut self, job_id: &str) {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == job_id) {
            job.cancel_flag.store(true, Ordering::Relaxed);
            if job.status == "Queued" {
                job.status = "Cancelled".to_string();
            }
        }
    }

    pub fn reset_job(&mut self, job_id: &str) {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == job_id) {
            job.status = "Queued".to_string();
            job.progress = 0;
            job.speed = "".to_string();
            job.error_log = None;
            job.cancel_flag = Arc::new(AtomicBool::new(false));
        }
    }

    pub fn update_channels(&mut self, ctx: &egui::Context) {
        // Cascade global cancellation signal to all active render jobs
        if self.cancel_flag.load(Ordering::Relaxed) && self.is_rendering {
            self.cancel_all();
        }

        // Update file pickers
        self.ffmpeg_picker.update(ctx);
        if let Some(path) = self.ffmpeg_picker.take_picked() {
            self.config.ffmpeg_path = path.to_string_lossy().into_owned();
            let _ = save_config(&self.config);
        }

        self.source_picker.update(ctx);
        if let Some(path) = self.source_picker.take_picked() {
            self.config.source_folder = path.to_string_lossy().into_owned();
            let _ = save_config(&self.config);
        }

        self.output_picker.update(ctx);
        if let Some(path) = self.output_picker.take_picked() {
            self.config.primary_export_dir = Some(path.to_path_buf());
            let _ = save_config(&self.config);
        }

        // Poll scanner channels
        let mut _finished_scan = false;
        if let Some(ref rx) = self.clip_rx {
            while let Ok(clip) = rx.try_recv() {
                let id = self.jobs.len().to_string();
                let job = RenderJobState {
                    id: id.clone(),
                    name: clip.base_name.clone(),
                    stream: if clip.clip_type == "hud_only" {
                        "HUD ONLY".to_string()
                    } else {
                        clip.img_folder.clone()
                    },
                    frames: clip.frame_count,
                    date: clip.date.clone(),
                    status: "Queued".to_string(),
                    speed: "".to_string(),
                    progress: 0,
                    error_log: None,
                    cancel_flag: Arc::new(AtomicBool::new(false)),
                    resolved_output_path: None,
                };
                self.clips.push(clip);
                self.jobs.push(job);
                ctx.request_repaint();
            }
        }

        if let Some(ref rx) = self.status_rx {
            while let Ok(msg) = rx.try_recv() {
                self.status_message = msg;
                ctx.request_repaint();
            }
        }

        // Check if scanner thread finished
        if self.is_scanning {
            let mut is_done = false;
            if let Some(ref handle) = self.scan_thread {
                if handle.is_finished() {
                    is_done = true;
                }
            }
            if is_done {
                if let Some(handle) = self.scan_thread.take() {
                    if let Ok(total) = handle.join() {
                        self.status_message = format!("Scan complete. Discovered {} clips ready to render.", total);
                    }
                }
                self.clip_rx = None;
                self.status_rx = None;
                self.is_scanning = false;
                if self.auto_render {
                    self.auto_render = false;
                    self.start_rendering();
                }
                ctx.request_repaint();
            }
        }

        // Poll render channel
        while let Ok(update) = self.render_rx.try_recv() {
            match update {
                RenderUpdate::Progress(id, pct) => {
                    if let Some(job) = self.jobs.iter_mut().find(|j| j.id == id) {
                        job.progress = pct;
                    }
                }
                RenderUpdate::Speed(id, speed) => {
                    if let Some(job) = self.jobs.iter_mut().find(|j| j.id == id) {
                        job.speed = speed;
                    }
                }
                RenderUpdate::Status(id, status) => {
                    if let Some(job) = self.jobs.iter_mut().find(|j| j.id == id) {
                        job.status = status;
                    }
                }
                RenderUpdate::Finished(id, success, err_log) => {
                    if let Some(job) = self.jobs.iter_mut().find(|j| j.id == id) {
                        if job.status == "Rendering" {
                            if err_log.is_some() {
                                job.status = "Error".to_string();
                            } else {
                                job.status = "Finished".to_string();
                                job.progress = 100;
                            }
                        }
                        job.error_log = err_log;

                        // ── Update autosave: mark job Completed on success ──────
                        if success {
                            if let Some(ref mut session) = self.render_session {
                                // Match by name (id order == session.jobs order).
                                let idx: usize = id.parse().unwrap_or(usize::MAX);
                                if let Some(rj) = session.jobs.get_mut(idx) {
                                    rj.status = RenderJobStatus::Completed;
                                    if let Some(ref out) = job.resolved_output_path {
                                        rj.output_path = out.clone();
                                    }
                                }
                                let path = crate::shared::paths::get_appdata_dir()
                                    .join(".render_autosave.json");
                                if let Ok(json) = serde_json::to_string_pretty(session) {
                                    let _ = std::fs::write(&path, &json);
                                }
                            }
                        }
                    }
                }
            }
            ctx.request_repaint();
        }

        // Schedule rendering jobs
        if self.is_rendering {
            let active_count = self.jobs.iter().filter(|j| j.status == "Rendering").count();
            let mut started_any = false;

            if active_count < self.config.max_concurrent_renders {
                let limit = self.config.max_concurrent_renders - active_count;
                // Count queued jobs that will actually start this tick so we can
                // calculate the true concurrent count for thread allocation.
                let queued_count = self.jobs.iter().filter(|j| j.status == "Queued").count();
                let jobs_starting = queued_count.min(limit);
                let mut started = 0;

                for i in 0..self.jobs.len() {
                    if started >= limit {
                        break;
                    }
                    if self.jobs[i].status == "Queued" {
                        let job = &mut self.jobs[i];
                        let cancel_flag = Arc::new(AtomicBool::new(false));
                        job.cancel_flag = cancel_flag.clone();
                        job.status = "Rendering".to_string();

                        let job_id = job.id.clone();
                        let clip = self.clips[i].clone();
                        let tx = self.render_tx.clone();

                        // Use the real concurrent count (already-running + newly starting),
                        // capped at max_concurrent_renders.  This ensures a lone job gets
                        // all available CPU threads instead of only 1/max of them.
                        let effective_concurrent = (active_count + jobs_starting)
                            .min(self.config.max_concurrent_renders)
                            .max(1);
                        let mut config = self.config.clone();
                        config.max_concurrent_renders = effective_concurrent;

                        tokio::spawn(async move {
                            run_render_job(job_id, clip, config, tx, cancel_flag).await;
                        });

                        started += 1;
                        started_any = true;
                    }
                }
            }

            // If we are rendering, but no jobs are active or queued, queue is finished!
            let has_active_or_queued = self.jobs.iter().any(|j| j.status == "Rendering" || j.status == "Queued");
            if !has_active_or_queued {
                self.is_rendering = false;
                self.status_message = "Render queue processing finished.".to_string();

                // ── Clean up render autosave on successful completion ──────────
                let autosave_path = crate::shared::paths::get_appdata_dir()
                    .join(".render_autosave.json");
                if let Err(e) = std::fs::remove_file(&autosave_path) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        log::warn!("[render_autosave] Failed to remove lockfile: {}", e);
                    }
                } else {
                    log::info!("[render_autosave] Lockfile removed after clean completion");
                }
                self.render_session = None;
                self.wake_lock = None;

                ctx.request_repaint();
            } else if started_any {
                ctx.request_repaint();
            }
        }
    }

    pub fn draw_ui(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        self.update_channels(ctx);

        if self.is_rendering {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        ui.vertical(|ui| {
            // 1. Settings Grid
            ui.group(|ui| {
                ui.heading("HLCR Configuration");
                ui.add_space(8.0);

                Grid::new("hlcr_config_grid")
                    .num_columns(3)
                    .spacing([8.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(format!("FFmpeg Path: {}", self.config.ffmpeg_path));
                        ui.label("");
                        ui.label("");
                        ui.end_row();

                        ui.label("Source Folder:");
                        ui.add(egui::TextEdit::singleline(&mut self.config.source_folder).desired_width(800.0));
                        if ui.button("Browse...").clicked() {
                            self.source_picker.pick_directory();
                        }
                        ui.end_row();

                    });

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label("Frame Rate (FPS):");
                    let mut fps_str = self.config.fps.to_string();
                    if ui.add(egui::TextEdit::singleline(&mut fps_str).desired_width(50.0)).changed() {
                        if let Ok(new_fps) = fps_str.parse::<u32>() {
                            self.config.fps = new_fps;
                        }
                    }

                    ui.add_space(16.0);


                    ui.label("Max Concurrent Renders:");
                    ui.add(egui::DragValue::new(&mut self.config.max_concurrent_renders).range(1..=8));
                });
            });

            ui.add_space(10.0);

            // 2. Control Buttons
            ui.horizontal(|ui| {
                if ui.button("Scan Folder").clicked() {
                    self.start_scan();
                }

                if ui.button("Start Render").clicked() {
                    self.start_rendering();
                }

                if ui.button("Cancel All").clicked() {
                    self.cancel_all();
                }

                ui.add_space(20.0);
                ui.weak(&self.status_message);
            });

            ui.add_space(10.0);

            // 3. Queue Table
            let table_height = ui.available_height() - 24.0;
            TableBuilder::new(ui)
                .striped(true)
                .max_scroll_height(table_height)
                .cell_layout(Layout::left_to_right(Align::Center))
                .column(Column::initial(300.0).resizable(true).clip(true)) // Clip Name
                .column(Column::initial(80.0).resizable(true)) // Stream
                .column(Column::initial(80.0).resizable(true)) // Frames
                .column(Column::initial(140.0).resizable(true)) // Date
                .column(Column::initial(80.0).resizable(true)) // Status
                .column(Column::initial(80.0).resizable(true)) // Speed
                .column(Column::initial(150.0).resizable(false)) // Progress
                .column(Column::remainder()) // Actions
                .header(20.0, |mut header| {
                    header.col(|ui| { ui.strong("Clip Name"); });
                    header.col(|ui| { ui.strong("Stream"); });
                    header.col(|ui| { ui.strong("Frames"); });
                    header.col(|ui| { ui.strong("Date"); });
                    header.col(|ui| { ui.strong("Status"); });
                    header.col(|ui| { ui.strong("Speed"); });
                    header.col(|ui| { ui.strong("Progress"); });
                    header.col(|ui| { ui.strong("Actions"); });
                })
                .body(|body| {
                    body.rows(24.0, self.jobs.len(), |mut row| {
                        let idx = row.index();
                        let (job_id, name, stream, frames, date, status, speed, progress, has_error) = {
                            let job = &self.jobs[idx];
                            (
                                job.id.clone(),
                                job.name.clone(),
                                job.stream.clone(),
                                job.frames,
                                job.date.clone(),
                                job.status.clone(),
                                job.speed.clone(),
                                job.progress,
                                job.error_log.is_some(),
                            )
                        };

                        row.col(|ui| { ui.label(&name); });
                        row.col(|ui| { ui.label(&stream); });
                        row.col(|ui| { ui.label(frames.to_string()); });
                        row.col(|ui| { ui.label(&date); });

                        row.col(|ui| {
                            let color = match status.as_str() {
                                "Finished" => Color32::GREEN,
                                "Error" => Color32::RED,
                                "Cancelled" => Color32::YELLOW,
                                "Rendering" => Color32::LIGHT_BLUE,
                                _ => Color32::WHITE,
                            };
                            ui.colored_label(color, &status);
                        });

                        row.col(|ui| { ui.label(&speed); });

                        row.col(|ui| {
                            ui.add(egui::ProgressBar::new(progress as f32 / 100.0).text(format!("{}%", progress)));
                        });

                        row.col(|ui| {
                            ui.horizontal(|ui| {
                                if status == "Rendering" || status == "Queued" {
                                    if ui.button("✖").clicked() {
                                        self.cancel_job(&job_id);
                                    }
                                } else if status == "Cancelled" || status == "Finished" || status == "Error" {
                                    if ui.button("🔄").on_hover_text("Reset to Queued").clicked() {
                                        self.reset_job(&job_id);
                                    }
                                }

                                if has_error {
                                    if ui.button("⚠️ View Log").clicked() {
                                        self.active_modal_job_id = Some(job_id.clone());
                                    }
                                }
                            });
                        });
                    });
                });
        });

        // 4. Modal Window
        if let Some(ref job_id) = self.active_modal_job_id {
            if let Some(job) = self.jobs.iter().find(|j| &j.id == job_id) {
                let mut open = true;
                egui::Window::new("FFmpeg Error Log")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(true)
                    .default_size([600.0, 400.0])
                    .show(ctx, |ui| {
                        ui.heading(&job.name);
                        ui.add_space(8.0);

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            if let Some(ref log) = job.error_log {
                                ui.text_edit_multiline(&mut log.as_str());
                            }
                        });

                        ui.add_space(8.0);
                        if ui.button("Close").clicked() {
                            self.active_modal_job_id = None;
                        }
                    });
                if !open {
                    self.active_modal_job_id = None;
                }
            }
        }
    }
}
