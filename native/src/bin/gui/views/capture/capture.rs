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
/// - `tx`                    — GuiMessage sender for the relay thread
/// - `studio_state`          — written to CaptureStudioState::Render on Proceed
#[cfg(not(target_arch = "wasm32"))]
pub fn render(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    capture_engine_running: &mut bool,
    engine_msg: &str,
    _engine_progress: f32,
    engine_jobs_done: usize,
    engine_jobs_total: usize,
    tx: std::sync::mpsc::Sender<crate::types::GuiMessage>,
    studio_state: &mut crate::types::CaptureStudioState,
    cancel_token: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    ui.vertical(|ui| {
        // CRITICAL: Engine relies on HLAE injection for mirv_streams. Do not rename to 'Native'.
        ui.heading("🎬 HLAE Game Capture Engine");
        ui.add_space(8.0);

        // ── Paths configuration ──────────────────────────────────────────────
        ui.group(|ui| {
            ui.strong("Paths Configuration");
            let mut config = crate::views::capture::get_patcher_config().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            
            ui.add_space(4.0);

            let mut dummy_error = None;
            let old_game = config.game_path.clone();
            crate::views::capture::widgets::render_path_row(
                ui,
                "hl.exe Path:",
                &mut config.game_path,
                "hl.exe",
                &["exe"],
                None,
                &mut dummy_error,
            );
            if old_game != config.game_path {
                crate::settings::save_patcher_config(&config);
            }

            let old_hlae = config.hlae_path.clone();
            crate::views::capture::widgets::render_path_row(
                ui,
                "hlae.exe Path:",
                &mut config.hlae_path,
                "hlae.exe",
                &["exe"],
                None,
                &mut dummy_error,
            );
            if old_hlae != config.hlae_path {
                crate::settings::save_patcher_config(&config);
            }
        });

        let (hlae_path_str, game_path_str) = {
            let config = crate::views::capture::get_patcher_config().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            (config.hlae_path.clone(), config.game_path.clone())
        };

        ui.add_space(12.0);

        // ── Launch controls ──────────────────────────────────────────────────
        let hl_exists = !game_path_str.is_empty()
            && std::path::Path::new(&game_path_str).exists();
        let hlae_exists = !hlae_path_str.is_empty()
            && std::path::Path::new(&hlae_path_str).exists();
        let can_launch = hl_exists && hlae_exists && !*capture_engine_running;

        ui.horizontal(|ui| {
            if ui.add_enabled(can_launch, egui::Button::new("🎬 Launch Capture Engine")).clicked() {
                let hlae_path = std::sync::Arc::new(std::path::PathBuf::from(&hlae_path_str));
                let hl_path = std::sync::Arc::new(std::path::PathBuf::from(&game_path_str));
                let dod_dir = hl_path.parent().unwrap().join("dod");

                // Build raw streak list and global frame-time map from the shared queued demos mutex.
                let (raw_streaks, global_arrays) = {
                    let queued_demos_arc = crate::views::capture::get_queued_demos();
                    let queued_demos = match queued_demos_arc.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            log::error!("Mutex poisoned, attempting recovery...");
                            poisoned.into_inner()
                        }
                    };
                    let streaks = crate::views::capture::payload::build_capture_streak_payload(
                        &queued_demos,
                        crate::views::capture::payload::StreakFilter {
                            selected_only: true,
                            pov_local_only: true,
                        },
                    );
                    let mut arrays = std::collections::HashMap::new();
                    for demo in queued_demos.iter() {
                        arrays.insert(demo.demo_name.clone(), demo.frame_times.clone());
                    }
                    (streaks, arrays)
                };

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

                let patch_jobs = match native::patch::build_batch_queue(raw_streaks, &patcher_config, &global_arrays) {
                    Ok(jobs) => jobs,
                    Err(e) => {
                        log::error!("Failed to write helper config: {}", e);
                        let _ = tx.send(crate::types::GuiMessage::CaptureEngineEvent(crate::types::EngineEvent::Error(format!("Failed to write helper config: {}", e))));
                        return;
                    }
                };

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
                    // Reset the token from any prior run before spawning,
                    // so a previous cancellation doesn't abort this new batch.
                    cancel_token.store(false, std::sync::atomic::Ordering::Relaxed);

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
                        cancel_token.clone(),
                        patcher_config,
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

            // Cancel Capture — only enabled while the engine is running.
            if ui.add_enabled(
                *capture_engine_running,
                egui::Button::new("⛔ Cancel Capture"),
            ).clicked() {
                cancel_token.store(true, std::sync::atomic::Ordering::Relaxed);
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
                let mut pct = if engine_jobs_total > 0 {
                    (engine_jobs_done as f32 / engine_jobs_total as f32) * 100.0
                } else {
                    0.0
                };
                if engine_jobs_done < engine_jobs_total && pct >= 100.0 {
                    pct = 99.9;
                }
                let bar_val = if engine_jobs_total > 0 {
                    (engine_jobs_done as f32 / engine_jobs_total as f32).clamp(0.0, 1.0)
                } else {
                    0.0
                };

                ui.add(
                    egui::ProgressBar::new(bar_val)
                        .text(format!(
                            "{} / {} jobs processed ({:.1}%)",
                            engine_jobs_done,
                            engine_jobs_total,
                            pct,
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
    _tx: std::sync::mpsc::Sender<crate::types::GuiMessage>,
    _studio_state: &mut crate::types::CaptureStudioState,
    _cancel_token: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    ui.label("Game Capture is not supported in WASM.");
}
