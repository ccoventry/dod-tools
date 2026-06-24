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

use std::sync::{Arc, Mutex, atomic::AtomicBool};
use native::patch::{PatcherConfig, CaptureStreak, build_batch_queue};
use crate::types::{DemoData, CaptureStudioState};
use super::{is_patching, set_is_patching, acquire_lock};
use super::log_markdown;
use egui_extras::{TableBuilder, Column};

fn create_pinned_file_dialog() -> egui_file_dialog::FileDialog {
    let mut fd = egui_file_dialog::FileDialog::new();
    let global = crate::settings::load_settings();
    if !global.pinned_folders.is_empty() {
        fd = fd.add_quick_access("Bookmarks", |s| {
            for path in &global.pinned_folders {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                s.add_path(&name, path.clone());
            }
        });
    }
    fd
}

pub fn render(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state_ptr: &mut CaptureStudioState,
    tx: std::sync::mpsc::Sender<crate::types::GuiMessage>,
    loading_ptr: &mut bool,
    hide_non_pov: &mut bool,
    queued_demos_arc: Arc<Mutex<Arc<Vec<DemoData>>>>,
    patcher_config_mutex: &'static Mutex<PatcherConfig>,
    render_config_mutex: &'static Mutex<native::hlcr::config::RenderConfig>,
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

    let mut patcher_config = acquire_lock!(patcher_config_mutex);

    // ── Step 2 header group ──────────────────────────────────────────────────────
    ui.group(|ui| {
        ui.vertical(|ui| {
            ui.heading("📂 Step 2: Select Highlights & Patch");
            ui.add_space(4.0);
            ui.label("Enable streaks, adjust patch parameters, and launch patcher.");
            ui.add_space(8.0);

            ui.checkbox(hide_non_pov, "Hide Non-Recording Players in POV Demos");
            ui.add_space(8.0);

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
                                        ui.strong(&demo.demo_name);
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
                                        .column(Column::auto())          // [Checkbox]
                                        .column(Column::auto())          // [Player Name]
                                        .column(Column::exact(140.0))    // [Kill Range]
                                        .column(Column::exact(50.0))     // [Duration]
                                        .column(Column::remainder())     // [Details]
                                        .header(20.0, |mut header| {
                                            header.col(|ui| { ui.strong("Sel"); });
                                            header.col(|ui| { ui.strong("Player"); });
                                            header.col(|ui| { ui.strong("Kill Range"); });
                                            header.col(|ui| { ui.strong("Dur."); });
                                            header.col(|ui| { ui.strong("Details"); });
                                        })
                                        .body(|body| {
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
                                                        log_markdown(&format!(
                                                            "UI Interaction: Toggled streak selection for {}, new value: {}",
                                                            streak.target_player, is_selected
                                                        ));
                                                        let mut guard = acquire_lock!(queued_demos_arc);
                                                        let queued = Arc::make_mut(&mut *guard);
                                                        queued[d_idx].streaks[streak_idx].is_selected = is_selected;
                                                    }
                                                });

                                                // ── [Player Name] ──────────────────────
                                                row.col(|ui| { ui.label(&streak.target_player); });

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
                                for s in &mut demo.streaks { s.is_selected = true; }
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
                ui.horizontal(|ui| {
                    if ui.button("Clear All Discovered").clicked() {
                        let mut guard = acquire_lock!(queued_demos_arc);
                        let queued = Arc::make_mut(&mut *guard);
                        queued.clear();
                    }
                    if ui.button("Select All").clicked() {
                        let mut guard = acquire_lock!(queued_demos_arc);
                        let queued = Arc::make_mut(&mut *guard);
                        for d in queued.iter_mut() {
                            for s in &mut d.streaks { s.is_selected = true; }
                        }
                    }
                    if ui.button("Deselect All").clicked() {
                        let mut guard = acquire_lock!(queued_demos_arc);
                        let queued = Arc::make_mut(&mut *guard);
                        for d in queued.iter_mut() {
                            for s in &mut d.streaks { s.is_selected = false; }
                        }
                    }
                });
            } else {
                ui.weak("No discovered highlight streaks. Go back to Scan and add demo files.");
            }
        });
    });

    ui.add_space(10.0);

    // ── Export Configuration ─────────────────────────────────────────────────────
    ui.group(|ui| {
        ui.vertical(|ui| {
            ui.heading("⚡ Capture Configuration");
            ui.add_space(4.0);

            static CAPTURE_PRIMARY_PICKER: std::sync::OnceLock<Mutex<egui_file_dialog::FileDialog>> = std::sync::OnceLock::new();
            static CAPTURE_BACKUP_PICKER: std::sync::OnceLock<Mutex<egui_file_dialog::FileDialog>> = std::sync::OnceLock::new();
            
            let mut cap_primary_picker = acquire_lock!(CAPTURE_PRIMARY_PICKER.get_or_init(|| Mutex::new(create_pinned_file_dialog())));
            ui.horizontal(|ui| {
                ui.label("Primary Capture Directory (Raw BMPs):");
                if ui.button("📁 Select...").clicked() {
                    cap_primary_picker.pick_directory();
                }
                if let Some(path) = &patcher_config.primary_media_dir {
                    ui.label(path.to_string_lossy());
                }
            });
            cap_primary_picker.update(ctx);
            if let Some(path) = cap_primary_picker.take_picked() {
                patcher_config.primary_media_dir = Some(path.to_path_buf());
                let mut global = crate::settings::load_settings();
                global.primary_media_dir = Some(path.to_string_lossy().into_owned());
                crate::settings::save_settings(&global);
            }

            let mut cap_backup_picker = acquire_lock!(CAPTURE_BACKUP_PICKER.get_or_init(|| Mutex::new(create_pinned_file_dialog())));
            ui.horizontal(|ui| {
                ui.label("Backup Capture Directory:");
                if ui.button("📁 Select...").clicked() {
                    cap_backup_picker.pick_directory();
                }
                if let Some(path) = &patcher_config.backup_media_dir {
                    ui.label(path.to_string_lossy());
                }
            });
            cap_backup_picker.update(ctx);
            if let Some(path) = cap_backup_picker.take_picked() {
                patcher_config.backup_media_dir = Some(path.to_path_buf());
                let mut global = crate::settings::load_settings();
                global.backup_media_dir = Some(path.to_string_lossy().into_owned());
                crate::settings::save_settings(&global);
            }

            ui.add_space(8.0);

            // Row 1: Pre-roll / Post-roll
            ui.horizontal(|ui| {
                ui.label("Pre-roll (sec):");
                ui.add(egui::DragValue::new(&mut patcher_config.pre_roll_seconds)
                    .range(0.0..=10.0).speed(0.1));
                ui.add_space(10.0);
                ui.label("Post-roll (sec):");
                ui.add(egui::DragValue::new(&mut patcher_config.post_roll_seconds)
                    .range(0.0..=10.0).speed(0.1));
            });

            // Row 2: Record Start Lead / Record Stop Trail
            ui.horizontal(|ui| {
                ui.label("Record Start Lead (sec):");
                ui.add(egui::DragValue::new(&mut patcher_config.record_start_lead)
                    .range(0.0..=10.0).speed(0.1));
                ui.add_space(10.0);
                ui.label("Record Stop Trail (sec):");
                ui.add(egui::DragValue::new(&mut patcher_config.record_stop_trail)
                    .range(0.0..=10.0).speed(0.1));
            });

            // Row 3: Initial Load Delay / Fast Forward Speed
            ui.horizontal(|ui| {
                ui.label("Initial Load Delay (sec):");
                ui.add(egui::DragValue::new(&mut patcher_config.initial_delay)
                    .range(0.0..=10.0).speed(0.1));
                ui.add_space(10.0);
                ui.label("Fast Forward Speed:");
                ui.add(egui::DragValue::new(&mut patcher_config.fast_forward_speed)
                    .range(0.01..=10.0).speed(0.01));
            });

            // Row 4: Capture FPS
            ui.horizontal(|ui| {
                ui.label("Capture FPS:");
                ui.add(egui::DragValue::new(&mut patcher_config.capture_fps)
                    .range(30..=1000).speed(1));
            });

            // Row 5: Separate HUD
            ui.horizontal(|ui| {
                ui.checkbox(&mut patcher_config.separate_hud, "Separate HUD (Alpha & Color)")
                    .on_hover_text("This toggle acts as the absolute source of truth and will override any separate_hud settings in your movie.cfg.");
            });

            ui.add_space(8.0);

            ui.heading("⚡ Export Configuration");

            // Row 6: HLCR Routing & Codec
            let mut render_config = acquire_lock!(render_config_mutex);
            ui.horizontal(|ui| {
                ui.label("Render Codec:");
                egui::ComboBox::from_id_salt("render_codec_combo")
                    .selected_text(format!("{:?}", render_config.target_codec))
                    .show_ui(ui, |ui| {
                        let mut changed = false;
                        changed |= ui.selectable_value(&mut render_config.target_codec, native::hlcr::config::RenderCodec::ProRes, "ProRes").changed();
                        changed |= ui.selectable_value(&mut render_config.target_codec, native::hlcr::config::RenderCodec::NvencH264, "NvencH264").changed();
                        changed |= ui.selectable_value(&mut render_config.target_codec, native::hlcr::config::RenderCodec::DnxHr, "DnxHr").changed();
                        if changed {
                            let _ = native::hlcr::config::save_config(&render_config);
                        }
                    });
            });

            static PRIMARY_PICKER: std::sync::OnceLock<Mutex<egui_file_dialog::FileDialog>> = std::sync::OnceLock::new();
            static BACKUP_PICKER: std::sync::OnceLock<Mutex<egui_file_dialog::FileDialog>> = std::sync::OnceLock::new();
            
            let mut primary_picker = acquire_lock!(PRIMARY_PICKER.get_or_init(|| Mutex::new(create_pinned_file_dialog())));
            ui.horizontal(|ui| {
                ui.label("Primary Export Directory (Final .mov):");
                if ui.button("📁 Select...").clicked() {
                    primary_picker.pick_directory();
                }
                if let Some(path) = &render_config.primary_export_dir {
                    ui.label(path.to_string_lossy());
                }
            });
            primary_picker.update(ctx);
            if let Some(path) = primary_picker.take_picked() {
                render_config.primary_export_dir = Some(path.to_path_buf());
                let _ = native::hlcr::config::save_config(&render_config);
            }

            let mut backup_picker = acquire_lock!(BACKUP_PICKER.get_or_init(|| Mutex::new(create_pinned_file_dialog())));
            ui.horizontal(|ui| {
                ui.label("Backup Export Directory:");
                if ui.button("📁 Select...").clicked() {
                    backup_picker.pick_directory();
                }
                if let Some(path) = &render_config.backup_export_dir {
                    ui.label(path.to_string_lossy());
                }
            });
            backup_picker.update(ctx);
            if let Some(path) = backup_picker.take_picked() {
                render_config.backup_export_dir = Some(path.to_path_buf());
                let _ = native::hlcr::config::save_config(&render_config);
            }

            ui.add_space(8.0);

            let selected_streaks_count = queued_demos_shared.iter()
                .flat_map(|d| &d.streaks)
                .filter(|s| s.is_selected)
                .count() as f32;

            let total_sequence_duration = selected_streaks_count * (patcher_config.pre_roll_seconds + patcher_config.post_roll_seconds + 10.0);
            let w = patcher_config.resolution_width;
            let h = patcher_config.resolution_height;
            let fps = patcher_config.capture_fps;
            let mut required_bytes = native::sys::disk::calculate_raw_sequence_bytes(w, h, fps, total_sequence_duration);
            if patcher_config.separate_hud {
                required_bytes *= 3;
            }
            let required_gb = required_bytes as f64 / 1_073_741_824.0;
            
            let is_missing_primary_dir = patcher_config.primary_media_dir.is_none();
            let check_path = patcher_config.primary_media_dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let available_bytes = if is_missing_primary_dir { 0 } else { native::sys::disk::get_available_bytes(&check_path) };
            let available_gb = if available_bytes == u64::MAX { 999.9 } else { available_bytes as f64 / 1_073_741_824.0 };
            
            let exceeds_space = required_bytes > available_bytes && available_bytes != u64::MAX;

            ui.horizontal(|ui| {
                ui.strong("Disk Space Estimate:");
                if is_missing_primary_dir {
                    ui.label(format!("Required: {:.1} GB / Available: N/A", required_gb));
                } else if available_bytes == u64::MAX {
                    ui.label(format!("Required: {:.1} GB / Available: Unknown", required_gb));
                } else {
                    let color = if exceeds_space { egui::Color32::RED } else { ui.visuals().text_color() };
                    ui.colored_label(color, format!("Required: {:.1} GB / Available: {:.1} GB", required_gb, available_gb));
                }
            });

            if is_missing_primary_dir {
                ui.colored_label(egui::Color32::YELLOW, "⚠️ Please select a Primary Directory to enable capturing.");
            } else if exceeds_space {
                ui.colored_label(egui::Color32::RED, "⚠️ WARNING: Not enough free disk space on the target drive!");
            }

            ui.add_space(8.0);

            // ── Proceed to Capture Button + async patch_worker ───────────────────
            let is_running = is_patching();

            ui.horizontal(|ui| {
                let btn = egui::Button::new("Proceed to Capture ->");
                if ui.add_enabled(!is_running && !queued_demos_shared.is_empty() && !exceeds_space && !is_missing_primary_dir, btn).clicked() {
                    set_is_patching(true);

                    // Build the flat payload from all selected, filter-passing streaks.
                    let mut payload = Vec::new();
                    for demo in queued_demos_shared.iter() {
                        let demo_path_str = demo.path.to_string_lossy().to_string();
                        for streak in &demo.streaks {
                            if !streak.is_selected {
                                continue;
                            }
                            if demo.is_pov && *hide_non_pov
                                && Some(streak.player_index) != demo.local_player_index
                            {
                                continue;
                            }
                            payload.push(CaptureStreak {
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

                    if !payload.is_empty() {
                        let cancel_token = Arc::new(AtomicBool::new(false));
                        let jobs = build_batch_queue(payload, &patcher_config);
                        let tx_clone = tx.clone();
                        let ctx_clone = ctx.clone();
                        let config_clone = patcher_config.clone();

                        std::thread::Builder::new()
                            .name("patch_worker".into())
                            .spawn(move || {
                                for job in jobs {
                                    let patcher = native::patch::StreamPatcher::new(
                                        &job.source_demo,
                                        &job.output_demo,
                                    );
                                    let _ = patcher.patch(&job, &config_clone, &cancel_token);
                                }
                                let _ = tx_clone.send(crate::types::GuiMessage::PatchingComplete);
                                ctx_clone.request_repaint();
                            })
                            .unwrap();
                    } else {
                        // No selectable payload — skip patching and jump straight to Capture.
                        set_is_patching(false);
                        *state_ptr = CaptureStudioState::Capture;
                    }
                }

                if is_running {
                    ui.add_space(10.0);
                    ui.spinner();
                    ui.label("Patching Demos... Please wait.");
                }
            });
        });
    });
}
