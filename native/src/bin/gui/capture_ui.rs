use std::sync::{Arc, Mutex, OnceLock, atomic::{AtomicBool, Ordering}};
use std::path::PathBuf;
use native::patch::{
    CaptureWorker, PatchEvent, PatcherConfig, CaptureStreak, build_batch_queue,
    spawn_patch_batch, HighlightRules, scan_demo_for_highlights,
};
use crate::types::{
    QueuedStreakExport, QueuedDemo, SelectableStreak, QueueGroupingMode,
    GroupedPlayer, GroupedPlayerStreak, FlatStreak, CaptureStudioState,
};

static WORKER_STATE: OnceLock<Mutex<Option<CaptureWorker>>> = OnceLock::new();
static PROGRESS_MSG: OnceLock<Mutex<String>> = OnceLock::new();
static PROGRESS_PCT: OnceLock<Mutex<f32>> = OnceLock::new();
static SUCCESS_STATE: OnceLock<Mutex<bool>> = OnceLock::new();
static ERROR_STATE: OnceLock<Mutex<Option<String>>> = OnceLock::new();

// Wizard fields string states to allow parsing None on empty inputs
static MIN_KILLS_STR: OnceLock<Mutex<String>> = OnceLock::new();
static MAX_TIME_GAP_STR: OnceLock<Mutex<String>> = OnceLock::new();

// Ingestion Wizard & Discovery State
static HIGHLIGHT_RULES: OnceLock<Mutex<HighlightRules>> = OnceLock::new();
static QUEUED_DEMOS: OnceLock<Mutex<Vec<QueuedDemo>>> = OnceLock::new();
static IS_INGESTING: OnceLock<Mutex<bool>> = OnceLock::new();
static PATCHER_CONFIG: OnceLock<Mutex<PatcherConfig>> = OnceLock::new();

// Caching variables to satisfy: "Do NOT perform grouping or sorting calculations inside the egui render loop."
static GROUPING_MODE: OnceLock<Mutex<QueueGroupingMode>> = OnceLock::new();
static CACHED_PLAYER_GROUPS: OnceLock<Mutex<Vec<GroupedPlayer>>> = OnceLock::new();
static CACHED_FLAT_LIST: OnceLock<Mutex<Vec<FlatStreak>>> = OnceLock::new();

// Background Ingestion Channel
static INGESTION_CHANNEL: OnceLock<(
    std::sync::mpsc::Sender<QueuedDemo>,
    Mutex<std::sync::mpsc::Receiver<QueuedDemo>>
)> = OnceLock::new();

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

fn get_queued_demos() -> &'static Mutex<Vec<QueuedDemo>> {
    QUEUED_DEMOS.get_or_init(|| Mutex::new(Vec::new()))
}

fn get_is_ingesting() -> &'static Mutex<bool> {
    IS_INGESTING.get_or_init(|| Mutex::new(false))
}

fn get_patcher_config() -> &'static Mutex<PatcherConfig> {
    PATCHER_CONFIG.get_or_init(|| Mutex::new(PatcherConfig::default()))
}

fn get_grouping_mode() -> &'static Mutex<QueueGroupingMode> {
    GROUPING_MODE.get_or_init(|| Mutex::new(QueueGroupingMode::ByDemo))
}

fn get_cached_player_groups() -> &'static Mutex<Vec<GroupedPlayer>> {
    CACHED_PLAYER_GROUPS.get_or_init(|| Mutex::new(Vec::new()))
}

fn get_cached_flat_list() -> &'static Mutex<Vec<FlatStreak>> {
    CACHED_FLAT_LIST.get_or_init(|| Mutex::new(Vec::new()))
}

fn get_ingestion_channel() -> &'static (std::sync::mpsc::Sender<QueuedDemo>, Mutex<std::sync::mpsc::Receiver<QueuedDemo>>) {
    INGESTION_CHANNEL.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel();
        (tx, Mutex::new(rx))
    })
}

// Function to update the caches when queued_demos is modified or grouping mode changes.
fn update_grouping_cache(queued: &[QueuedDemo]) {
    // 1. Group by Player Cache
    let mut map: std::collections::HashMap<String, Vec<GroupedPlayerStreak>> = std::collections::HashMap::new();
    for (d_idx, demo) in queued.iter().enumerate() {
        for (s_idx, streak) in demo.streaks.iter().enumerate() {
            map.entry(streak.target_player.clone()).or_default().push(GroupedPlayerStreak {
                demo_path: demo.path.clone(),
                start_tick: streak.start_tick,
                end_tick: streak.end_tick,
                kill_count: streak.kill_count,
                is_selected: streak.is_selected,
                demo_index: d_idx,
                streak_index: s_idx,
            });
        }
    }
    let mut groups: Vec<GroupedPlayer> = map.into_iter()
        .map(|(name, streaks)| GroupedPlayer { name, streaks })
        .collect();
    groups.sort_by(|a, b| a.name.cmp(&b.name));
    
    let mut cache = get_cached_player_groups().lock().unwrap();
    *cache = groups;

    // 2. Flat List Cache (Chronologically Sorted)
    let mut flat = Vec::new();
    for (d_idx, demo) in queued.iter().enumerate() {
        let file_name = demo.path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        for (s_idx, streak) in demo.streaks.iter().enumerate() {
            flat.push(FlatStreak {
                demo_path: demo.path.clone(),
                file_name: file_name.clone(),
                start_tick: streak.start_tick,
                end_tick: streak.end_tick,
                kill_count: streak.kill_count,
                target_player: streak.target_player.clone(),
                is_selected: streak.is_selected,
                demo_index: d_idx,
                streak_index: s_idx,
            });
        }
    }
    
    // Sort flat list first alphabetically by filename, then ascending by start_tick
    flat.sort_by(|a, b| {
        a.file_name.cmp(&b.file_name)
            .then_with(|| a.start_tick.cmp(&b.start_tick))
    });

    let mut flat_cache = get_cached_flat_list().lock().unwrap();
    *flat_cache = flat;
}

pub fn render_patch_ui(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    _export_queue: &mut Vec<QueuedStreakExport>,
    current_state: CaptureStudioState,
    state_ptr: &mut CaptureStudioState,
) {
    let mut worker_lock = get_worker().lock().unwrap();
    let mut progress_msg = get_progress_msg().lock().unwrap();
    let mut progress_pct = get_progress_pct().lock().unwrap();
    let mut success = get_success().lock().unwrap();
    let mut error = get_error().lock().unwrap();

    let mut rules = get_highlight_rules().lock().unwrap();
    let mut queued_demos = get_queued_demos().lock().unwrap();
    let is_ingesting = get_is_ingesting().lock().unwrap();
    let mut patcher_config = get_patcher_config().lock().unwrap();
    let mut grouping_mode = get_grouping_mode().lock().unwrap();

    let mut min_kills_str = get_min_kills_str().lock().unwrap();
    let mut max_time_gap_str = get_max_time_gap_str().lock().unwrap();

    // Check background ingestion channel
    let mut cache_dirty = false;
    let (_, rx_lock) = get_ingestion_channel();
    {
        let rx = rx_lock.lock().unwrap();
        while let Ok(new_demo) = rx.try_recv() {
            if !queued_demos.iter().any(|d| d.path == new_demo.path) {
                queued_demos.push(new_demo);
                cache_dirty = true;
            }
        }
    }
    if cache_dirty {
        update_grouping_cache(&queued_demos);
    }

    // Poll worker receiver
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
                        let ingesting = *is_ingesting;
                        if ui.add_enabled(!ingesting, egui::Button::new("➕ Add Demo Files")).clicked() {
                            if let Some(files) = rfd::FileDialog::new()
                                .add_filter("Demo files", &["dem"])
                                .pick_files()
                            {
                                spawn_ingestion_thread(files, rules.clone(), ctx.clone());
                            }
                        }

                        if ui.add_enabled(!ingesting, egui::Button::new("📂 Add Folder")).clicked() {
                            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                if let Ok(entries) = std::fs::read_dir(folder) {
                                    let files: Vec<PathBuf> = entries
                                        .filter_map(|e| e.ok())
                                        .map(|e| e.path())
                                        .filter(|p| p.extension().map(|ext| ext == "dem").unwrap_or(false))
                                        .collect();
                                    if !files.is_empty() {
                                        spawn_ingestion_thread(files, rules.clone(), ctx.clone());
                                    }
                                }
                            }
                        }

                        if ingesting {
                            ui.spinner();
                            ui.weak("Scanning demos...");
                        }
                    });

                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        if ui.button("Proceed to Selection ->").clicked() {
                            *state_ptr = CaptureStudioState::Select;
                        }
                    });
                });
            });
        }
        CaptureStudioState::Select => {
            // STEP 2: SELECT UI
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.heading("📂 Step 2: Select Highlights & Patch");
                    ui.add_space(4.0);
                    ui.label("Choose grouping mode, enable streaks, adjust patch parameters, and launch patcher.");
                    ui.add_space(8.0);

                    // Grouping Mode Combo Box
                    ui.horizontal(|ui| {
                        ui.label("Grouping Mode:");
                        let old_mode = *grouping_mode;
                        egui::ComboBox::from_id_salt("grouping_mode_combo")
                            .selected_text(match *grouping_mode {
                                QueueGroupingMode::ByDemo => "By Demo",
                                QueueGroupingMode::ByPlayer => "By Player",
                                QueueGroupingMode::Flat => "Flat List",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut *grouping_mode, QueueGroupingMode::ByDemo, "By Demo");
                                ui.selectable_value(&mut *grouping_mode, QueueGroupingMode::ByPlayer, "By Player");
                                ui.selectable_value(&mut *grouping_mode, QueueGroupingMode::Flat, "Flat List");
                            });
                        if old_mode != *grouping_mode {
                            update_grouping_cache(&queued_demos);
                        }
                    });

                    ui.add_space(8.0);

                    // Collapsing Queue of Discovered Streaks
                    if !queued_demos.is_empty() {
                        ui.strong("Discovered Highlight Streaks");
                        ui.add_space(4.0);

                        let mut demo_to_remove = None;
                        let mut cache_dirty_local = false;

                        egui::ScrollArea::vertical()
                            .max_height(200.0)
                            .id_salt("discovered_streaks_scroll")
                            .show(ui, |ui| {
                                match *grouping_mode {
                                    QueueGroupingMode::ByDemo => {
                                        for (d_idx, demo) in queued_demos.iter_mut().enumerate() {
                                            let file_name = demo.path.file_name().unwrap_or_default().to_string_lossy();
                                            let total_streaks = demo.streaks.len();
                                            let selected_count = demo.streaks.iter().filter(|s| s.is_selected).count();
                                            let mut all_selected = selected_count == total_streaks;

                                            ui.horizontal(|ui| {
                                                if ui.checkbox(&mut all_selected, "").changed() {
                                                    for streak in &mut demo.streaks {
                                                        streak.is_selected = all_selected;
                                                    }
                                                    cache_dirty_local = true;
                                                }

                                                egui::collapsing_header::CollapsingState::load_with_default_open(
                                                    ui.ctx(),
                                                    ui.make_persistent_id(format!("demo_collapsible_{}", d_idx)),
                                                    true,
                                                )
                                                .show_header(ui, |ui| {
                                                    ui.label(format!("{} ({} / {} selected) - {:.1} fps", file_name, selected_count, total_streaks, demo.tickrate));
                                                })
                                                .body(|ui| {
                                                    for (_s_idx, streak) in demo.streaks.iter_mut().enumerate() {
                                                        let label = format!(
                                                            "Player: {} | Kills: {} | Ticks: {} to {}",
                                                            streak.target_player, streak.kill_count, streak.start_tick, streak.end_tick
                                                        );
                                                        if ui.checkbox(&mut streak.is_selected, label).changed() {
                                                            cache_dirty_local = true;
                                                        }
                                                    }
                                                });

                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    if ui.button("🗑").on_hover_text("Remove this demo").clicked() {
                                                        demo_to_remove = Some(d_idx);
                                                    }
                                                });
                                            });
                                            ui.separator();
                                        }
                                    }
                                    QueueGroupingMode::ByPlayer => {
                                        let mut player_groups = get_cached_player_groups().lock().unwrap();
                                        for (p_idx, group) in player_groups.iter_mut().enumerate() {
                                            let total_streaks = group.streaks.len();
                                            let selected_count = group.streaks.iter().filter(|s| s.is_selected).count();
                                            let mut all_selected = selected_count == total_streaks;

                                            ui.horizontal(|ui| {
                                                if ui.checkbox(&mut all_selected, "").changed() {
                                                    for streak in &mut group.streaks {
                                                        streak.is_selected = all_selected;
                                                        queued_demos[streak.demo_index].streaks[streak.streak_index].is_selected = all_selected;
                                                    }
                                                    cache_dirty_local = true;
                                                }

                                                egui::collapsing_header::CollapsingState::load_with_default_open(
                                                    ui.ctx(),
                                                    ui.make_persistent_id(format!("player_collapsible_{}", p_idx)),
                                                    true,
                                                )
                                                .show_header(ui, |ui| {
                                                    ui.label(format!("Player: {} ({} / {} selected)", group.name, selected_count, total_streaks));
                                                })
                                                .body(|ui| {
                                                    for streak in &mut group.streaks {
                                                        let file_name = streak.demo_path.file_name().unwrap_or_default().to_string_lossy();
                                                        let label = format!(
                                                            "{} | Kills: {} | Ticks: {} to {}",
                                                            file_name, streak.kill_count, streak.start_tick, streak.end_tick
                                                        );
                                                        if ui.checkbox(&mut streak.is_selected, label).changed() {
                                                            queued_demos[streak.demo_index].streaks[streak.streak_index].is_selected = streak.is_selected;
                                                            cache_dirty_local = true;
                                                        }
                                                    }
                                                });
                                            });
                                            ui.separator();
                                        }
                                    }
                                    QueueGroupingMode::Flat => {
                                        let mut flat_list = get_cached_flat_list().lock().unwrap();
                                        for streak in flat_list.iter_mut() {
                                            let label = format!(
                                                "[{}] Player: {} | Kills: {} | Ticks: {} to {}",
                                                streak.file_name, streak.target_player, streak.kill_count, streak.start_tick, streak.end_tick
                                            );
                                            if ui.checkbox(&mut streak.is_selected, label).changed() {
                                                queued_demos[streak.demo_index].streaks[streak.streak_index].is_selected = streak.is_selected;
                                                cache_dirty_local = true;
                                            }
                                        }
                                    }
                                }
                            });

                        if let Some(idx) = demo_to_remove {
                            queued_demos.remove(idx);
                            cache_dirty_local = true;
                        }

                        if cache_dirty_local {
                            update_grouping_cache(&queued_demos);
                        }

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button("Clear All Discovered").clicked() {
                                queued_demos.clear();
                                update_grouping_cache(&queued_demos);
                            }
                            if ui.button("Select All").clicked() {
                                for d in queued_demos.iter_mut() {
                                    for s in &mut d.streaks {
                                        s.is_selected = true;
                                    }
                                }
                                update_grouping_cache(&queued_demos);
                            }
                            if ui.button("Deselect All").clicked() {
                                for d in queued_demos.iter_mut() {
                                    for s in &mut d.streaks {
                                        s.is_selected = false;
                                    }
                                }
                                update_grouping_cache(&queued_demos);
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
                        if ui.add_enabled(!is_running && !queued_demos.is_empty(), btn).clicked() {
                            let mut raw_streaks = Vec::new();
                            for demo in queued_demos.iter() {
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
                                        });
                                    }
                                }
                            }

                            let jobs = build_batch_queue(raw_streaks, &patcher_config);
                            if !jobs.is_empty() {
                                let cancel_token = Arc::new(AtomicBool::new(false));
                                let worker = spawn_patch_batch(jobs, patcher_config.clone(), cancel_token);
                                *worker_lock = Some(worker);
                                *progress_msg = "Spawning worker...".to_string();
                                *progress_pct = 0.0;
                                *success = false;
                                *error = None;
                            } else {
                                *progress_msg = "No selected streaks to patch.".to_string();
                            }
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

fn spawn_ingestion_thread(files: Vec<PathBuf>, rules: HighlightRules, ctx: egui::Context) {
    let (tx, _) = get_ingestion_channel();
    let tx = tx.clone();
    
    let ingesting_lock = get_is_ingesting();
    {
        let mut ingesting = ingesting_lock.lock().unwrap();
        *ingesting = true;
    }

    std::thread::spawn(move || {
        for file in files {
            if let Ok((tickrate, streaks)) = scan_demo_for_highlights(&file, &rules) {
                let selectable: Vec<SelectableStreak> = streaks
                    .into_iter()
                    .map(|s| SelectableStreak {
                        start_tick: s.start_tick,
                        end_tick: s.end_tick,
                        kill_count: s.kill_count,
                        target_player: s.target_player.unwrap_or_default(),
                        is_selected: true,
                    })
                    .collect();

                if !selectable.is_empty() {
                    let _ = tx.send(QueuedDemo {
                        path: file,
                        streaks: selectable,
                        tickrate,
                    });
                }
            }
        }

        let mut ingesting = get_is_ingesting().lock().unwrap();
        *ingesting = false;
        ctx.request_repaint();
    });
}
