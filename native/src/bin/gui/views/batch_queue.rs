use egui::{Ui, Color32, Layout, Align, Grid, ScrollArea, Frame, Stroke};
use crate::QueuedStreakExport;
use std::collections::HashMap;
use native::FileInfo;
use analysis::Analysis;

pub fn batch_queue_ui(
    export_queue: &mut Vec<QueuedStreakExport>,
    settings: &mut crate::AppSettings,
    cache: &mut crate::PlayerDetailsCache,
    analyses: &HashMap<String, (FileInfo, Analysis)>,
    ui: &mut Ui,
) {
    ui.vertical(|ui| {


        // Path existence checks
        let hlae_exists = !settings.hlae_path.is_empty() && std::path::Path::new(&settings.hlae_path).exists();
        let game_exists = !settings.game_path.is_empty() && std::path::Path::new(&settings.game_path).exists();

        if !settings.hlae_path.is_empty() && !hlae_exists {
            ui.colored_label(Color32::from_rgb(239, 68, 68), "⚠ HLAE path does not exist on disk.");
        }
        if !settings.game_path.is_empty() && !game_exists {
            ui.colored_label(Color32::from_rgb(239, 68, 68), "⚠ Game path (hl.exe) does not exist on disk.");
        }

        ui.add_space(8.0);

        // 2. Queue actions header
        ui.horizontal(|ui| {
            ui.heading(format!("Batch Queue ({})", export_queue.len()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("🗑 Clear Queue").on_hover_text("Remove all items from the queue").clicked() {
                    export_queue.clear();
                }

                let enabled_count = export_queue.iter().filter(|item| item.enabled).count();
                let has_errors = export_queue.iter().any(|item| {
                    if !item.enabled {
                        return false;
                    }
                    item.output_name.trim().is_empty() 
                        || item.output_name.chars().any(|c| r#"\/:*?"<>|"#.contains(c))
                        || export_queue.iter().filter(|o| o.enabled && o.id != item.id).any(|o| o.output_name.trim().eq_ignore_ascii_case(item.output_name.trim()))
                });

                #[cfg(not(target_arch = "wasm32"))]
                {
                    let btn = egui::Button::new("📤 Export Enabled Streaks")
                        .fill(Color32::from_rgb(37, 99, 235))
                        .stroke(Stroke::NONE);

                    let can_export = enabled_count > 0 && hlae_exists && game_exists && !has_errors;
                    if ui.add_enabled(can_export, btn).on_hover_text("Select output folder to patch and export all enabled highlights").clicked() {
                        cache.batch_export_request = true;
                    }
                }
                #[cfg(target_arch = "wasm32")]
                {
                    ui.add_enabled(false, egui::Button::new("📤 Export Enabled (Desktop only)"));
                }
            });
        });

        ui.separator();
        ui.add_space(4.0);

        if export_queue.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.weak("The batch queue is currently empty.");
                ui.weak("Add player highlights from the 'Player Details' tab using the ➕ button next to any streak.");
            });
            return;
        }

        // 3. Queue list
        let mut swap_up = None;
        let mut swap_down = None;
        let mut delete_idx = None;

        let queue_len = export_queue.len();
        let mut duplicate_names = std::collections::HashSet::new();
        let mut seen_names = std::collections::HashSet::new();
        for item in export_queue.iter() {
            if item.enabled {
                let trimmed = item.output_name.trim().to_lowercase();
                if !seen_names.insert(trimmed.clone()) {
                    duplicate_names.insert(trimmed);
                }
            }
        }

        ScrollArea::vertical()
            .id_salt("batch_queue_scroll")
            .show(ui, |ui| {
                for (idx, item) in export_queue.iter_mut().enumerate() {
                    let item_frame = Frame::NONE
                        .fill(if item.enabled { ui.visuals().window_fill() } else { ui.visuals().faint_bg_color })
                        .stroke(Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color))
                        .corner_radius(6.0)
                        .inner_margin(8.0);

                    item_frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Checkbox to enable/disable
                            ui.checkbox(&mut item.enabled, "");

                            // Header details
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.strong(&item.player_name);
                                    ui.weak(format!("(Streak {}, {} Kills)", item.streak_idx, item.kills_count));
                                    ui.weak(format!("Demo: {}", item.input_path.file_name().and_then(|n| n.to_str()).unwrap_or(""))).on_hover_text(item.input_path.to_string_lossy());
                                });

                                // Filename input and validation
                                ui.horizontal(|ui| {
                                    ui.label("Output Demo Name:");
                                    let mut out_name = item.output_name.clone();
                                    let resp = ui.add(
                                        egui::TextEdit::singleline(&mut out_name)
                                            .desired_width(280.0)
                                    );
                                    if resp.changed() {
                                        item.output_name = out_name;
                                    }

                                    // Validations
                                    let name_is_empty = item.output_name.trim().is_empty();
                                    let has_invalid_chars = item.output_name.chars().any(|c| r#"\/:*?"<>|"#.contains(c));
                                    
                                    let matches_original = if let Some(orig_name) = item.input_path.file_name().and_then(|n| n.to_str()) {
                                        item.output_name.trim().eq_ignore_ascii_case(orig_name)
                                    } else {
                                        false
                                    };

                                    let is_duplicate = if item.enabled {
                                         duplicate_names.contains(&item.output_name.trim().to_lowercase())
                                     } else {
                                         false
                                     };

                                    let mut file_exists_in_game = false;
                                    if game_exists {
                                        if let Some(game_dir) = std::path::Path::new(&settings.game_path).parent() {
                                            let dod_path = game_dir.join("dod").join(&item.output_name);
                                            if dod_path.exists() {
                                                file_exists_in_game = true;
                                            }
                                        }
                                    }

                                    // Display real-time validation warnings
                                    if name_is_empty {
                                        ui.colored_label(Color32::from_rgb(239, 68, 68), "⚠ Cannot be empty");
                                    } else if has_invalid_chars {
                                        ui.colored_label(Color32::from_rgb(239, 68, 68), "⚠ Invalid characters (\\/:*?\"<>|)");
                                    } else if matches_original {
                                        ui.colored_label(Color32::from_rgb(217, 119, 6), "⚠ Same as original file");
                                    } else if is_duplicate {
                                        ui.colored_label(Color32::from_rgb(239, 68, 68), "⚠ Duplicate name in queue");
                                    } else if file_exists_in_game {
                                        ui.colored_label(Color32::from_rgb(217, 119, 6), "⚠ Already exists in dod/");
                                    }
                                });
                            });

                            // Action buttons: Move Up, Move Down, Delete
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button("❌").on_hover_text("Delete from queue").clicked() {
                                    delete_idx = Some(idx);
                                }

                                if ui.add_enabled(idx < queue_len - 1, egui::Button::new("⏷")).on_hover_text("Move Down").clicked() {
                                    swap_down = Some(idx);
                                }

                                if ui.add_enabled(idx > 0, egui::Button::new("⏶")).on_hover_text("Move Up").clicked() {
                                    swap_up = Some(idx);
                                }
                            });
                        });

                        ui.add_space(4.0);

                        // Settings overrides drawer
                        let collapsing_id = ui.make_persistent_id(&item.id);
                        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), collapsing_id, false)
                            .show_header(ui, |ui| {
                                ui.label("🔧 Custom Settings Overrides");
                            })
                            .body(|ui| {
                                ui.add_space(4.0);
                                Grid::new(format!("grid_{}", item.id))
                                    .num_columns(2)
                                    .spacing([8.0, 6.0])
                                    .striped(true)
                                    .show(ui, |ui| {
                                        ui.label("Exit Game On Finish:");
                                        ui.checkbox(&mut item.exit_on_finish, "");
                                        ui.end_row();

                                        ui.label("Fast-Forward Speed:");
                                        ui.add(egui::Slider::new(&mut item.fast_forward_speed, 0.01..=5.0).step_by(0.05));
                                        ui.end_row();

                                        ui.label("Initial Load Delay (s):");
                                        ui.add(egui::Slider::new(&mut item.initial_delay, 0.0..=30.0).step_by(0.5));
                                        ui.end_row();

                                        ui.label("Pre-Record Buffer (s):");
                                        ui.add(egui::Slider::new(&mut item.pre_record_buffer, 0.0..=30.0).step_by(0.5));
                                        ui.end_row();

                                        ui.label("Record Start Lead (s):");
                                        ui.add(egui::Slider::new(&mut item.record_start_lead, 0.0..=10.0).step_by(0.5));
                                        ui.end_row();

                                        ui.label("Record Stop Trail (s):");
                                        ui.add(egui::Slider::new(&mut item.record_stop_trail, 0.0..=10.0).step_by(0.5));
                                        ui.end_row();

                                        ui.label("Post-Record Buffer (s):");
                                        ui.add(egui::Slider::new(&mut item.post_record_buffer, 0.0..=30.0).step_by(0.5));
                                        ui.end_row();

                                        ui.label("HLTV Spec Player:");
                                        let path_str = item.input_path.to_string_lossy().to_string();
                                        if let Some((_, analysis)) = analyses.get(&path_str) {
                                            if analysis.demo_info.demo_type == "HLTV" {
                                                let mut player_names: Vec<String> = analysis.state.players.iter().map(|p| p.name.clone()).collect();
                                                player_names.sort();
                                                if player_names.is_empty() {
                                                    ui.label("No players found");
                                                } else {
                                                    let mut current_selected = item.hltv_spec_player.clone().unwrap_or_else(|| item.player_name.clone());
                                                    if !player_names.contains(&current_selected) {
                                                        if player_names.contains(&item.player_name) {
                                                            current_selected = item.player_name.clone();
                                                        } else {
                                                            current_selected = player_names[0].clone();
                                                        }
                                                    }
                                                    let mut selected_name = current_selected.clone();
                                                    egui::ComboBox::from_id_salt(format!("spec_combo_{}", item.id))
                                                        .selected_text(&selected_name)
                                                        .show_ui(ui, |ui| {
                                                            for name in &player_names {
                                                                ui.selectable_value(&mut selected_name, name.clone(), name);
                                                            }
                                                        });
                                                    if selected_name != current_selected || item.hltv_spec_player.is_none() {
                                                        item.hltv_spec_player = Some(selected_name);
                                                    }
                                                }
                                            } else {
                                                ui.label("POV: Auto-detect");
                                            }
                                        } else {
                                            ui.weak("Demo analysis not loaded");
                                        }
                                        ui.end_row();

                                        ui.label("Init Commands:");
                                        ui.text_edit_multiline(&mut item.init_commands);
                                        ui.end_row();

                                        ui.label("Custom Timed Commands:");
                                        ui.vertical(|ui| {
                                            let mut delete_cmd_idx = None;

                                            for (c_idx, cmd) in item.custom_commands.iter_mut().enumerate() {
                                                ui.horizontal(|ui| {
                                                    ui.add(egui::TextEdit::singleline(&mut cmd.command).desired_width(180.0));
                                                    
                                                    let is_after = cmd.relation == native::patch::CommandRelation::After;
                                                    if ui.selectable_label(!is_after, "Before").clicked() {
                                                        cmd.relation = native::patch::CommandRelation::Before;
                                                    }
                                                    if ui.selectable_label(is_after, "After").clicked() {
                                                        cmd.relation = native::patch::CommandRelation::After;
                                                    }

                                                    ui.add(egui::DragValue::new(&mut cmd.offset).speed(0.1).range(0.0..=60.0).suffix("s"));
                                                    
                                                    if ui.button("❌").clicked() {
                                                        delete_cmd_idx = Some(c_idx);
                                                    }
                                                });
                                            }

                                            if let Some(c_idx) = delete_cmd_idx {
                                                item.custom_commands.remove(c_idx);
                                            }

                                            if ui.button("➕ Add Command").clicked() {
                                                item.custom_commands.push(native::patch::CustomCommand {
                                                    command: "".to_string(),
                                                    offset: 2.0,
                                                    relation: native::patch::CommandRelation::Before,
                                                });
                                            }
                                        });
                                        ui.end_row();
                                    });

                                ui.add_space(4.0);
                                if ui.button("Reset to Defaults").on_hover_text("Reset all fields of this highlight item to match global preferences").clicked() {
                                    item.exit_on_finish = true;
                                    item.init_commands = settings.capture_init_commands.clone();
                                    item.custom_commands = settings.custom_commands.clone();
                                    item.fast_forward_speed = settings.capture_fast_forward_speed;
                                    item.initial_delay = settings.capture_initial_delay;
                                    item.pre_record_buffer = settings.capture_pre_record_buffer;
                                    item.record_start_lead = settings.capture_record_start_lead;
                                    item.record_stop_trail = settings.capture_record_stop_trail;
                                    item.post_record_buffer = settings.post_record_buffer;
                                }
                            });
                    });

                    ui.add_space(6.0);
                }
            });

        // 4. Perform modifications
        if let Some(idx) = swap_up {
            if idx > 0 {
                export_queue.swap(idx, idx - 1);
            }
        }
        if let Some(idx) = swap_down {
            if idx < export_queue.len() - 1 {
                export_queue.swap(idx, idx + 1);
            }
        }
        if let Some(idx) = delete_idx {
            export_queue.remove(idx);
        }
    });
}
