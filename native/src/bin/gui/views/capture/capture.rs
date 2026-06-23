// ============================================================
// views/capture/capture.rs
// Stateless render function for CaptureStudioState::Capture.
//
// Extracted from capture_studio.rs (Stage 2 refactor).
// Takes fine-grained field references rather than &mut Gui to
// satisfy the borrow checker at the call site.
// ============================================================

/// Render the HLAE Game Capture Engine step.
///
/// # Parameters
/// - `settings`              — mutable ref to `Gui.settings` (game_path, hlae_path, save_settings)
/// - `capture_engine_running` — read to gate Launch/Proceed; written eagerly before spawn
/// - `engine_msg/progress/done/total` — display-only progress fields
/// - `hide_non_pov`          — streak filter flag (read-only)
/// - `tx`                    — GuiMessage sender for the relay thread
/// - `studio_state`          — written to CaptureStudioState::Render on Proceed
#[cfg(not(target_arch = "wasm32"))]
pub fn render(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    settings: &mut crate::settings::AppSettings,
    capture_engine_running: &mut bool,
    engine_msg: &str,
    engine_progress: f32,
    engine_jobs_done: usize,
    engine_jobs_total: usize,
    hide_non_pov: bool,
    tx: std::sync::mpsc::Sender<crate::types::GuiMessage>,
    studio_state: &mut crate::types::CaptureStudioState,
) {
    ui.vertical(|ui| {
        // CRITICAL: Engine relies on HLAE injection for mirv_streams. Do not rename to 'Native'.
        ui.heading("🎬 HLAE Game Capture Engine");
        ui.add_space(8.0);

        // ── Paths configuration ──────────────────────────────────────────────
        ui.group(|ui| {
            ui.strong("Paths Configuration");
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("hl.exe Path:");
                // Draft-only update on keystroke — no disk write.
                ui.text_edit_singleline(&mut settings.game_path);
                if ui.button("Browse...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("hl.exe", &["exe"])
                        .pick_file()
                    {
                        settings.game_path = path.to_string_lossy().to_string();
                        // Save only on explicit Browse dialog confirmation.
                        crate::settings::save_settings(settings);
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label("hlae.exe Path:");
                // Draft-only update on keystroke — no disk write.
                ui.text_edit_singleline(&mut settings.hlae_path);
                if ui.button("Browse...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("hlae.exe", &["exe"])
                        .pick_file()
                    {
                        settings.hlae_path = path.to_string_lossy().to_string();
                        // Save only on explicit Browse dialog confirmation.
                        crate::settings::save_settings(settings);
                    }
                }
            });
        });

        ui.add_space(12.0);

        // ── Launch controls ──────────────────────────────────────────────────
        let hl_exists = !settings.game_path.is_empty()
            && std::path::Path::new(&settings.game_path).exists();
        let hlae_exists = !settings.hlae_path.is_empty()
            && std::path::Path::new(&settings.hlae_path).exists();
        let can_launch = hl_exists && hlae_exists && !*capture_engine_running;

        ui.horizontal(|ui| {
            if ui.add_enabled(can_launch, egui::Button::new("🎬 Launch Capture Engine")).clicked() {
                let hlae_path = std::sync::Arc::new(std::path::PathBuf::from(&settings.hlae_path));
                let hl_path = std::sync::Arc::new(std::path::PathBuf::from(&settings.game_path));
                let dod_dir = hl_path.parent().unwrap().join("dod");

                // Build raw streak list from the shared queued demos mutex.
                let mut raw_streaks = Vec::new();
                {
                    let queued_demos_arc = crate::views::capture::get_queued_demos();
                    let queued_demos = match queued_demos_arc.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            log::error!("Mutex poisoned, attempting recovery...");
                            poisoned.into_inner()
                        }
                    };
                    for demo in queued_demos.iter() {
                        let demo_path_str = demo.path.to_string_lossy().to_string();
                        for streak in &demo.streaks {
                            if streak.is_selected {
                                if hide_non_pov
                                    && demo.is_pov
                                    && Some(streak.player_index) != demo.local_player_index
                                {
                                    continue;
                                }
                                raw_streaks.push(native::patch::CaptureStreak {
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
                }

                // Resolve patcher config and convert seconds → ticks.
                let patcher_config_mutex = crate::views::capture::get_patcher_config();
                let mut patcher_config = match patcher_config_mutex.lock() {
                    Ok(guard) => guard.clone(),
                    Err(poisoned) => {
                        log::error!("Mutex poisoned, attempting recovery...");
                        poisoned.into_inner().clone()
                    }
                };
                // Standard DoD demo tickrate is 100.0.
                patcher_config.pre_roll_ticks = (patcher_config.pre_roll_seconds * 100.0) as i32;
                patcher_config.post_roll_ticks = (patcher_config.post_roll_seconds * 100.0) as i32;

                let patch_jobs = native::patch::build_batch_queue(raw_streaks, &patcher_config);

                let mut capture_jobs = Vec::new();
                for job in patch_jobs {
                    let demo_filename = job.output_demo.file_name().unwrap();
                    let demo_name_no_ext = match std::path::Path::new(demo_filename).file_stem() {
                        Some(stem) => stem.to_string_lossy().to_string(),
                        None => demo_filename.to_string_lossy().to_string(),
                    };
                    let expected_take_folder = dod_dir.join("hlcr_captures").join(&demo_name_no_ext);
                    capture_jobs.push(crate::types::CaptureJob {
                        patched_demo_path: job.output_demo,
                        expected_take_folder,
                    });
                }

                if !capture_jobs.is_empty() {
                    // Eagerly set the flag before thread spawn to close the
                    // one-frame window where a second click would be possible
                    // before EngineEvent::Starting arrives (Stage 1 fix #6).
                    *capture_engine_running = true;

                    let (engine_tx, engine_rx) = std::sync::mpsc::channel();
                    let gui_tx = tx.clone();
                    let ctx_clone = ctx.clone();

                    std::thread::spawn(move || {
                        while let Ok(event) = engine_rx.recv() {
                            let _ = gui_tx.send(crate::types::GuiMessage::CaptureEngineEvent(event));
                            ctx_clone.request_repaint();
                        }
                    });

                    crate::capture_engine::spawn_capture_engine(
                        capture_jobs,
                        hlae_path,
                        hl_path,
                        engine_tx,
                    );
                }
            }

            // Disabled while engine is running to prevent advancing mid-capture
            // (Stage 1 fix #2).
            if ui.add_enabled(
                !*capture_engine_running,
                egui::Button::new("Proceed to Render ->"),
            ).clicked() {
                *studio_state = crate::types::CaptureStudioState::Render;
            }
        });

        ui.add_space(12.0);

        // ── Progress display ─────────────────────────────────────────────────
        if *capture_engine_running || !engine_msg.is_empty() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    if *capture_engine_running {
                        ui.spinner();
                    }
                    ui.label(engine_msg);
                });

                ui.add_space(8.0);
                ui.add(
                    egui::ProgressBar::new(engine_progress)
                        .text(format!(
                            "{} / {} jobs processed ({:.1}%)",
                            engine_jobs_done,
                            engine_jobs_total,
                            engine_progress * 100.0,
                        ))
                );
            });
        }
    });
}

/// WASM stub — game capture is native-only.
#[cfg(target_arch = "wasm32")]
pub fn render(
    ui: &mut egui::Ui,
    _ctx: &egui::Context,
    _settings: &mut crate::settings::AppSettings,
    _capture_engine_running: &mut bool,
    _engine_msg: &str,
    _engine_progress: f32,
    _engine_jobs_done: usize,
    _engine_jobs_total: usize,
    _hide_non_pov: bool,
    _tx: std::sync::mpsc::Sender<crate::types::GuiMessage>,
    _studio_state: &mut crate::types::CaptureStudioState,
) {
    ui.label("Game Capture is not supported in WASM.");
}
