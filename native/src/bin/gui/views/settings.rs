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
            ui.heading("Recording Engine Configurations");
            ui.add_space(8.0);

            // HLAE Path configuration
            ui.label("HLAE Path (hlae.exe):");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.draft_settings.hlae_path).desired_width(ui.available_width() - 80.0));
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Executables", &["exe"])
                            .pick_file()
                        {
                            if path.file_name().and_then(|n| n.to_str()).map(|s| s.to_lowercase()) == Some("hlae.exe".to_string()) {
                                self.draft_settings.hlae_path = path.to_string_lossy().to_string();
                            } else {
                                self.error_message = Some("Selected file must be hlae.exe".to_string());
                            }
                        }
                    }
                }
            });

            ui.add_space(8.0);

            // DoD Game Path configuration
            ui.label("DoD Game Path (hl.exe):");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.draft_settings.game_path).desired_width(ui.available_width() - 80.0));
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Executables", &["exe"])
                            .pick_file()
                        {
                            if path.file_name().and_then(|n| n.to_str()).map(|s| s.to_lowercase()) == Some("hl.exe".to_string()) {
                                self.draft_settings.game_path = path.to_string_lossy().to_string();
                            } else {
                                self.error_message = Some("Selected file must be hl.exe".to_string());
                            }
                        }
                    }
                }
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            ui.heading("Highlight Capture Settings");
            ui.add_space(8.0);

            ui.label("Init Commands (startup):");
            let mut val = self.draft_settings.capture_init_commands.clone();
            if ui.text_edit_multiline(&mut val).changed() {
                self.draft_settings.capture_init_commands = val;
            }
            
            ui.add_space(8.0);
            ui.label("Default Custom Commands:");
            ui.add_space(4.0);
            ui.vertical(|ui| {
                let mut delete_idx = None;
                
                egui::ScrollArea::vertical()
                    .max_height(120.0)
                    .id_salt("default_commands_scroll")
                    .show(ui, |ui| {
                        for (i, cmd) in self.draft_settings.custom_commands.iter_mut().enumerate() {
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
                    self.draft_settings.custom_commands.remove(i);
                }
                if ui.button("➕ Add Default").clicked() {
                    self.draft_settings.custom_commands.push(native::patch::CustomCommand {
                        command: "".to_string(),
                        offset: 2.0,
                        relation: native::patch::CommandRelation::Before,
                    });
                }
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            ui.strong("Timeline Buffers");
            ui.add_space(4.0);

            ui.label("Initial Load Delay:");
            let mut val = self.draft_settings.capture_initial_delay;
            if ui.add(egui::Slider::new(&mut val, 0.0..=30.0).step_by(0.5).suffix("s")).changed() {
                self.draft_settings.capture_initial_delay = val;
            }

            ui.label("Fast-Forward Speed:");
            let mut val = self.draft_settings.capture_fast_forward_speed;
            if ui.add(egui::Slider::new(&mut val, 0.01..=5.0).step_by(0.05)).changed() {
                self.draft_settings.capture_fast_forward_speed = val;
            }

            ui.label("Pre-Record Buffer:");
            let mut val = self.draft_settings.capture_pre_record_buffer;
            if ui.add(egui::Slider::new(&mut val, 0.0..=30.0).step_by(0.5).suffix("s")).changed() {
                self.draft_settings.capture_pre_record_buffer = val;
            }

            ui.label("Record Start Lead:");
            let mut val = self.draft_settings.capture_record_start_lead;
            if ui.add(egui::Slider::new(&mut val, 0.0..=10.0).step_by(0.5).suffix("s")).changed() {
                self.draft_settings.capture_record_start_lead = val;
            }

            ui.label("Record Stop Trail:");
            let mut val = self.draft_settings.capture_record_stop_trail;
            if ui.add(egui::Slider::new(&mut val, 0.0..=10.0).step_by(0.5).suffix("s")).changed() {
                self.draft_settings.capture_record_stop_trail = val;
            }

            ui.label("Post-Record Buffer:");
            let mut val = self.draft_settings.post_record_buffer;
            if ui.add(egui::Slider::new(&mut val, 0.0..=30.0).step_by(0.5).suffix("s")).changed() {
                self.draft_settings.post_record_buffer = val;
            }

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
