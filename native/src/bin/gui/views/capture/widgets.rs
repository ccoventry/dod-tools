use std::sync::{Arc, Mutex, atomic::AtomicBool};
use native::patch::{PatcherConfig, build_batch_queue};
use crate::types::{DemoData, CaptureStudioState};
use super::{is_patching, set_is_patching};
use super::payload::{build_capture_streak_payload, StreakFilter};


pub fn render_bulk_actions(
    ui: &mut egui::Ui,
    queued_demos_shared: &Arc<Mutex<Arc<Vec<DemoData>>>>,
) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        if ui.button("Clear All Discovered").clicked() {
            let mut guard = match queued_demos_shared.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let queued = Arc::make_mut(&mut *guard);
            queued.clear();
            clicked = true;
        }
        if ui.button("Select All").clicked() {
            let mut guard = match queued_demos_shared.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let queued = Arc::make_mut(&mut *guard);
            for d in queued.iter_mut() {
                for s in &mut d.streaks {
                    if d.is_pov && Some(s.player_index) != d.local_player_index {
                        continue;
                    }
                    s.is_selected = true;
                }
            }
            clicked = true;
        }
        if ui.button("Deselect All").clicked() {
            let mut guard = match queued_demos_shared.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let queued = Arc::make_mut(&mut *guard);
            for d in queued.iter_mut() {
                for s in &mut d.streaks {
                    s.is_selected = false;
                }
            }
            clicked = true;
        }
    });
    clicked
}

pub fn render_primary_actions(
    ui: &mut egui::Ui,
    patcher_config: &mut PatcherConfig,
    state_ptr: &mut CaptureStudioState,
    _loading_ptr: &mut bool,
    tx: &std::sync::mpsc::Sender<crate::types::GuiMessage>,
    queued_demos_shared: &Arc<Mutex<Arc<Vec<DemoData>>>>,
    ctx: &egui::Context,
) {
    let temp_id = egui::Id::new("dodtools_clear_previews_list");
    let clear_previews_list: Option<Vec<std::path::PathBuf>> = ctx.data(|d| d.get_temp(temp_id));

    let is_running = is_patching();
    let queued_demos = match queued_demos_shared.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }.clone();
    ui.horizontal(|ui| {
        let can_preview = !is_running && !queued_demos.is_empty();
        let show_preview_success: bool = ctx.data(|d| d.get_temp(egui::Id::new("dodtools_preview_success_msg")).unwrap_or(false));
        if ui.add_enabled(can_preview, egui::Button::new("🔍 Create Previews for all loaded demos"))
            .on_hover_text("Patches all detected highlights as viewdemo Event List entries into a _preview.dem copy of each demo. No capture is triggered.")
            .clicked()
        {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("dodtools_preview_success_msg"), false));
            let preview_payload = build_capture_streak_payload(
                &queued_demos,
                StreakFilter {
                    selected_only: false,
                    pov_local_only: true,
                },
            );

            if !preview_payload.is_empty() {
                use native::patch::build_preview_patch_jobs;
                let game_path_buf = std::path::PathBuf::from(&patcher_config.game_path);
                let dod_dir = game_path_buf.parent().unwrap_or(std::path::Path::new("")).join("dod");
                let jobs = build_preview_patch_jobs(
                    preview_payload,
                    Some(dod_dir.as_path()),
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
                                    let _ = (|| -> std::io::Result<()> {
                                        #[cfg(windows)]
                                        use std::os::windows::fs::OpenOptionsExt;
                                        use std::fs::OpenOptions;

                                        let mut options = OpenOptions::new();
                                        options.write(true).create(true).truncate(true);

                                        #[cfg(windows)]
                                        options.custom_flags(0x00000002); // FILE_ATTRIBUTE_HIDDEN

                                        let _file = options.open(&sidecar_path)?;
                                        Ok(())
                                    })();
                                }
                                Err(e) => super::log_markdown(&format!("[PREVIEW PATCH] ❌ Error: {}", e)),
                            }
                        }
                        ctx_clone.data_mut(|d| d.insert_temp(egui::Id::new("dodtools_preview_success_msg"), true));
                        let _ = tx_clone.send(crate::types::GuiMessage::PreviewPatchingComplete);
                        ctx_clone.request_repaint();
                    })
                    .unwrap();
            }
        }

        ui.add_space(8.0);

        if show_preview_success {
            ui.weak("Previews Generated. Click [▶ Preview] on a demo row, or manually load the _preview.dem files in-game.");
            ui.add_space(8.0);
        }

        if ui.button("🗑️ Clear Previews").clicked() {
            let mut verified_previews = Vec::new();
            let mut dirs_to_scan = std::collections::HashSet::new();
            let game_path_buf = std::path::PathBuf::from(&patcher_config.game_path);
            let dod_dir = game_path_buf.parent().unwrap_or(std::path::Path::new("")).join("dod");
            dirs_to_scan.insert(dod_dir);
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

        let cache_id = egui::Id::new("dodtools_disk_estimate_cache");
        let required_bytes = ctx.data(|d| d.get_temp::<u64>(cache_id)).unwrap_or(0);
        let mut available_bytes: u64 = 0;
        for path in &patcher_config.capture_directories {
            let free_bytes = native::sys::disk::get_available_bytes(path);
            if free_bytes != u64::MAX {
                available_bytes += free_bytes;
            }
        }

        ui.add_enabled_ui(required_bytes <= available_bytes, |ui| {
            let btn = egui::Button::new("Proceed to Capture ->");
            if ui.add_enabled(!is_running && !queued_demos.is_empty() && !patcher_config.capture_directories.is_empty(), btn).clicked() {
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
                    
                    let tx_clone = tx.clone();
                    let ctx_clone = ctx.clone();
                    let config_clone = patcher_config.clone();

                    std::thread::Builder::new()
                        .name("patch_worker".into())
                        .spawn(move || {
                            let jobs = match build_batch_queue(payload, &config_clone) {
                                Ok(jobs) => jobs,
                                Err(e) => {
                                    let err_msg = format!("Failed to build batch queue (capacity simulation may have failed): {}", e);
                                    log::error!("{}", err_msg);
                                    let _ = tx_clone.send(crate::types::GuiMessage::CaptureEngineEvent(crate::types::EngineEvent::Error(err_msg)));
                                    ctx_clone.request_repaint();
                                    return;
                                }
                            };

                            let total_jobs = jobs.len();
                            for (i, job) in jobs.into_iter().enumerate() {
                                ctx_clone.data_mut(|d| d.insert_temp(egui::Id::new("dodtools_patch_progress"), format!("Patching demo {} of {}... Please wait.", i + 1, total_jobs)));
                                ctx_clone.request_repaint();

                                let patcher = native::patch::StreamPatcher::new(
                                    &job.source_demo,
                                    &job.output_demo,
                                );
                                if let Err(e) = patcher.patch(&job, &config_clone, &cancel_token) {
                                    let _ = tx_clone.send(crate::types::GuiMessage::CaptureEngineEvent(crate::types::EngineEvent::Error(format!(
                                        "Patching failed for {}: {}",
                                        job.source_demo, e
                                    ))));
                                    ctx_clone.request_repaint();
                                    return;
                                }
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
        });


        if is_running {
            ui.add_space(10.0);
            ui.spinner();
            let progress_msg = ctx.data(|d| d.get_temp::<String>(egui::Id::new("dodtools_patch_progress"))).unwrap_or_else(|| "Patching Demos... Please wait for a few minutes now.".to_string());
            ui.label(progress_msg);
        }

        let current_state = *state_ptr;
        if current_state == CaptureStudioState::Select {
            if let Some(err) = ctx.data(|d| d.get_temp::<String>(egui::Id::new("dodtools_patch_error"))) {
                ui.add_space(4.0);
                ui.colored_label(egui::Color32::RED, format!("⚠ {}", err));
            }
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

