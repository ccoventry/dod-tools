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

    // ── Step 2 header group ──────────────────────────────────────────────────────
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
    ui.group(|ui| {
        ui.vertical(|ui| {
            ui.heading("📂 Step 2: Select Highlights & Patch");
            ui.add_space(4.0);
            ui.label("Enable streaks, adjust patch parameters, and launch patcher.");
            ui.add_space(8.0);

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
            ui.add_space(8.0);

            if !queued_demos_shared.is_empty() {
                widgets::render_bulk_actions(ui, &queued_demos_arc);
                ui.add_space(4.0);
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
                    .min_scrolled_height(ui.available_height() - 150.0)
                    .id_salt("discovered_streaks_scroll_tables")
                    .show(ui, |ui| {
                        ui.columns(2, |columns| {
                            for (d_idx, demo) in data.iter().enumerate() {
                                if demo.streaks.is_empty() {
                                    continue;
                                }

                                let col_ui = &mut columns[d_idx % 2];
                                egui::Frame::group(col_ui.style()).show(col_ui, |ui| {
                                    // ── Per-demo header row with bulk controls ───────
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
                                         .striped(true)
                                         .vscroll(false)
                                         .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                                         .column(Column::initial(30.0))   // [Row Number]
                                         .column(Column::auto())          // [Checkbox]
                                         .column(Column::exact(140.0))    // [Kill Range]
                                         .column(Column::exact(40.0))     // [Kills]
                                         .column(Column::initial(70.0).resizable(true)) // [Timestamp]
                                         .column(Column::exact(50.0))     // [Duration]
                                         .column(Column::remainder())     // [Details]
                                         .header(20.0, |mut header| {
                                             header.col(|ui| { ui.strong("Row #"); });
                                             header.col(|ui| { ui.strong("Sel"); });
                                             header.col(|ui| { ui.strong("Kill Range"); });
                                             header.col(|ui| { ui.strong("Kills"); });
                                             header.col(|ui| { ui.strong("Timestamp"); });
                                             header.col(|ui| { ui.strong("Dur."); });
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

                                                // ── [Row Number] ──────────────────────
                                                row.col(|ui| {
                                                    ui.label(format!("{}", row_idx + 1));
                                                });

                                                // ── [Checkbox] ────────────────────────
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
                                                    }
                                                });

                                                // ── [Kill Range] ───────────────────────
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
                                                                let mut guard = acquire_lock!(queued_demos_arc);
                                                                let queued = Arc::make_mut(&mut *guard);
                                                                let sm = &mut queued[d_idx].streaks[streak_idx];
                                                                sm.start_index = start_idx;
                                                                sm.end_index   = end_idx;
                                                                sm.update_visuals();
                                                            }
                                                        });

                                                        // Reset button — only visible when range is narrowed.
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

                                                // ── [Kills] ───────────────────────────
                                                row.col(|ui| { ui.label(streak.kill_count.to_string()); });

                                                // ── [Timestamp] ───────────────────────
                                                row.col(|ui| {
                                                    let absolute_timestamp = streak.viewdemo_times.get(streak.start_index).copied().unwrap_or(0.0);
                                                    let ts_secs = absolute_timestamp.round() as i32;
                                                    ui.label(format!("{}:{:02}", ts_secs / 60, ts_secs % 60));
                                                });                                        

                                                // ── [Duration] ────────────────────────
                                                row.col(|ui| { ui.label(&streak.duration_string); });

                                                // ── [Details / Timeline] ──────────────
                                                row.col(|ui| { ui.label(&streak.timeline_string); });
                                            });
                                        });
                                });
                                col_ui.add_space(4.0);
                            }
                        });
                    });

                // ── Execute all deferred actions post-loop ───────────────────────────
                // Process removals last (in reverse index order) to prevent index shift.
                let mut removals: Vec<usize> = actions_to_apply
                    .iter()
                    .filter_map(|a| if let DemoAction::RemoveDemo(i) = a { Some(*i) } else { None })
                    .collect();
                removals.sort_unstable_by(|a, b| b.cmp(a));

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
                        }
                        DemoAction::DeselectAll(idx) => {
                            let mut guard = acquire_lock!(queued_demos_arc);
                            let queued = Arc::make_mut(&mut *guard);
                            if let Some(demo) = queued.get_mut(*idx) {
                                for s in &mut demo.streaks { s.is_selected = false; }
                            }
                        }
                        DemoAction::RemoveDemo(_) => {} // handled below
                    }
                }
                for idx in removals {
                    let mut guard = acquire_lock!(queued_demos_arc);
                    let queued = Arc::make_mut(&mut *guard);
                    if idx < queued.len() {
                        queued.remove(idx);
                    }
                }

                // ── Global bulk-action bar ───────────────────────────────────────────
                ui.add_space(6.0);
                widgets::render_bulk_actions(ui, &queued_demos_arc);
            } else {
                ui.weak("No discovered highlight streaks. Go back to Scan and add demo files.");
            }
        });
    });

    ui.add_space(10.0);

    // ── Global Paths & Configuration ─────────────────────────────────────────────
        widgets::render_error_banner(ui, ctx, error_message);

        {
            let mut patcher_config = acquire_lock!(patcher_config_mutex);

            egui::CollapsingHeader::new("Recording Engine Configurations").default_open(true).show(ui, |ui| {
                panels::render_engine_config_panel(ui, &mut patcher_config, error_message);
            });

            ui.add_space(8.0);

            egui::CollapsingHeader::new("Highlight Capture Settings").default_open(true).show(ui, |ui| {
                panels::render_highlight_settings_panel(
                    ui,
                    ctx,
                    &mut patcher_config,
                    settings,
                    draft_settings,
                    subdir_cache,
                    tree_demo_cache,
                );
            });

            ui.add_space(8.0);

            egui::CollapsingHeader::new("⚡ Capture Configuration").default_open(true).show(ui, |ui| {
                panels::render_capture_config_panel(ui, ctx, &mut patcher_config);
            });

            ui.add_space(8.0);

            egui::CollapsingHeader::new("🐛 Debugging Settings").default_open(true).show(ui, |ui| {
                panels::render_debug_panel(ui, &mut patcher_config);
            });
        }

        ui.add_space(8.0);

        egui::CollapsingHeader::new("⚡ Export Configuration").default_open(true).show(ui, |ui| {
            let mut render_config = acquire_lock!(render_config_mutex);
            panels::render_export_config_panel(ui, ctx, &mut render_config);
        });

        ui.add_space(8.0);

        let selected_streaks_count = queued_demos_shared.iter()
            .flat_map(|d| &d.streaks)
            .filter(|s| s.is_selected)
            .count() as f32;

        {
            let mut patcher_config = acquire_lock!(patcher_config_mutex);
            let disk_estimate = widgets::compute_disk_estimate(&patcher_config, selected_streaks_count);
            widgets::render_disk_estimate_row(ui, &disk_estimate);

            ui.add_space(8.0);

            // ── Proceed to Capture Button + async patch_worker ───────────────────
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
}
