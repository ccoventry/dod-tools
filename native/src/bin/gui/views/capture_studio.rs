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
                let _is_wasm = cfg!(target_arch = "wasm32");

                let step1_active = phase == CaptureStudioState::Scan;
                let step1_btn = ui.selectable_label(step1_active, "1. Scan");
                if step1_btn.clicked() {
                    self.capture_studio_state = CaptureStudioState::Scan;
                }

                #[cfg(not(target_arch = "wasm32"))]
                {
                    ui.label(" ➔ ");
                    let step2_active = phase == CaptureStudioState::Select;
                    let step2_btn = ui.selectable_label(step2_active, "2. Select");
                    if step2_btn.clicked() {
                        let queued_arc = crate::views::capture::get_queued_demos();
                        let queued_guard = match queued_arc.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => poisoned.into_inner(),
                        };
                        let msg = format!("Transitioning with {} items", queued_guard.len());
                        log::info!("{}", msg);
                        crate::views::capture::log_markdown(&msg);
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
                CaptureStudioState::Scan
                | CaptureStudioState::Select
                | CaptureStudioState::Capture => {
                    // All three steps are delegated to render_patch_ui, which
                    // dispatches to the appropriate sub-module based on current_state.
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        crate::views::capture::render_patch_ui(
                            ui,
                            ctx,
                            &mut self.export_queue,
                            self.capture_studio_state,
                            &mut self.capture_studio_state,
                            self.tx.clone(),
                            &mut self.capture_studio_loading,
                            &mut self.hide_non_pov,
                            // Capture step fields:
                            &mut self.settings,
                            &mut self.capture_engine_running,
                            &self.capture_engine_msg,
                            self.capture_engine_progress,
                            self.capture_engine_jobs_done,
                            self.capture_engine_jobs_total,
                            self.capture_cancel_token.clone(),
                        );
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        ui.label("Not supported in WASM");
                    }
                }
                CaptureStudioState::Render => {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        if let Ok(resolved_path) = crate::settings::resolve_ffmpeg_path(&self.settings) {
                            self.hlcr_state.config.ffmpeg_path = resolved_path.to_string_lossy().to_string();
                        }
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
