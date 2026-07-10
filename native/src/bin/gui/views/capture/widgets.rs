use std::sync::{Arc, Mutex, atomic::AtomicBool};
use native::patch::{PatcherConfig, CaptureStreak, build_batch_queue};
use crate::types::{DemoData, CaptureStudioState};
use super::{is_patching, set_is_patching};

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
    let is_running = is_patching();
    let queued_demos = super::acquire_lock!(queued_demos_shared).clone();

    let selected_streaks_count = queued_demos.iter()
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
    
    let is_missing_primary_dir = patcher_config.primary_media_dir.is_none();
    let check_path = patcher_config.primary_media_dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let available_bytes = if is_missing_primary_dir { 0 } else { native::sys::disk::get_available_bytes(&check_path) };
    
    let exceeds_space = required_bytes > available_bytes && available_bytes != u64::MAX;

    ui.horizontal(|ui| {
        let can_preview = !is_running && !queued_demos.is_empty();
        if ui.add_enabled(can_preview, egui::Button::new("🔍 Add Director Events for Previewing"))
            .on_hover_text("Patches all detected highlights as viewdemo Event List entries into a _preview.dem copy of each demo. No capture is triggered.")
            .clicked()
        {
            let mut preview_payload = Vec::new();
            for demo in queued_demos.iter() {
                let demo_path_str = demo.path.to_string_lossy().to_string();
                for streak in &demo.streaks {
                    if demo.is_pov && Some(streak.player_index) != demo.local_player_index {
                        continue;
                    }
                    preview_payload.push(CaptureStreak {
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
                        total_demo_frames: demo.playback_frames,
                        demo_fps: demo.tickrate,
                        viewdemo_times: streak.viewdemo_times.clone(),
                        frame_times: streak.frame_times.clone(),
                    });
                }
            }

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
                                Ok(()) => super::log_markdown(&format!("[PREVIEW PATCH] ✅ Done: {}", job.output_demo.display())),
                                Err(e) => super::log_markdown(&format!("[PREVIEW PATCH] ❌ Error: {}", e)),
                            }
                        }
                        let _ = tx_clone.send(crate::types::GuiMessage::PatchingComplete);
                        ctx_clone.request_repaint();
                    })
                    .unwrap();
            }
        }

        ui.add_space(16.0);

        let btn = egui::Button::new("Proceed to Capture ->");
        if ui.add_enabled(!is_running && !queued_demos.is_empty() && !exceeds_space && !is_missing_primary_dir, btn).clicked() {
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
            let mut payload = Vec::new();
            for demo in queued_demos.iter() {
                let demo_path_str = demo.path.to_string_lossy().to_string();
                for streak in &demo.streaks {
                    if !streak.is_selected {
                        continue;
                    }
                    if demo.is_pov && Some(streak.player_index) != demo.local_player_index {
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
                        total_demo_frames: demo.playback_frames,
                        demo_fps: demo.tickrate,
                        viewdemo_times: streak.viewdemo_times.clone(),
                        frame_times: streak.frame_times.clone(),
                    });
                }
            }

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
}
