use std::sync::{Arc, Mutex, OnceLock, atomic::{AtomicBool, Ordering}};
use std::path::PathBuf;

pub use native::log_markdown;

macro_rules! acquire_lock {
    ($mutex:expr) => {
        match $mutex.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                log::error!("Mutex poisoned, attempting recovery...");
                poisoned.into_inner()
            }
        }
    };
}

use native::patch::{
    CaptureWorker, PatchEvent, PatcherConfig, CaptureStreak, build_batch_queue,
    spawn_patch_batch, HighlightRules, scan_demo_for_highlights,
};
use crate::types::{
    QueuedStreakExport, DemoData, HighlightStreak, CaptureStudioState,
};
use egui_extras::{TableBuilder, Column};

static WORKER_STATE: OnceLock<Mutex<Option<CaptureWorker>>> = OnceLock::new();
static PROGRESS_MSG: OnceLock<Mutex<String>> = OnceLock::new();
static PROGRESS_PCT: OnceLock<Mutex<f32>> = OnceLock::new();
static SUCCESS_STATE: OnceLock<Mutex<bool>> = OnceLock::new();
static ERROR_STATE: OnceLock<Mutex<Option<String>>> = OnceLock::new();

// Wizard fields string states to allow parsing None on empty inputs
static MIN_KILLS_STR: OnceLock<Mutex<String>> = OnceLock::new();
static MAX_TIME_GAP_STR: OnceLock<Mutex<String>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureState {
    Idle,
    Scanning(String),
    Ready,
}

// Ingestion Wizard & Discovery State
static HIGHLIGHT_RULES: OnceLock<Mutex<HighlightRules>> = OnceLock::new();
static QUEUED_DEMOS: OnceLock<Arc<Mutex<Arc<Vec<DemoData>>>>> = OnceLock::new();
static CAPTURE_STATE: OnceLock<Mutex<CaptureState>> = OnceLock::new();
static PATCHER_CONFIG: OnceLock<Mutex<PatcherConfig>> = OnceLock::new();

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

fn get_min_kills_str() -> &'static Mutex<String> {
    MIN_KILLS_STR.get_or_init(|| Mutex::new("3".to_string()))
}

fn get_max_time_gap_str() -> &'static Mutex<String> {
    MAX_TIME_GAP_STR.get_or_init(|| Mutex::new("4.0".to_string()))
}

fn get_highlight_rules() -> &'static Mutex<HighlightRules> {
    HIGHLIGHT_RULES.get_or_init(|| Mutex::new(HighlightRules {
        min_kills: Some(3),
        max_time_gap: Some(4.0),
        target_players: Vec::new(),
    }))
}

pub(crate) fn get_queued_demos() -> Arc<Mutex<Arc<Vec<DemoData>>>> {
    QUEUED_DEMOS.get_or_init(|| Arc::new(Mutex::new(Arc::new(Vec::new())))).clone()
}

fn get_capture_state() -> &'static Mutex<CaptureState> {
    CAPTURE_STATE.get_or_init(|| Mutex::new(CaptureState::Idle))
}

pub(crate) fn get_patcher_config() -> &'static Mutex<PatcherConfig> {
    PATCHER_CONFIG.get_or_init(|| Mutex::new(PatcherConfig::default()))
}

// Caching helper not needed as we use the raw flat structure directly now.

pub fn render_patch_ui(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    _export_queue: &mut Vec<QueuedStreakExport>,
    current_state: CaptureStudioState,
    state_ptr: &mut CaptureStudioState,
    tx: std::sync::mpsc::Sender<crate::types::GuiMessage>,
    loading_ptr: &mut bool,
    hide_non_pov: &mut bool,
) {
    {
        let state = acquire_lock!(get_capture_state()).clone();
        if let CaptureState::Scanning(msg) = state {
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.heading("📂 Step 1: Scan & Discover Highlights");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("{}...", msg));
                    });
                });
            });
            return;
        }
    }

    let mut worker_lock = acquire_lock!(get_worker());
    let mut progress_msg = acquire_lock!(get_progress_msg());
    let mut progress_pct = acquire_lock!(get_progress_pct());
    let mut success = acquire_lock!(get_success());
    let mut error = acquire_lock!(get_error());

    let mut rules = acquire_lock!(get_highlight_rules());
    let mut patcher_config = acquire_lock!(get_patcher_config());

    let mut min_kills_str = acquire_lock!(get_min_kills_str());
    let mut max_time_gap_str = acquire_lock!(get_max_time_gap_str());

    // Poll worker receiver
    let mut should_drop_worker = false;
    if let Some(ref worker) = *worker_lock {
        ctx.request_repaint();
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

    // Render Tab View
    match current_state {
        CaptureStudioState::Scan => {
            // STEP 1: SCAN UI
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.heading("📂 Step 1: Scan & Discover Highlights");
                    ui.add_space(4.0);
                    ui.label("Configure highlight rules and scan files/folders to discover streaks dynamically.");
                    ui.add_space(8.0);

                    // Rule Configuration Fields
                    ui.horizontal(|ui| {
                        ui.label("Min Kills:");
                        if ui.text_edit_singleline(&mut *min_kills_str).changed() {
                            let trimmed = min_kills_str.trim();
                            if trimmed.is_empty() {
                                rules.min_kills = None;
                            } else if let Ok(val) = trimmed.parse::<usize>() {
                                rules.min_kills = Some(val);
                            }
                        }

                        ui.add_space(10.0);
                        ui.label("Max Gap (sec):");
                        if ui.text_edit_singleline(&mut *max_time_gap_str).changed() {
                            let trimmed = max_time_gap_str.trim();
                            if trimmed.is_empty() {
                                rules.max_time_gap = None;
                            } else if let Ok(val) = trimmed.parse::<f32>() {
                                rules.max_time_gap = Some(val);
                            }
                        }
                    });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Target Player Filter (comma-separated):");
                        let mut filter_text = rules.target_players.join(", ");
                        if ui.text_edit_singleline(&mut filter_text).changed() {
                            rules.target_players = filter_text
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                        }
                    });

                    ui.add_space(8.0);

                    // Import Buttons (using RFD)
                    ui.horizontal(|ui| {
                        let ingesting = matches!(*get_capture_state().lock().unwrap(), CaptureState::Scanning(_));
                        if ui.add_enabled(!ingesting, egui::Button::new("➕ Add Demo Files")).clicked() {
                            *loading_ptr = true;
                            let ctx_clone = ctx.clone();
                            let rules_clone = rules.clone();
                            let tx_clone = tx.clone();
                            std::thread::Builder::new()
                                .name("rfd_dialog".into())
                                .stack_size(8 * 1024 * 1024)
                                .spawn(move || {
                                    if let Some(files) = rfd::FileDialog::new()
                                        .add_filter("Demo files", &["dem"])
                                        .pick_files()
                                    {
                                        spawn_ingestion_thread(IngestionInput::Files(files), rules_clone, ctx_clone, tx_clone);
                                    } else {
                                        let _ = tx_clone.send(crate::types::GuiMessage::IngestionFinished);
                                    }
                                })
                                .unwrap();
                        }

                        if ui.add_enabled(!ingesting, egui::Button::new("📂 Add Folder")).clicked() {
                            *loading_ptr = true;
                            let ctx_clone = ctx.clone();
                            let rules_clone = rules.clone();
                            let tx_clone = tx.clone();
                            std::thread::Builder::new()
                                .name("rfd_dialog".into())
                                .stack_size(8 * 1024 * 1024)
                                .spawn(move || {
                                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                        spawn_ingestion_thread(IngestionInput::Folder(folder), rules_clone, ctx_clone, tx_clone);
                                    } else {
                                        let _ = tx_clone.send(crate::types::GuiMessage::IngestionFinished);
                                    }
                                })
                                .unwrap();
                        }

                        if ingesting {
                            ui.spinner();
                            ui.weak("Scanning files... (App is responsive)");
                        }
                    });

                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        if ui.button("Proceed to Selection ->").clicked() {
                            log_markdown("UI Interaction: Clicked Proceed to Selection");
                            let queued_arc = get_queued_demos();
                            let queued_guard = acquire_lock!(queued_arc);
                            // [STEP 3] Diagnostic Logging
                            let msg = format!("Transitioning with {} items", queued_guard.len());
                            log::info!("{}", msg);
                            log_markdown(&msg);
                            *state_ptr = CaptureStudioState::Select;
                        }
                    });
                });
            });
        }
        CaptureStudioState::Select => {
            // [STEP 1 & STEP 3] Diagnostic Check
            if *loading_ptr {
                log::info!("UI: State is in Loading");
                ui.label("Loading...");
                return;
            } else {
                let msg = "UI: State transition to DisplayList";
                log::info!("{}", msg);
            }

            let queued_demos_arc = get_queued_demos();

            // 1b. Ensure that no clone() or deep-copy operations are being performed on the collection being moved into the state.
            // We clone the Arc pointer (O(1)) rather than deep copying the underlying vector data.
            let queued_demos_shared = {
                let queued_demos_guard = acquire_lock!(queued_demos_arc);
                queued_demos_guard.clone()
            };
            let data = &*queued_demos_shared;

            // STEP 2: SELECT UI
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.heading("📂 Step 2: Select Highlights & Patch");
                    ui.add_space(4.0);
                    ui.label("Enable streaks, adjust patch parameters, and launch patcher.");
                    ui.add_space(8.0);

                    ui.checkbox(hide_non_pov, "Hide Non-Recording Players in POV Demos");
                    ui.add_space(8.0);

                    // Collapsing Queue of Discovered Streaks
                    if !queued_demos_shared.is_empty() {
                        ui.strong("Discovered Highlight Streaks");
                        ui.add_space(4.0);

                        // Action intent enum — collected during the render loop and applied
                        // strictly AFTER the loop completes to avoid simultaneous iteration
                        // and mutation (anti-crash protocol).
                        enum DemoAction {
                            RemoveDemo(usize),
                            SelectAll(usize),
                            DeselectAll(usize),
                        }
                        let mut actions_to_apply: Vec<DemoAction> = Vec::new();

                        egui::ScrollArea::vertical()
                            .max_height(250.0)
                            .id_salt("discovered_streaks_scroll_tables")
                            .show(ui, |ui| {
                                for (d_idx, demo) in data.iter().enumerate() {
                                    if demo.streaks.is_empty() {
                                        continue;
                                    }

                                    egui::Frame::group(ui.style()).show(ui, |ui| {
                                        // ── Per-demo header row with bulk controls ──────
                                        ui.horizontal(|ui| {
                                            ui.strong(&demo.demo_name);
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                if ui.button("🗑 Remove Demo").on_hover_text("Remove this demo from the queue").clicked() {
                                                    actions_to_apply.push(DemoAction::RemoveDemo(d_idx));
                                                }
                                                ui.add_space(4.0);
                                                if ui.button("Deselect All").clicked() {
                                                    actions_to_apply.push(DemoAction::DeselectAll(d_idx));
                                                }
                                                if ui.button("Select All").clicked() {
                                                    actions_to_apply.push(DemoAction::SelectAll(d_idx));
                                                }
                                            });
                                        });
                                        ui.add_space(2.0);

                                        TableBuilder::new(ui)
                                            .striped(true)
                                            .vscroll(false)
                                            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                                            .column(Column::auto())          // [Checkbox]
                                            .column(Column::auto())          // [Player Name]
                                            .column(Column::exact(110.0))   // [Kill Range]
                                            .column(Column::exact(50.0))    // [Duration]
                                            .column(Column::remainder())     // [Details]
                                            .header(20.0, |mut header| {
                                                header.col(|ui| { ui.strong("Sel"); });
                                                header.col(|ui| { ui.strong("Player"); });
                                                header.col(|ui| { ui.strong("Kill Range"); });
                                                header.col(|ui| { ui.strong("Dur."); });
                                                header.col(|ui| { ui.strong("Details"); });
                                            })
                                            .body(|mut body| {
                                                let filtered_indices: Vec<usize> = (0..demo.streaks.len())
                                                    .filter(|&idx| {
                                                        let streak = &demo.streaks[idx];
                                                        if demo.is_pov && *hide_non_pov {
                                                            Some(streak.player_index) == demo.local_player_index
                                                        } else {
                                                            true
                                                        }
                                                    })
                                                    .collect();

                                                body.rows(20.0, filtered_indices.len(), |mut row| {
                                                    let row_idx = row.index();
                                                    let streak_idx = filtered_indices[row_idx];
                                                    let streak = &demo.streaks[streak_idx];

                                                    // ── [Checkbox] ────────────────────────
                                                    row.col(|ui| {
                                                        let mut is_selected = streak.is_selected;
                                                        if ui.checkbox(&mut is_selected, "").changed() {
                                                            log_markdown(&format!("UI Interaction: Toggled streak selection for {}, new value: {}", streak.target_player, is_selected));
                                                            let queued_arc = get_queued_demos();
                                                            let mut queued_guard = acquire_lock!(queued_arc);
                                                            let queued = Arc::make_mut(&mut *queued_guard);
                                                            queued[d_idx].streaks[streak_idx].is_selected = is_selected;
                                                        }
                                                    });

                                                    // ── [Player Name] ─────────────────────
                                                    row.col(|ui| { ui.label(&streak.target_player); });

                                                    // ── [Kill Range] ──────────────────────
                                                    row.col(|ui| {
                                                        let max_idx = streak.kills.len().saturating_sub(1);
                                                        let is_modified = streak.start_index > 0
                                                            || streak.end_index < max_idx;

                                                        // Copy mutable locals from the read-only snapshot.
                                                        let mut start_idx = streak.start_index;
                                                        let mut end_idx   = streak.end_index;

                                                        ui.horizontal(|ui| {
                                                            // Colour the range values orange when the range is narrowed.
                                                            ui.scope(|ui| {
                                                                if is_modified {
                                                                    ui.visuals_mut().override_text_color =
                                                                        Some(egui::Color32::from_rgb(255, 165, 0));
                                                                }
                                                                let start_changed = ui.add(
                                                                    egui::DragValue::new(&mut start_idx)
                                                                        .range(0..=end_idx)
                                                                        .custom_formatter(|n, _| format!("{}", n as usize + 1))
                                                                        .custom_parser(|s| s.parse::<f64>().ok().map(|v| (v - 1.0).max(0.0)))
                                                                        .speed(0.05),
                                                                ).changed();
                                                                ui.label("-");
                                                                let end_changed = ui.add(
                                                                    egui::DragValue::new(&mut end_idx)
                                                                        .range(start_idx..=max_idx)
                                                                        .custom_formatter(|n, _| format!("{}", n as usize + 1))
                                                                        .custom_parser(|s| s.parse::<f64>().ok().map(|v| (v - 1.0).max(0.0)))
                                                                        .speed(0.05),
                                                                ).changed();

                                                                if start_changed || end_changed {
                                                                    let queued_arc = get_queued_demos();
                                                                    let mut queued_guard = acquire_lock!(queued_arc);
                                                                    let queued = Arc::make_mut(&mut *queued_guard);
                                                                    let sm = &mut queued[d_idx].streaks[streak_idx];
                                                                    sm.start_index = start_idx;
                                                                    sm.end_index   = end_idx;
                                                                    sm.update_visuals();
                                                                }
                                                            });

                                                            // Reset button — only shown when range is narrowed.
                                                            if is_modified && ui.button("↺").on_hover_text("Reset to full range").clicked() {
                                                                let queued_arc = get_queued_demos();
                                                                let mut queued_guard = acquire_lock!(queued_arc);
                                                                let queued = Arc::make_mut(&mut *queued_guard);
                                                                let sm = &mut queued[d_idx].streaks[streak_idx];
                                                                sm.start_index = 0;
                                                                sm.end_index   = sm.kills.len().saturating_sub(1);
                                                                sm.update_visuals();
                                                            }
                                                        });
                                                    });

                                                    // ── [Duration] ────────────────────────
                                                    row.col(|ui| { ui.label(&streak.duration_string); });

                                                    // ── [Details / Timeline] ──────────────
                                                    row.col(|ui| { ui.label(&streak.timeline_string); });
                                                });
                                            });
                                    });

                                    ui.add_space(4.0);
                                }
                            });

                        // ── Execute all deferred actions post-loop ──────────────────────
                        // Process removals last (in reverse index order) to avoid index shift.
                        let mut removals: Vec<usize> = actions_to_apply
                            .iter()
                            .filter_map(|a| if let DemoAction::RemoveDemo(i) = a { Some(*i) } else { None })
                            .collect();
                        removals.sort_unstable_by(|a, b| b.cmp(a));

                        for action in &actions_to_apply {
                            match action {
                                DemoAction::SelectAll(idx) => {
                                    let queued_arc = get_queued_demos();
                                    let mut queued_guard = acquire_lock!(queued_arc);
                                    let queued = Arc::make_mut(&mut *queued_guard);
                                    if let Some(demo) = queued.get_mut(*idx) {
                                        for s in &mut demo.streaks { s.is_selected = true; }
                                    }
                                }
                                DemoAction::DeselectAll(idx) => {
                                    let queued_arc = get_queued_demos();
                                    let mut queued_guard = acquire_lock!(queued_arc);
                                    let queued = Arc::make_mut(&mut *queued_guard);
                                    if let Some(demo) = queued.get_mut(*idx) {
                                        for s in &mut demo.streaks { s.is_selected = false; }
                                    }
                                }
                                DemoAction::RemoveDemo(_) => {} // handled below
                            }
                        }
                        for idx in removals {
                            let queued_arc = get_queued_demos();
                            let mut queued_guard = acquire_lock!(queued_arc);
                            let queued = Arc::make_mut(&mut *queued_guard);
                            if idx < queued.len() {
                                queued.remove(idx);
                            }
                        }

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button("Clear All Discovered").clicked() {
                                let queued_arc = get_queued_demos();
                                  let mut queued_guard = acquire_lock!(queued_arc);
                                  let queued = Arc::make_mut(&mut *queued_guard);
                                  queued.clear();
                            }
                            if ui.button("Select All").clicked() {
                                  let queued_arc = get_queued_demos();
                                  let mut queued_guard = acquire_lock!(queued_arc);
                                  let queued = Arc::make_mut(&mut *queued_guard);
                                  for d in queued.iter_mut() {
                                      for s in &mut d.streaks {
                                          s.is_selected = true;
                                      }
                                  }
                            }
                            if ui.button("Deselect All").clicked() {
                                  let queued_arc = get_queued_demos();
                                  let mut queued_guard = acquire_lock!(queued_arc);
                                  let queued = Arc::make_mut(&mut *queued_guard);
                                  for d in queued.iter_mut() {
                                      for s in &mut d.streaks {
                                          s.is_selected = false;
                                      }
                                  }
                            }
                        });
                    } else {
                        ui.weak("No discovered highlight streaks. Go back to Scan and add demo files.");
                    }
                });
            });

            ui.add_space(10.0);

            // 2. Fast Streaming Patcher Section
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.heading("⚡ Fast Streaming Patcher Settings");
                    ui.add_space(4.0);

                    // Patcher Config settings
                    ui.horizontal(|ui| {
                        ui.label("Pre-roll (sec):");
                        ui.add(egui::DragValue::new(&mut patcher_config.pre_roll_seconds).range(0.0..=10.0).speed(0.1));
                        ui.add_space(10.0);
                        ui.label("Post-roll (sec):");
                        ui.add(egui::DragValue::new(&mut patcher_config.post_roll_seconds).range(0.0..=10.0).speed(0.1));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Record Start Lead (sec):");
                        ui.add(egui::DragValue::new(&mut patcher_config.record_start_lead).range(0.0..=10.0).speed(0.1));
                        ui.add_space(10.0);
                        ui.label("Record Stop Trail (sec):");
                        ui.add(egui::DragValue::new(&mut patcher_config.record_stop_trail).range(0.0..=10.0).speed(0.1));
                    });

                    ui.add_space(8.0);

                    let is_running = worker_lock.as_ref().map(|w| w.is_running).unwrap_or(false);

                    // Start Batch Button
                    ui.horizontal(|ui| {
                        let btn = egui::Button::new("🎬 Start Direct Batch Patch");
                        if ui.add_enabled(!is_running && !queued_demos_shared.is_empty(), btn).clicked() {
                            *progress_msg = "Initializing patcher...".to_string();
                            *progress_pct = 0.0;
                            *success = false;
                            *error = None;

                            let queued_demos_shared = queued_demos_shared.clone();
                            let mut patcher_config = patcher_config.clone();
                            let ctx_clone = ctx.clone();

                            std::thread::spawn(move || {
                                let mut raw_streaks = Vec::new();
                                for demo in queued_demos_shared.iter() {
                                    let demo_path_str = demo.path.to_string_lossy().to_string();
                                    patcher_config.tickrate = demo.tickrate;
                                    patcher_config.pre_roll_ticks = (patcher_config.pre_roll_seconds * demo.tickrate) as i32;
                                    patcher_config.post_roll_ticks = (patcher_config.post_roll_seconds * demo.tickrate) as i32;

                                    for streak in &demo.streaks {
                                        if streak.is_selected {
                                            raw_streaks.push(CaptureStreak {
                                                start_tick: streak.start_tick,
                                                end_tick: streak.end_tick,
                                                source_demo: demo_path_str.clone(),
                                                target_player: Some(streak.target_player.clone()),
                                                kill_count: streak.kill_count,
                                                timeline_string: streak.timeline_string.clone(),
                                                duration_string: streak.duration_string.clone(),
                                                player_index: streak.player_index,
                                                kills: streak.kills.clone(),
                                                start_index: streak.start_index,
                                                end_index: streak.end_index,
                                            });
                                        }
                                    }
                                }

                                let jobs = build_batch_queue(raw_streaks, &patcher_config);
                                if !jobs.is_empty() {
                                    let cancel_token = Arc::new(AtomicBool::new(false));
                                    let worker = spawn_patch_batch(jobs, patcher_config.clone(), cancel_token);
                                    *acquire_lock!(get_worker()) = Some(worker);
                                    *acquire_lock!(get_progress_msg()) = "Spawning worker...".to_string();
                                    *acquire_lock!(get_progress_pct()) = 0.0;
                                    *acquire_lock!(get_success()) = false;
                                    *acquire_lock!(get_error()) = None;
                                } else {
                                    *acquire_lock!(get_progress_msg()) = "No selected streaks to patch.".to_string();
                                }
                                ctx_clone.request_repaint();
                            });
                        }

                        if is_running {
                            ui.spinner();
                        }
                    });

                    ui.add_space(8.0);

                    // ProgressBar using tracked float state
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
        _ => {}
    }
}

pub enum IngestionInput {
    Files(Vec<PathBuf>),
    Folder(PathBuf),
}

fn spawn_ingestion_thread(
    input: IngestionInput,
    rules: HighlightRules,
    ctx: egui::Context,
    tx: std::sync::mpsc::Sender<crate::types::GuiMessage>,
) {
    {
        let mut state = acquire_lock!(get_capture_state());
        *state = CaptureState::Scanning("Scanning files".to_string());
    }

    log_markdown("STARTING SCAN");

    std::thread::Builder::new()
        .name("ingestion_worker".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut last_repaint = std::time::Instant::now();
            let mut batch_count = 0;
            let files = match input {
                IngestionInput::Files(f) => f,
                IngestionInput::Folder(folder) => {
                    let mut list = Vec::new();
                    if let Ok(entries) = std::fs::read_dir(folder) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_file() && path.extension().map(|ext| ext == "dem").unwrap_or(false) {
                                list.push(path);
                            }
                        }
                    }
                    list
                }
            };

            let total_files = files.len();
            for (index, file) in files.into_iter().enumerate() {
                {
                    let mut state = acquire_lock!(get_capture_state());
                    *state = CaptureState::Scanning(format!("demo {} of {}", index + 1, total_files));
                }
                ctx.request_repaint();

                log_markdown(&format!("Attempting Demo {}: {:?}", index + 1, file.file_name().unwrap_or_default()));

                println!("-> Parsing: {:?}", file.file_name().unwrap_or_default());
                match scan_demo_for_highlights(&file, &rules) {
                    Ok((tickrate, streaks, is_pov, local_player_index)) => {
                        println!("<- Success: {:?}", file.file_name().unwrap_or_default());
                        let selectable: Vec<HighlightStreak> = streaks
                            .into_iter()
                            .map(|s| {
                                let target_player = s.target_player.unwrap_or_default();
                                let display_text = format!(
                                    "Player: {} | Kills: {} | Ticks: {} to {}",
                                    target_player, s.kill_count, s.start_tick, s.end_tick
                                );
                                HighlightStreak {
                                    start_tick: s.start_tick,
                                    end_tick: s.end_tick,
                                    kill_count: s.kill_count,
                                    target_player,
                                    is_selected: true,
                                    display_text,
                                    timeline_string: s.timeline_string,
                                    duration_string: s.duration_string,
                                    player_index: s.player_index,
                                    kills: s.kills,
                                    start_index: s.start_index,
                                    end_index: s.end_index,
                                }
                            })
                            .collect();

                        if !selectable.is_empty() {
                            let demo_name = file.file_name().unwrap_or_default().to_string_lossy().into_owned();
                            let item = DemoData {
                                demo_name,
                                path: file.to_path_buf(),
                                streaks: selectable,
                                tickrate,
                                is_pov,
                                local_player_index,
                            };
                            
                            // [STEP 2] Prevent Deadlock - Lock is dropped immediately after modifying the collection.
                            let _added = {
                                let queued_arc = get_queued_demos();
                                let mut queued_guard = acquire_lock!(queued_arc);
                                // TODO: Cleanup
                                log::info!("Ingestion thread acquired lock to push: {:?}", item.path);
                                if !queued_guard.iter().any(|d| d.path == item.path) {
                                    let queued = Arc::make_mut(&mut *queued_guard);
                                    queued.push(item);
                                    true
                                } else {
                                    false
                                }
                            };
                        }
                    }
                    Err(err) => {
                        if err == "Unsupported HLTV proxy demo format" {
                            eprintln!("ℹ Skipping HLTV proxy: {:?}", file.file_name().unwrap_or_default());
                        } else {
                            eprintln!("⚠ Skipped corrupted demo {:?}: {}", file.file_name().unwrap_or_default(), err);
                        }
                    }
                }

                // Batch repaints gate
                batch_count += 1;
                if last_repaint.elapsed() > std::time::Duration::from_millis(16) || batch_count >= 5 {
                    ctx.request_repaint();
                    last_repaint = std::time::Instant::now();
                    batch_count = 0;
                }
            }

            {
                let mut state = acquire_lock!(get_capture_state());
                *state = CaptureState::Ready;
            }
            // [STEP 1b] Send message to toggle loading state boolean
            let _ = tx.send(crate::types::GuiMessage::IngestionFinished);
            ctx.request_repaint();
        })
        .unwrap();
}