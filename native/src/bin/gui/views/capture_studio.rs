use egui::Context;
use crate::Gui;
use crate::types::CaptureStudioState;

impl Gui {
    pub fn capture_studio_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        ui.vertical(|ui| {
            ui.add_space(8.0);
            
            ui.horizontal(|ui| {
                ui.heading("🎬 Capture Studio");
            });

            ui.separator();
            ui.add_space(8.0);

            // Sub-views based on CaptureStudioState
            match self.capture_studio_state {
                CaptureStudioState::Workspace
                | CaptureStudioState::Capture => {
                    // Workspace and Capture steps are delegated to render_patch_ui, which
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
                            // Capture step fields:
                            &mut self.settings,
                            &mut self.draft_settings,
                            &mut self.error_message,
                            &mut self.subdir_cache,
                            &mut self.tree_demo_cache,
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
            }
        });
    }
}

