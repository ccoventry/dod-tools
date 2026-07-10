use std::sync::{Arc, Mutex, atomic::AtomicBool};
use native::patch::{PatcherConfig, build_batch_queue};
use crate::types::{DemoData, CaptureStudioState};
use super::{is_patching, set_is_patching};
use super::payload::{build_capture_streak_payload, StreakFilter};

pub struct DiskEstimate {
    pub _required_bytes: u64,
    pub available_bytes: u64,
    pub required_gb: f64,
    pub available_gb: f64,
    pub exceeds_space: bool,
    pub is_missing_primary_dir: bool,
}

pub fn compute_disk_estimate(config: &PatcherConfig, selected_count: f32) -> DiskEstimate {
    let total_sequence_duration = selected_count * (config.pre_roll_seconds + config.post_roll_seconds + 10.0);
    let w = config.resolution_width;
    let h = config.resolution_height;
    let fps = config.capture_fps;
    let mut required_bytes = native::sys::disk::calculate_raw_sequence_bytes(w, h, fps, total_sequence_duration);
    if config.separate_hud {
        required_bytes *= 3;
    }
    
    let is_missing_primary_dir = config.primary_media_dir.is_none();
    let check_path = config.primary_media_dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let available_bytes = if is_missing_primary_dir { 0 } else { native::sys::disk::get_available_bytes(&check_path) };
    
    let exceeds_space = required_bytes > available_bytes && available_bytes != u64::MAX;
    
    let required_gb = required_bytes as f64 / 1_073_741_824.0;
    let available_gb = if available_bytes == u64::MAX { 999.9 } else { available_bytes as f64 / 1_073_741_824.0 };
    
    DiskEstimate {
        _required_bytes: required_bytes,
        available_bytes,
        required_gb,
        available_gb,
        exceeds_space,
        is_missing_primary_dir,
    }
}

pub fn render_disk_estimate_row(ui: &mut egui::Ui, estimate: &DiskEstimate) {
    ui.horizontal(|ui| {
        ui.strong("Disk Space Estimate:");
        if estimate.is_missing_primary_dir {
            ui.label(format!("Required: {:.1} GB / Available: N/A", estimate.required_gb));
        } else if estimate.available_bytes == u64::MAX {
            ui.label(format!("Required: {:.1} GB / Available: Unknown", estimate.required_gb));
        } else {
            let color = if estimate.exceeds_space { egui::Color32::RED } else { ui.visuals().text_color() };
            ui.colored_label(color, format!("Required: {:.1} GB / Available: {:.1} GB", estimate.required_gb, estimate.available_gb));
        }
    });

    if estimate.is_missing_primary_dir {
        ui.colored_label(egui::Color32::YELLOW, "⚠️ Please select a Primary Directory to enable capturing.");
    } else if estimate.exceeds_space {
        ui.colored_label(egui::Color32::RED, "⚠️ WARNING: Not enough free disk space on the target drive!");
    }
}


pub fn render_bulk_actions(
    ui: &mut egui::Ui,
    queued_demos_shared: &Arc<Mutex<Arc<Vec<DemoData>>>>,
) {
    ui.horizontal(|ui| {
        if ui.button("Clear All Discovered").clicked() {
            let mut guard = super::acquire_lock!(queued_demos_shared);
            let queued = Arc::make_mut(&mut *guard);
            queued.clear();
        }
        if ui.button("Select All").clicked() {
            let mut guard = super::acquire_lock!(queued_demos_shared);
            let queued = Arc::make_mut(&mut *guard);
            for d in queued.iter_mut() {
                for s in &mut d.streaks {
                    if d.is_pov && Some(s.player_index) != d.local_player_index {
                        continue;
                    }
                    s.is_selected = true;
                }
            }
        }
        if ui.button("Deselect All").clicked() {
            let mut guard = super::acquire_lock!(queued_demos_shared);
            let queued = Arc::make_mut(&mut *guard);
            for d in queued.iter_mut() {
                for s in &mut d.streaks {
                    s.is_selected = false;
                }
            }
        }
    });
}

pub fn render_primary_actions(
    ui: &mut egui::Ui,
    patcher_config: &mut PatcherConfig,
    state_ptr: &mut CaptureStudioState,
    loading_ptr: &mut bool,
    tx: &std::sync::mpsc::Sender<crate::types::GuiMessage>,
    queued_demos_shared: &Arc<Mutex<Arc<Vec<DemoData>>>>,
    ctx: &egui::Context,
) {
    let temp_id = egui::Id::new("dodtools_clear_previews_list");
    let clear_previews_list: Option<Vec<std::path::PathBuf>> = ctx.data(|d| d.get_temp(temp_id));

    let is_running = is_patching();
    let queued_demos = super::acquire_lock!(queued_demos_shared).clone();

    let selected_streaks_count = queued_demos.iter()
        .flat_map(|d| &d.streaks)
        .filter(|s| s.is_selected)
        .count() as f32;

    let disk_estimate = compute_disk_estimate(patcher_config, selected_streaks_count);

    ui.horizontal(|ui| {
        let can_preview = !is_running && !queued_demos.is_empty();
        if ui.add_enabled(can_preview, egui::Button::new("🔍 Add Director Events for Previewing"))
            .on_hover_text("Patches all detected highlights as viewdemo Event List entries into a _preview.dem copy of each demo. No capture is triggered.")
            .clicked()
        {
            let preview_payload = build_capture_streak_payload(
                &queued_demos,
                StreakFilter {
                    selected_only: false,
                    pov_local_only: true,
                },
            );

            if !preview_payload.is_empty() {
                use native::patch::build_preview_patch_jobs;
                let jobs = build_preview_patch_jobs(
                    preview_payload,
                    patcher_config.output_dir.as_deref(),
                );
                let tx_clone = tx.clone();
                let ctx_clone = ctx.clone();
                let cancel_token = Arc::new(AtomicBool::new(false));

                super::log_markdown(&format!("[PREVIEW PATCH] Injecting director events into {} demo(s).", jobs.len()));

                set_is_patching(true);
                std::thread::Builder::new()
                    .name("preview_patch_worker".into())
                    .spawn(move || {
                        for job in &jobs {
                            super::log_markdown(&format!("[PREVIEW PATCH] Writing: {}", job.output_demo.display()));
                            let patcher = native::patch::StreamPatcher::new(
                                &job.source_demo,
                                &job.output_demo,
                            );
                            match patcher.patch(job, &native::patch::PatcherConfig::default(), &cancel_token) {
                                Ok(()) => {
                                    super::log_markdown(&format!("[PREVIEW PATCH] ✅ Done: {}", job.output_demo.display()));
                                    let sidecar_path = job.output_demo.with_extension("dodtools_preview");
                                    let _ = std::fs::write(&sidecar_path, "");
                                }
                                Err(e) => super::log_markdown(&format!("[PREVIEW PATCH] ❌ Error: {}", e)),
                            }
                        }
                        let _ = tx_clone.send(crate::types::GuiMessage::PatchingComplete);
                        ctx_clone.request_repaint();
                    })
                    .unwrap();
            }
        }

        ui.add_space(8.0);

        if ui.button("🗑️ Clear Previews").clicked() {
            let mut verified_previews = Vec::new();
            let mut dirs_to_scan = std::collections::HashSet::new();
            if let Some(ref out_dir) = patcher_config.output_dir {
                dirs_to_scan.insert(out_dir.clone());
            }
            for demo in queued_demos.iter() {
                if let Some(parent) = demo.path.parent() {
                    dirs_to_scan.insert(parent.to_path_buf());
                }
            }
            for dir in dirs_to_scan {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                                if filename.ends_with("_preview.dem") {
                                    let sidecar = path.with_extension("dodtools_preview");
                                    if sidecar.exists() {
                                        verified_previews.push(path);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            ctx.data_mut(|d| d.insert_temp(temp_id, verified_previews));
        }

        ui.add_space(16.0);

        let btn = egui::Button::new("Proceed to Capture ->");
        if ui.add_enabled(!is_running && !queued_demos.is_empty() && !disk_estimate.exceeds_space && !disk_estimate.is_missing_primary_dir, btn).clicked() {
            #[cfg(not(target_arch = "wasm32"))]
            {
                use sysinfo::{System, SystemExt, DiskExt};
                let mut sys = System::new_all();
                sys.refresh_disks_list();
                
                let active_export_dir = patcher_config.primary_media_dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                let mut available_space = u64::MAX;
                let mut disk_found = false;
                for disk in sys.disks() {
                    if active_export_dir.starts_with(disk.mount_point()) {
                        available_space = disk.available_space();
                        disk_found = true;
                        break;
                    }
                }

                if disk_found && available_space < 15_u64 * 1024 * 1024 * 1024 {
                    log::warn!("Capture aborted: Target drive has less than 15GB free space.");
                    return;
                }
            }

            set_is_patching(true);

            crate::settings::save_patcher_config(&patcher_config);

            patcher_config.session_id = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();

            // Build the flat payload from all selected, filter-passing streaks.
            let payload = build_capture_streak_payload(
                &queued_demos,
                StreakFilter {
                    selected_only: true,
                    pov_local_only: true,
                },
            );

            if !payload.is_empty() {
                let cancel_token = Arc::new(AtomicBool::new(false));

                super::log_markdown(&format!("[CAPTURE CONFIG PAYLOAD] Pre: {}, Lead: {}, Trail: {}, Post: {}, FPS: {}, Auto-Quit: {}",
                    patcher_config.pre_roll_seconds,
                    patcher_config.record_start_lead,
                    patcher_config.record_stop_trail,
                    patcher_config.post_roll_seconds,
                    patcher_config.capture_fps,
                    patcher_config.exit_on_finish
                ));
                
                let jobs = match build_batch_queue(payload, &patcher_config) {
                    Ok(jobs) => jobs,
                    Err(e) => {
                        log::error!("Failed to write helper config: {}", e);
                        let _ = tx.send(crate::types::GuiMessage::CaptureEngineEvent(crate::types::EngineEvent::Error(format!("Failed to write helper config: {}", e))));
                        return;
                    }
                };
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
                // No selectable payload - skip patching and jump straight to Capture.
                set_is_patching(false);
                *state_ptr = CaptureStudioState::Capture;
            }
        }

        if ui.button("💾 Export Session").clicked() {
            let entries = queued_demos.iter().map(|d| {
                let highlights = d.streaks.iter().map(|s| {
                    crate::session::HighlightMetadata {
                        is_selected: s.is_selected,
                        start_kill: s.start_index as i32,
                        end_kill: s.end_index as i32,
                    }
                }).collect();
                crate::session::DemoEntry {
                    path: d.path.clone(),
                    key: native::utils::demo_hasher::calculate_demo_key(&d.path),
                    highlights,
                }
            }).collect();
            let session_data = crate::session::SessionData { entries };
            if let Some(path) = rfd::FileDialog::new().add_filter("JSON", &["json"]).save_file() {
                if let Ok(json) = serde_json::to_string_pretty(&session_data) {
                    let _ = std::fs::write(path, json);
                }
            }
        }

        if ui.button("📂 Import Session").clicked() {
            *loading_ptr = true;
            let ctx_clone = ctx.clone();
            let rules_clone = super::get_highlight_rules_clone();
            let tx_clone = tx.clone();
            let queued_demos_clone = queued_demos_shared.clone();
            std::thread::Builder::new()
                .name("rfd_dialog_import".into())
                .stack_size(8 * 1024 * 1024)
                .spawn(move || {
                    if let Some(json_path) = rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file() {
                        if let Ok(json) = std::fs::read_to_string(&json_path) {
                            if let Ok(session_data) = serde_json::from_str::<crate::session::SessionData>(&json) {
                                if let Some(base_dir) = rfd::FileDialog::new().pick_folder() {
                                    let rt = tokio::runtime::Runtime::new().unwrap();
                                    let resolved = rt.block_on(crate::session::import_session_async(base_dir, session_data.entries));
                                    if !resolved.is_empty() {
                                        let mut paths_to_ingest = Vec::new();
                                        {
                                            let mut guard = super::acquire_lock!(queued_demos_clone);
                                            let queued = Arc::make_mut(&mut *guard);
                                            for (path, metas) in resolved {
                                                if let Some(demo) = queued.iter_mut().find(|d| d.path == path) {
                                                    for (streak, meta) in demo.streaks.iter_mut().zip(metas) {
                                                        streak.is_selected = meta.is_selected;
                                                        streak.start_index = meta.start_kill as usize;
                                                        streak.end_index = meta.end_kill as usize;
                                                        streak.update_visuals();
                                                    }
                                                } else {
                                                    paths_to_ingest.push(path);
                                                }
                                            }
                                        }
                                        if !paths_to_ingest.is_empty() {
                                            super::spawn_ingestion_thread(
                                                super::IngestionInput::Batch(paths_to_ingest),
                                                rules_clone,
                                                ctx_clone,
                                                tx_clone,
                                            );
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let _ = tx_clone.send(crate::types::GuiMessage::IngestionFinished);
                })
                .unwrap();
        }

        if is_running {
            ui.add_space(10.0);
            ui.spinner();
            ui.label("Patching Demos... Please wait.");
        }
    });

    if let Some(ref list) = clear_previews_list {
        let mut open = true;
        egui::Window::new("Audit Preview Files to Delete")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(format!("Found {} verified preview files to delete:", list.len()));
                ui.add_space(8.0);
                
                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                    for path in list {
                        ui.label(path.to_string_lossy());
                    }
                });
                
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button(format!("Delete {} Files", list.len())).clicked() {
                        for path in list {
                            let _ = std::fs::remove_file(path);
                            let _ = std::fs::remove_file(path.with_extension("dodtools_preview"));
                        }
                        ctx.data_mut(|d| d.remove_temp::<Vec<std::path::PathBuf>>(temp_id));
                    }
                    if ui.button("Cancel").clicked() {
                        ctx.data_mut(|d| d.remove_temp::<Vec<std::path::PathBuf>>(temp_id));
                    }
                });
            });
        if !open {
            ctx.data_mut(|d| d.remove_temp::<Vec<std::path::PathBuf>>(temp_id));
        }
    }
}

pub fn render_error_banner(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    error_message: &mut Option<String>,
) {
    if let Some(error) = error_message.clone() {
        let mut dismiss = false;
        egui::Frame::NONE
            .fill(ui.visuals().faint_bg_color)
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(239, 68, 68)))
            .corner_radius(6.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Configuration Error")
                            .heading()
                            .color(egui::Color32::from_rgb(239, 68, 68)),
                    );
                    ui.add_space(8.0);
                    ui.label(&error);
                    ui.add_space(12.0);
                    if ui.button("Dismiss").clicked() {
                        dismiss = true;
                    }
                });
            });
        if dismiss {
            *error_message = None;
            ctx.request_repaint();
        }
    }
}

pub fn render_path_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    filter_name: &str,
    filter_exts: &[&str],
    expected_filename: Option<&str>,
    error_message: &mut Option<String>,
) {
    ui.label(label);
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(value).desired_width(ui.available_width() - 80.0));
        #[cfg(not(target_arch = "wasm32"))]
        {
            if ui.button("Browse...").clicked() {
                let mut dialog = rfd::FileDialog::new();
                if !filter_exts.is_empty() {
                    dialog = dialog.add_filter(filter_name, filter_exts);
                }
                if let Some(path) = dialog.pick_file() {
                    if let Some(expected) = expected_filename {
                        let is_expected = path.file_name()
                            .and_then(|n| n.to_str())
                            .map(|s| s.to_lowercase()) == Some(expected.to_lowercase());
                        if is_expected {
                            *value = path.to_string_lossy().to_string();
                        } else {
                            *error_message = Some(format!("Selected file must be {}", expected));
                        }
                    } else {
                        *value = path.to_string_lossy().to_string();
                    }
                }
            }
        }
    });
}

pub fn render_dir_picker_row(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    label: &str,
    picker: &mut egui_file_dialog::FileDialog,
    current_path: &mut Option<std::path::PathBuf>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.button("📁 Select...").clicked() {
            picker.pick_directory();
        }
        if let Some(path) = current_path {
            ui.label(path.to_string_lossy());
        } else {
            ui.colored_label(egui::Color32::YELLOW, "Warning: Defaulting to OS Drive");
        }
    });
    picker.update(ctx);
    if let Some(path) = picker.take_picked() {
        *current_path = Some(path.to_path_buf());
        changed = true;
    }
    changed
}

pub fn render_command_list(
    ui: &mut egui::Ui,
    id_salt: &str,
    commands: &mut Vec<String>,
    max_height: f32,
) {
    let mut delete_idx = None;
    
    egui::ScrollArea::vertical()
        .max_height(max_height)
        .id_salt(id_salt)
        .show(ui, |ui| {
            for (i, cmd) in commands.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(cmd).desired_width(ui.available_width() - 40.0));
                    if ui.button("❌").clicked() {
                        delete_idx = Some(i);
                    }
                });
            }
        });

    if let Some(i) = delete_idx {
        commands.remove(i);
    }
    if ui.button("➕ Add Command").clicked() {
        commands.push("".to_string());
    }
}

pub fn render_custom_command_list(
    ui: &mut egui::Ui,
    id_salt: &str,
    commands: &mut Vec<native::patch::CustomCommand>,
    max_height: f32,
) {
    let mut delete_idx = None;
    
    egui::ScrollArea::vertical()
        .max_height(max_height)
        .id_salt(id_salt)
        .show(ui, |ui| {
            for (i, cmd) in commands.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut cmd.command).desired_width(120.0));
                    
                    let is_after = cmd.relation == native::patch::CommandRelation::After;
                    if ui.selectable_label(!is_after, "B").on_hover_text("Before Highlight").clicked() {
                        cmd.relation = native::patch::CommandRelation::Before;
                    }
                    if ui.selectable_label(is_after, "A").on_hover_text("After Highlight").clicked() {
                        cmd.relation = native::patch::CommandRelation::After;
                    }
                    
                    ui.add(egui::DragValue::new(&mut cmd.offset).speed(0.1).range(0.0..=60.0).suffix("s"));
                    if ui.button("❌").clicked() {
                        delete_idx = Some(i);
                    }
                });
            }
        });

    if let Some(i) = delete_idx {
        commands.remove(i);
    }
    if ui.button("➕ Add Default").clicked() {
        commands.push(native::patch::CustomCommand {
            command: "".to_string(),
            offset: 2.0,
            relation: native::patch::CommandRelation::Before,
        });
    }
}

