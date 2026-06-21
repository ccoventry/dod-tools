use egui::Context;
use crate::Gui;
use crate::types::CaptureStudioState;

impl Gui {
    pub fn capture_studio_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        ui.vertical(|ui| {
            ui.add_space(8.0);
            
            // Stepper UI
            ui.horizontal(|ui| {
                ui.heading("🎬 Capture Studio");
                ui.separator();

                let phase = self.capture_studio_state;
                let is_wasm = cfg!(target_arch = "wasm32");

                let step1_active = phase == CaptureStudioState::Scan;
                let step1_btn = ui.selectable_label(step1_active, "1. Scan");
                if step1_btn.clicked() {
                    self.capture_studio_state = CaptureStudioState::Scan;
                }

                if !is_wasm {
                    ui.label(" ➔ ");
                    let step2_active = phase == CaptureStudioState::Select;
                    let step2_btn = ui.selectable_label(step2_active, "2. Select");
                    if step2_btn.clicked() {
                        self.capture_studio_state = CaptureStudioState::Select;
                    }

                    ui.label(" ➔ ");
                    let step3_active = phase == CaptureStudioState::Capture;
                    let step3_btn = ui.selectable_label(step3_active, "3. Capture");
                    if step3_btn.clicked() {
                        self.capture_studio_state = CaptureStudioState::Capture;
                    }

                    ui.label(" ➔ ");
                    let step4_active = phase == CaptureStudioState::Render;
                    let step4_btn = ui.selectable_label(step4_active, "4. Render");
                    if step4_btn.clicked() {
                        self.capture_studio_state = CaptureStudioState::Render;
                    }

                    ui.label(" ➔ ");
                    let step5_active = phase == CaptureStudioState::Finish;
                    let step5_btn = ui.selectable_label(step5_active, "5. Finish");
                    if step5_btn.clicked() {
                        self.capture_studio_state = CaptureStudioState::Finish;
                    }
                }
            });

            ui.separator();
            ui.add_space(8.0);

            // Sub-views based on CaptureStudioState
            match self.capture_studio_state {
                CaptureStudioState::Scan | CaptureStudioState::Select => {
                    // Both rendering are delegated to render_patch_ui where we check the state internally
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        crate::views::capture_ui::render_patch_ui(ui, ctx, &mut self.export_queue, self.capture_studio_state, &mut self.capture_studio_state);
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        ui.label("Not supported in WASM");
                    }
                }
                CaptureStudioState::Capture => {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        ui.vertical(|ui| {
                            // CRITICAL: Engine relies on HLAE injection for mirv_streams. Do not rename to 'Native'.
                            ui.heading("🎬 HLAE Game Capture Engine");
                            ui.add_space(8.0);

                            // Paths configuration
                            ui.group(|ui| {
                                ui.strong("Paths Configuration");
                                ui.add_space(4.0);

                                ui.horizontal(|ui| {
                                    ui.label("hl.exe Path:");
                                    if ui.text_edit_singleline(&mut self.settings.game_path).changed() {
                                        self.draft_settings.game_path = self.settings.game_path.clone();
                                        crate::settings::save_settings(&self.settings);
                                    }
                                    if ui.button("Browse...").clicked() {
                                        if let Some(path) = rfd::FileDialog::new()
                                            .add_filter("hl.exe", &["exe"])
                                            .pick_file()
                                        {
                                            self.settings.game_path = path.to_string_lossy().to_string();
                                            self.draft_settings.game_path = self.settings.game_path.clone();
                                            crate::settings::save_settings(&self.settings);
                                        }
                                    }
                                });

                                ui.horizontal(|ui| {
                                    ui.label("hlae.exe Path:");
                                    if ui.text_edit_singleline(&mut self.settings.hlae_path).changed() {
                                        self.draft_settings.hlae_path = self.settings.hlae_path.clone();
                                        crate::settings::save_settings(&self.settings);
                                    }
                                    if ui.button("Browse...").clicked() {
                                        if let Some(path) = rfd::FileDialog::new()
                                            .add_filter("hlae.exe", &["exe"])
                                            .pick_file()
                                        {
                                            self.settings.hlae_path = path.to_string_lossy().to_string();
                                            self.draft_settings.hlae_path = self.settings.hlae_path.clone();
                                            crate::settings::save_settings(&self.settings);
                                        }
                                    }
                                });
                            });

                            ui.add_space(12.0);

                            // Launch controls
                            let hl_exists = !self.settings.game_path.is_empty() && std::path::Path::new(&self.settings.game_path).exists();
                            let hlae_exists = !self.settings.hlae_path.is_empty() && std::path::Path::new(&self.settings.hlae_path).exists();
                            let can_launch = hl_exists && hlae_exists && !self.capture_engine_running;

                            ui.horizontal(|ui| {
                                if ui.add_enabled(can_launch, egui::Button::new("🎬 Launch Capture Engine")).clicked() {
                                    let hlae_path = std::sync::Arc::new(std::path::PathBuf::from(&self.settings.hlae_path));
                                    let hl_path = std::sync::Arc::new(std::path::PathBuf::from(&self.settings.game_path));
                                    let dod_dir = hl_path.parent().unwrap().join("dod");

                                    let mut raw_streaks = Vec::new();
                                    {
                                        let queued_demos_arc = crate::views::capture_ui::get_queued_demos();
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
                                                    raw_streaks.push(native::patch::CaptureStreak {
                                                        start_tick: streak.start_tick,
                                                        end_tick: streak.end_tick,
                                                        source_demo: demo_path_str.clone(),
                                                        target_player: Some(streak.target_player.clone()),
                                                        kill_count: streak.kill_count,
                                                    });
                                                }
                                            }
                                        }
                                    }

                                    let patcher_config_mutex = crate::views::capture_ui::get_patcher_config();
                                    let patcher_config = match patcher_config_mutex.lock() {
                                        Ok(guard) => guard,
                                        Err(poisoned) => {
                                            log::error!("Mutex poisoned, attempting recovery...");
                                            poisoned.into_inner()
                                        }
                                    };
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
                                        let (engine_tx, engine_rx) = std::sync::mpsc::channel();
                                        let gui_tx = self.tx.clone();
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

                                if ui.button("Proceed to Render ->").clicked() {
                                    self.capture_studio_state = CaptureStudioState::Render;
                                }
                            });

                            ui.add_space(12.0);

                            // Progress Display
                            if self.capture_engine_running || !self.capture_engine_msg.is_empty() {
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        if self.capture_engine_running {
                                            ui.spinner();
                                        }
                                        ui.label(&self.capture_engine_msg);
                                    });

                                    ui.add_space(8.0);
                                    ui.add(
                                        egui::ProgressBar::new(self.capture_engine_progress)
                                            .text(format!(
                                                "{} / {} jobs processed ({:.1}%)",
                                                self.capture_engine_jobs_done,
                                                self.capture_engine_jobs_total,
                                                self.capture_engine_progress * 100.0
                                            ))
                                    );
                                });
                            }
                        });
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        ui.label("Game Capture is not supported in WASM.");
                    }
                }
                CaptureStudioState::Render => {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        self.hlcr_state.draw_ui(ui, ctx);
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        ui.label("HLCR rendering is not supported in the WASM target.");
                    }
                }
                CaptureStudioState::Finish => {
                    ui.vertical_centered(|ui| {
                        ui.heading("Capture Studio Complete");
                        ui.add_space(10.0);
                        ui.label("All recording and rendering processes have finished.");
                        if ui.button("Return to Scan").clicked() {
                            self.capture_studio_state = CaptureStudioState::Scan;
                        }
                    });
                }
            }
        });
    }
}
