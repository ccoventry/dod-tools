// ============================================================
// views/capture/select.rs
// Renders CaptureStudioState::Select — Step 2 of the Capture Studio wizard.
//
// Responsibilities:
//   - POV filtering checkbox
//   - 2-wide column grid of per-demo streak tables (TableBuilder)
//   - Per-streak: selection checkbox, player name, kill-range DragValues
//     with orange colouring + reset button, duration, timeline string
//   - Deferred action dispatch (SelectAll / DeselectAll / RemoveDemo) applied
//     post-loop to satisfy the anti-crash / anti-simultaneous-mutation protocol
//   - Global bulk-action bar (Clear All / Select All / Deselect All)
//   - Export Configuration panel: all 6 DragValue sliders wired to PatcherConfig
//   - Async "Proceed to Capture ->" button: builds payload, calls build_batch_queue,
//     spawns patch_worker thread, sends GuiMessage::PatchingComplete on completion
// ============================================================

use std::sync::{Arc, Mutex};
use native::patch::PatcherConfig;
use crate::types::{DemoData, CaptureStudioState};
use super::{acquire_lock, widgets, panels};
use super::log_markdown;
use egui_extras::{TableBuilder, Column};


#[derive(PartialEq, Clone, Copy)]
enum SelectTab {
    Highlights,
    Configuration,
    Advanced,
}

pub fn render(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state_ptr: &mut CaptureStudioState,
    tx: std::sync::mpsc::Sender<crate::types::GuiMessage>,
    loading_ptr: &mut bool,
    _rules_mutex: &'static Mutex<native::patch::HighlightRules>,
    queued_demos_arc: Arc<Mutex<Arc<Vec<DemoData>>>>,
    patcher_config_mutex: &'static Mutex<PatcherConfig>,
    render_config_mutex: &'static Mutex<native::hlcr::config::RenderConfig>,
    settings: &mut crate::settings::AppSettings,
    draft_settings: &mut crate::settings::AppSettings,
    error_message: &mut Option<String>,
    subdir_cache: &mut std::collections::HashMap<std::path::PathBuf, Vec<std::path::PathBuf>>,
    tree_demo_cache: &mut std::collections::HashMap<std::path::PathBuf, usize>,
) {
    // Loading guard: ingestion thread may still be finishing its final write.
    if *loading_ptr {
        log::info!("UI: State is in Loading");
        ui.label("Loading...");
        return;
    } else {
        log::info!("UI: State transition to DisplayList");
    }

    // O(1) Arc pointer clone — does not deep-copy the underlying Vec.
    let queued_demos_shared = {
        let guard = acquire_lock!(queued_demos_arc);
        guard.clone()
    };
    let data = &*queued_demos_shared;

    // Persist active tab selection in temp memory
    let mut active_tab = ctx.data_mut(|d| {
        *d.get_temp_mut_or_insert_with(egui::Id::new("dodtools_select_tab"), || SelectTab::Highlights)
    });

    // Compute disk warning visibility state beforehand to feed the global footer warnings
    let mut show_disk_warning = false;
    {
        let patcher_config = acquire_lock!(patcher_config_mutex);
        if !data.is_empty() {
            let total_sequence_duration = calculate_merged_duration(data, &patcher_config);
            let w = patcher_config.resolution_width;
            let h = patcher_config.resolution_height;
            let fps = patcher_config.capture_fps;
            let mut bytes = native::sys::disk::calculate_raw_sequence_bytes(w, h, fps, total_sequence_duration);
            if patcher_config.separate_hud {
                bytes *= 3;
            }
            let required_gb = bytes as f64 / 1_073_741_824.0;
            let mut pool_free_bytes: u64 = 0;
            for path in &patcher_config.capture_directories {
                let free_bytes = native::sys::disk::get_available_bytes(path);
                if free_bytes != u64::MAX {
                    pool_free_bytes += free_bytes;
                }
            }
            let pool_total_gb = pool_free_bytes as f64 / 1_073_741_824.0;
            if required_gb > pool_total_gb {
                show_disk_warning = true;
            }
        }
    }

    // ── [STEP 3] Pinned Action Footer Panel ──────────────────────────────────────────
    egui::TopBottomPanel::bottom("select_action_footer").show_inside(ui, |ui| {
        ui.vertical(|ui| {
            // Error banner & disk space warning
            widgets::render_error_banner(ui, ctx, error_message);
            if show_disk_warning {
                ui.colored_label(egui::Color32::RED, "⚠ WARNING: Not enough total free disk space across the capture pool!");
            }

            ui.add_space(4.0);

            // Engine paths (HLAE / hl.exe)
            {
                let mut patcher_config = acquire_lock!(patcher_config_mutex);
                ui.horizontal(|ui| {
                    ui.strong("Recording Engine Paths:");
                });
                panels::render_engine_config_panel(ui, &mut patcher_config, error_message);
            }

            ui.add_space(4.0);



            // Primary process dispatch buttons (Proceed to Capture, Build Payload)
            {
                let mut patcher_config = acquire_lock!(patcher_config_mutex);
                widgets::render_primary_actions(
                    ui,
                    &mut patcher_config,
                    &mut *state_ptr,
                    &mut *loading_ptr,
                    &tx,
                    &queued_demos_arc,
                    ctx,
                );
            }
        });
    });

    // ── [STEP 4] Tab Navigation Header & Content Central Panel ─────────────────────
    egui::CentralPanel::default().show_inside(ui, |ui| {
        let mut tab_changed = false;
        ui.horizontal(|ui| {
            let r1 = ui.selectable_value(&mut active_tab, SelectTab::Highlights, "Highlights");
            let r2 = ui.selectable_value(&mut active_tab, SelectTab::Configuration, "Configuration");
            let r3 = ui.selectable_value(&mut active_tab, SelectTab::Advanced, "Advanced");
            if r1.changed() || r2.changed() || r3.changed() {
                tab_changed = true;
            }
        });
        if tab_changed {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("dodtools_select_tab"), active_tab));
        }

        ui.separator();
        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                match active_tab {
                    SelectTab::Highlights => {
                        ui.heading("📂 Step 2: Select Highlights");
                        ui.add_space(4.0);
                        ui.label("Enable streaks for capture and adjust start/end kill markers.");
                        ui.add_space(8.0);

                        if !data.is_empty() {
                            if widgets::render_bulk_actions(ui, &queued_demos_arc) {
                                ctx.data_mut(|d| d.insert_temp(egui::Id::new("dodtools_disk_estimate_dirty"), true));
                            }
                            ui.add_space(4.0);

                            ui.strong("Discovered Highlight Streaks");
                            ui.add_space(4.0);

                            enum DemoAction {
                                RemoveDemo(usize),
                                SelectAll(usize),
                                DeselectAll(usize),
                            }
                            let mut actions_to_apply: Vec<DemoAction> = Vec::new();

                            egui::ScrollArea::vertical()
                                .min_scrolled_height(ui.available_height() - 100.0)
                                .id_salt("discovered_streaks_scroll_tables")
                                .show(ui, |ui| {
                                    ui.columns(2, |columns| {
                                        for (d_idx, demo) in data.iter().enumerate() {
                                            if demo.streaks.is_empty() {
                                                continue;
                                            }

                                            let col_ui = &mut columns[d_idx % 2];
                                            col_ui.push_id(&demo.demo_name, |ui| {
                                                egui::Frame::group(ui.style()).show(ui, |ui| {
                                                    ui.horizontal(|ui| {
                                                        let player_name = demo.streaks.iter()
                                                            .find(|s| Some(s.player_index) == demo.local_player_index)
                                                            .map(|s| s.target_player.as_str())
                                                            .unwrap_or("Unknown");
                                                        let header_text = format!("{} - Player: {}", demo.demo_name, player_name);
                                                        ui.strong(header_text);
                                                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                                            if ui.button("Select All").clicked() {
                                                                actions_to_apply.push(DemoAction::SelectAll(d_idx));
                                                            }
                                                            if ui.button("Deselect All").clicked() {
                                                                actions_to_apply.push(DemoAction::DeselectAll(d_idx));
                                                            }
                                                            ui.add_space(4.0);
                                                            if ui.button("🗑 Remove Demo")
                                                                .on_hover_text("Remove this demo from the queue")
                                                                .clicked()
                                                            {
                                                                actions_to_apply.push(DemoAction::RemoveDemo(d_idx));
                                                            }
                                                        });
                                                    });
                                                    ui.add_space(2.0);

                                                    TableBuilder::new(ui)
                                                        .id_salt(format!("{}_table", demo.demo_name))
                                                        .striped(true)
                                                        .vscroll(false)
                                                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                                                        .column(Column::initial(30.0))
                                                        .column(Column::auto())
                                                        .column(Column::exact(140.0))
                                                        .column(Column::exact(40.0))
                                                        .column(Column::initial(70.0))
                                                        .column(Column::exact(50.0))
                                                        .column(Column::exact(85.0))
                                                        .column(Column::remainder())
                                                        .header(20.0, |mut header| {
                                                            header.col(|ui| { ui.strong("Row #"); });
                                                            header.col(|ui| { ui.strong("Sel"); });
                                                            header.col(|ui| { ui.strong("Kill Range"); });
                                                            header.col(|ui| { ui.strong("Kills"); });
                                                            header.col(|ui| { ui.strong("Timestamp"); });
                                                            header.col(|ui| { ui.strong("Dur."); });
                                                            header.col(|ui| { ui.strong("Status"); });
                                                            header.col(|ui| { ui.strong("Details"); });
                                                        })
                                                        .body(|body| {
                                                            let filtered_indices: Vec<usize> = (0..demo.streaks.len())
                                                                .filter(|&idx| {
                                                                    let streak = &demo.streaks[idx];
                                                                    if demo.is_pov && Some(streak.player_index) != demo.local_player_index {
                                                                        false
                                                                    } else {
                                                                        true
                                                                    }
                                                                })
                                                                .collect();

                                                            body.rows(20.0, filtered_indices.len(), |mut row| {
                                                                let row_idx = row.index();
                                                                let streak_idx = filtered_indices[row_idx];
                                                                let streak = &demo.streaks[streak_idx];

                                                                row.col(|ui| {
                                                                    ui.label(format!("{}", row_idx + 1));
                                                                });

                                                                row.col(|ui| {
                                                                    let mut is_selected = streak.is_selected;
                                                                    if ui.checkbox(&mut is_selected, "").changed() {
                                                                        log_markdown(&format!(
                                                                            "UI Interaction: Toggled streak selection for {}, new value: {}",
                                                                            streak.target_player, is_selected
                                                                        ));
                                                                        let mut guard = acquire_lock!(queued_demos_arc);
                                                                        let queued = Arc::make_mut(&mut *guard);
                                                                        queued[d_idx].streaks[streak_idx].is_selected = is_selected;
                                                                        ctx.data_mut(|d| d.insert_temp(egui::Id::new("dodtools_disk_estimate_dirty"), true));
                                                                    }
                                                                });

                                                                row.col(|ui| {
                                                                    let max_idx = streak.kills.len().saturating_sub(1);
                                                                    let is_modified = streak.start_index > 0
                                                                        || streak.end_index < max_idx;

                                                                    let mut start_idx = streak.start_index;
                                                                    let mut end_idx   = streak.end_index;

                                                                    ui.horizontal(|ui| {
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
                                                                                let mut guard = acquire_lock!(queued_demos_arc);
                                                                                let queued = Arc::make_mut(&mut *guard);
                                                                                let sm = &mut queued[d_idx].streaks[streak_idx];
                                                                                sm.start_index = start_idx;
                                                                                sm.end_index   = end_idx;
                                                                                sm.update_visuals();
                                                                                ctx.data_mut(|d| d.insert_temp(egui::Id::new("dodtools_disk_estimate_dirty"), true));
                                                                            }
                                                                        });

                                                                        if is_modified && ui.button("↺")
                                                                            .on_hover_text("Reset to full range")
                                                                            .clicked()
                                                                        {
                                                                            let mut guard = acquire_lock!(queued_demos_arc);
                                                                            let queued = Arc::make_mut(&mut *guard);
                                                                            let sm = &mut queued[d_idx].streaks[streak_idx];
                                                                            sm.start_index = 0;
                                                                            sm.end_index   = max_idx;
                                                                            sm.update_visuals();
                                                                        }
                                                                    });
                                                                });

                                                                row.col(|ui| { ui.label(streak.kill_count.to_string()); });

                                                                row.col(|ui| {
                                                                    let absolute_timestamp = streak.viewdemo_times.get(streak.start_index).copied().unwrap_or(0.0);
                                                                    let ts_secs = absolute_timestamp.round() as i32;
                                                                    ui.label(format!("{}:{:02}", ts_secs / 60, ts_secs % 60));
                                                                });

                                                                row.col(|ui| { ui.label(&streak.duration_string); });

                                                                row.col(|ui| {
                                                                    let mut status = streak.status;
                                                                    let status_changed = egui::ComboBox::from_id_salt(format!("{}_{}_status", demo.demo_name, streak_idx))
                                                                        .selected_text(format!("{:?}", status))
                                                                        .show_ui(ui, |ui| {
                                                                            let mut changed = false;
                                                                            for &val in &[crate::session::HighlightStatus::None, crate::session::HighlightStatus::Pending, crate::session::HighlightStatus::Captured, crate::session::HighlightStatus::Rendered] {
                                                                                changed |= ui.selectable_value(&mut status, val, format!("{:?}", val)).changed();
                                                                            }
                                                                            changed
                                                                        }).inner.unwrap_or(false);

                                                                    if status_changed {
                                                                        let mut guard = acquire_lock!(queued_demos_arc);
                                                                        let queued = Arc::make_mut(&mut *guard);
                                                                        queued[d_idx].streaks[streak_idx].status = status;
                                                                        ctx.data_mut(|d| d.insert_temp(egui::Id::new("dodtools_disk_estimate_dirty"), true));
                                                                    }
                                                                });

                                                                row.col(|ui| { ui.label(&streak.timeline_string); });
                                                            });
                                                        });
                                                });
                                            });
                                            col_ui.add_space(4.0);
                                        }
                                    });
                                });

                            let mut removals: Vec<usize> = actions_to_apply
                                .iter()
                                .filter_map(|a| if let DemoAction::RemoveDemo(i) = a { Some(*i) } else { None })
                                .collect();
                            removals.sort_unstable_by(|a, b| b.cmp(a));

                            let mut estimate_changed = false;
                            for action in &actions_to_apply {
                                match action {
                                    DemoAction::SelectAll(idx) => {
                                        let mut guard = acquire_lock!(queued_demos_arc);
                                        let queued = Arc::make_mut(&mut *guard);
                                        if let Some(demo) = queued.get_mut(*idx) {
                                            for s in &mut demo.streaks {
                                                if demo.is_pov && Some(s.player_index) != demo.local_player_index {
                                                    continue;
                                                }
                                                s.is_selected = true;
                                            }
                                        }
                                        estimate_changed = true;
                                    }
                                    DemoAction::DeselectAll(idx) => {
                                        let mut guard = acquire_lock!(queued_demos_arc);
                                        let queued = Arc::make_mut(&mut *guard);
                                        if let Some(demo) = queued.get_mut(*idx) {
                                            for s in &mut demo.streaks { s.is_selected = false; }
                                        }
                                        estimate_changed = true;
                                    }
                                    DemoAction::RemoveDemo(_) => {}
                                }
                            }
                            for idx in removals {
                                let mut guard = acquire_lock!(queued_demos_arc);
                                let queued = Arc::make_mut(&mut *guard);
                                if idx < queued.len() {
                                    queued.remove(idx);
                                }
                                estimate_changed = true;
                            }
                            if estimate_changed {
                                ctx.data_mut(|d| d.insert_temp(egui::Id::new("dodtools_disk_estimate_dirty"), true));
                            }
                        } else {
                            ui.weak("No discovered highlight streaks. Go back to Scan and add demo files.");
                        }
                    }
                    SelectTab::Configuration => {
                        ui.heading("⚙ Capture Configuration");
                        ui.add_space(4.0);
                        ui.label("Configure target codecs, folders, and resolution options.");
                        ui.add_space(8.0);

                        let mut patcher_config = acquire_lock!(patcher_config_mutex);

                        // Disk Space Estimate
                        {
                            let cache_id = egui::Id::new("dodtools_disk_estimate_cache");
                            let dirty_id = egui::Id::new("dodtools_disk_estimate_dirty");
                            let is_dirty: bool = ctx.data(|d| d.get_temp(dirty_id)).unwrap_or(true);
                            let cached_estimate: Option<u64> = ctx.data(|d| d.get_temp(cache_id));

                            let required_bytes = match cached_estimate {
                                Some(bytes) if !is_dirty => bytes,
                                _ => {
                                    let latest_demos = acquire_lock!(queued_demos_arc);
                                    let total_sequence_duration = calculate_merged_duration(&latest_demos, &patcher_config);
                                    let w = patcher_config.resolution_width;
                                    let h = patcher_config.resolution_height;
                                    let fps = patcher_config.capture_fps;
                                    let mut bytes = native::sys::disk::calculate_raw_sequence_bytes(w, h, fps, total_sequence_duration);
                                    if patcher_config.separate_hud {
                                        bytes *= 3;
                                    }
                                    ctx.data_mut(|d| {
                                        d.insert_temp(cache_id, bytes);
                                        d.insert_temp(dirty_id, false);
                                    });
                                    bytes
                                }
                            };
                            let required_gb = required_bytes as f64 / 1_073_741_824.0;

                            let mut pool_free_bytes: u64 = 0;
                            let mut drives_info = Vec::new();

                            for path in &patcher_config.capture_directories {
                                let free_bytes = native::sys::disk::get_available_bytes(path);
                                if free_bytes != u64::MAX {
                                    pool_free_bytes += free_bytes;
                                }
                                let free_gb = if free_bytes == u64::MAX { 0.0 } else { free_bytes as f64 / 1_073_741_824.0 };
                                let path_str = path.to_string_lossy().to_string();
                                let shortened_path = if path_str.len() > 30 {
                                    let head = if path_str.len() >= 3 { &path_str[..3] } else { "" };
                                    format!("{}...{}", head, &path_str[path_str.len() - 24..])
                                } else {
                                    path_str
                                };
                                drives_info.push((shortened_path, free_gb));
                            }

                            let pool_total_gb = pool_free_bytes as f64 / 1_073_741_824.0;

                            ui.horizontal(|ui| {
                                ui.strong("Disk Space Estimate:");
                                ui.label(format!("Disk Space Pool: Required: {:.1} GB / Total Free: {:.1} GB", required_gb, pool_total_gb));
                            });

                            for (shortened_path, free_gb) in &drives_info {
                                ui.label(format!("  ↳ {} : {:.1} GB Free", shortened_path, free_gb));
                            }
                        }

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        // Capture Output Drives failover prioritization vector
                        ui.strong("Mapped Capture Output Drives (Failover Priority Vector):");
                        ui.add_space(4.0);

                        let mut to_remove = None;
                        let mut swap_indices = None;
                        let dirs_len = patcher_config.capture_directories.len();
                        for (idx, dir) in patcher_config.capture_directories.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(format!("{}:", idx + 1));
                                ui.label(dir.to_string_lossy());
                                if idx > 0 {
                                    if ui.button("⬆").clicked() {
                                        swap_indices = Some((idx, idx - 1));
                                    }
                                }
                                if idx < dirs_len.saturating_sub(1) {
                                    if ui.button("⬇").clicked() {
                                        swap_indices = Some((idx, idx + 1));
                                    }
                                }
                                if ui.button("🗑 Remove").clicked() {
                                    to_remove = Some(idx);
                                }
                            });
                        }

                        let mut dir_changed = false;
                        if let Some((i, j)) = swap_indices {
                            patcher_config.capture_directories.swap(i, j);
                            crate::settings::save_patcher_config(&patcher_config);
                            dir_changed = true;
                        } else if let Some(idx) = to_remove {
                            patcher_config.capture_directories.remove(idx);
                            crate::settings::save_patcher_config(&patcher_config);
                            dir_changed = true;
                        }

                        static DRIVE_PICKER: std::sync::OnceLock<Mutex<egui_file_dialog::FileDialog>> = std::sync::OnceLock::new();
                        let mut drive_picker = acquire_lock!(DRIVE_PICKER.get_or_init(|| Mutex::new(egui_file_dialog::FileDialog::new())));

                        if ui.button("➕ Add Drive").clicked() {
                            drive_picker.pick_directory();
                        }

                        drive_picker.update(ctx);
                        if let Some(path) = drive_picker.take_picked() {
                            patcher_config.capture_directories.push(path);
                            crate::settings::save_patcher_config(&patcher_config);
                            dir_changed = true;
                        }

                        let config_changed = panels::render_capture_config_panel(ui, ctx, &mut patcher_config);
                        if dir_changed || config_changed {
                            ctx.data_mut(|d| d.insert_temp(egui::Id::new("dodtools_disk_estimate_dirty"), true));
                        }

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        // Export profiles & codecs
                        ui.strong("Export Configuration");
                        ui.add_space(4.0);
                        let mut render_config = acquire_lock!(render_config_mutex);
                        panels::render_export_config_panel(ui, ctx, &mut render_config);
                    }
                    SelectTab::Advanced => {
                        ui.heading("⚡ Advanced Adjustments & Dev Settings");
                        ui.add_space(4.0);
                        ui.label("Configure precision buffers, scripting commands, and debugging variables.");
                        ui.add_space(8.0);

                        let mut patcher_config = acquire_lock!(patcher_config_mutex);

                        // Render highlight buffers, scripting events and timelines
                        let highlight_changed = panels::render_highlight_settings_panel(
                            ui,
                            ctx,
                            &mut patcher_config,
                            settings,
                            draft_settings,
                            subdir_cache,
                            tree_demo_cache,
                        );
                        if highlight_changed {
                            ctx.data_mut(|d| d.insert_temp(egui::Id::new("dodtools_disk_estimate_dirty"), true));
                        }

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        // Debugging Settings
                        ui.strong("🐛 Debugging Settings");
                        ui.add_space(4.0);
                        panels::render_debug_panel(ui, &mut patcher_config);
                    }
                }
            });
    });
}

fn calculate_merged_duration(data: &[DemoData], config: &PatcherConfig) -> f32 {
    let mut total_duration = 0.0;
    
    // Group all selected streaks by demo path and player
    let mut grouped: std::collections::HashMap<(String, Option<String>), Vec<crate::types::HighlightStreak>> = std::collections::HashMap::new();
    for demo in data {
        for streak in &demo.streaks {
            if streak.is_selected {
                if !demo.is_pov || Some(streak.player_index) == demo.local_player_index {
                    grouped.entry((demo.path.to_string_lossy().to_string(), Some(streak.target_player.clone())))
                           .or_default()
                           .push(streak.clone());
                }
            }
        }
    }
    
    for (_, mut streaks) in grouped {
        if streaks.is_empty() {
            continue;
        }
        // Sort by start_tick
        streaks.sort_by_key(|s| s.start_tick);
        
        let engine_tickrate = 100.0;
        
        // Merge overlapping streaks
        let mut merged: Vec<(i32, i32)> = Vec::new();
        for s in streaks {
            let start = s.start_tick;
            let end = s.end_tick;
            if merged.is_empty() {
                merged.push((start, end));
            } else {
                let dynamic_pre_roll_ticks = (config.pre_roll_seconds * engine_tickrate) as i32;
                let dynamic_post_roll_ticks = (config.post_roll_seconds * engine_tickrate) as i32;
                let adjusted_start = (start - dynamic_pre_roll_ticks).max(0);
                let last = merged.last_mut().unwrap();
                if adjusted_start <= last.1 + dynamic_post_roll_ticks {
                    last.1 = last.1.max(end);
                } else {
                    merged.push((start, end));
                }
            }
        }
        
        for (start, end) in merged {
            let anchor_duration = ((end - start) as f32) / engine_tickrate;
            total_duration += config.calculate_total_capture_duration(anchor_duration);
        }
    }
    
    total_duration
}

