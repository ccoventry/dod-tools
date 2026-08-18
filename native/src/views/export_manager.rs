use egui::Context;
use crate::Gui;

pub fn show(app: &mut Gui, ctx: &Context, ui: &mut egui::Ui) {
    egui::TopBottomPanel::bottom("export_manager_footer").show_inside(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Queue Status: Idle | Active Renders: 0/2");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cancel All Renders").clicked() {
                    // TODO: Wire cancel dispatch
                }
            });
        });
    });

    ui.vertical(|ui| {
        ui.heading("🎥 Export Manager");
        ui.separator();
        ui.add_space(8.0);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let config_guard = match crate::views::capture::get_patcher_config().lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if let Ok(resolved_path) = crate::settings::resolve_ffmpeg_path(config_guard.ffmpeg_override_path.as_ref()) {
                app.hlcr_state.config.ffmpeg_path = resolved_path.to_string_lossy().to_string();
            }
            drop(config_guard);
            app.hlcr_state.draw_ui(ui, ctx);
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx;
            ui.label("HLCR rendering is not supported in the WASM target.");
        }
    });
}
