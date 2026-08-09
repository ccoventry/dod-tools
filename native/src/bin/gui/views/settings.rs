use egui::Context;
use crate::Gui;
use crate::views::t;
use crate::settings::{save_settings, apply_language_setting};

impl Gui {
    pub fn render_settings_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        if let Some(error) = self.error_message.clone() {
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
                self.error_message = None;
                ctx.request_repaint();
            }
            return;
        }

        ui.vertical(|ui| {
            ui.heading(t("#app_prefs_general"));
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(t("#app_prefs_language"));
                let mut current_lang = self.draft_settings.language.clone();
                egui::ComboBox::from_id_salt("language_select")
                    .selected_text(match current_lang.as_str() {
                        "auto" => t("#app_prefs_lang_auto"),
                        other => {
                            let mut chars = other.chars();
                            match chars.next() {
                                None => String::new(),
                                Some(f) => {
                                    f.to_uppercase().collect::<String>()
                                        + chars.as_str()
                                }
                            }
                        }
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut current_lang,
                            "auto".to_string(),
                            t("#app_prefs_lang_auto"),
                        );
                        ui.separator();
                        ui.selectable_value(
                            &mut current_lang,
                            "english".to_string(),
                            "English",
                        );
                        ui.selectable_value(
                            &mut current_lang,
                            "french".to_string(),
                            "French",
                        );
                        ui.selectable_value(
                            &mut current_lang,
                            "german".to_string(),
                            "German",
                        );
                        ui.selectable_value(
                            &mut current_lang,
                            "spanish".to_string(),
                            "Spanish",
                        );
                        ui.selectable_value(
                            &mut current_lang,
                            "russian".to_string(),
                            "Russian",
                        );
                        ui.selectable_value(
                            &mut current_lang,
                            "serbian".to_string(),
                            "Serbian",
                        );
                        ui.selectable_value(
                            &mut current_lang,
                            "polish".to_string(),
                            "Polish",
                        );
                        ui.selectable_value(
                            &mut current_lang,
                            "turkish".to_string(),
                            "Turkish",
                        );
                    });

                if current_lang != self.draft_settings.language {
                    self.draft_settings.language = current_lang;
                    ctx.request_repaint();
                }
            });

            ui.add_space(8.0);
            let mut scan_val = self.draft_settings.scan_folders_for_demos;
            if ui.checkbox(&mut scan_val, t("#app_prefs_scan_folders")).changed() {
                self.draft_settings.scan_folders_for_demos = scan_val;
                ctx.request_repaint();
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            ui.heading("File Picker Bookmarks");
            ui.add_space(8.0);

            ui.vertical(|ui| {
                let mut index_to_remove = None;
                for (i, folder) in self.draft_settings.pinned_folders.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.button("🗑").on_hover_text(crate::views::t("tooltip.remove_pin")).clicked() {
                            index_to_remove = Some(i);
                        }
                        ui.label(folder.to_string_lossy());
                    });
                }

                if let Some(i) = index_to_remove {
                    self.draft_settings.pinned_folders.remove(i);
                    self.settings = self.draft_settings.clone();
                    save_settings(&self.settings);
                    ctx.request_repaint();
                }

                #[cfg(not(target_arch = "wasm32"))]
                {
                    if ui.button("➕ Add New Pin").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            self.draft_settings.pinned_folders.push(folder);
                            self.settings = self.draft_settings.clone();
                            save_settings(&self.settings);
                            ctx.request_repaint();
                        }
                    }
                }
            });



            ui.add_space(16.0);
            ui.separator();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("💾 Save Settings").clicked() {
                    let old_scan = self.settings.scan_folders_for_demos;
                    self.settings = self.draft_settings.clone();
                    apply_language_setting(&self.settings.language);
                    save_settings(&self.settings);
                    
                    if old_scan != self.settings.scan_folders_for_demos {
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            self.subdir_cache.clear();
                            self.tree_demo_cache.clear();
                        }
                    }
                    ctx.request_repaint();
                }
                if ui.button("🔄 Revert Settings").clicked() {
                    self.draft_settings = self.settings.clone();
                    ctx.request_repaint();
                }
            });
        });
    }
}
