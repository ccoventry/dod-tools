use crate::FileInfo;
use analysis::{Analysis, ChatType, Team};
use egui::{Color32, RichText, ScrollArea, Ui};

fn format_game_time(d: &std::time::Duration) -> String {
    let total_secs = d.as_secs();
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    let millis = d.subsec_millis();
    let centis = millis / 10;
    format!("{:02}:{:02}:{:02}", mins, secs, centis)
}

pub fn chat_log_ui(file_info: Option<&FileInfo>, r: Option<&Analysis>, ui: &mut Ui) {
    ui.heading("Chat & System Logs");
    ui.add_space(8.0);

    let tab_id = if let Some(fi) = file_info {
        egui::Id::new(&fi.path).with("chat_log")
    } else {
        egui::Id::new("blank_report").with("chat_log")
    };

    let mm1_id = tab_id.with("show_mm1");
    let mm2_id = tab_id.with("show_mm2");
    let system_id = tab_id.with("show_system");
    let filter_text_id = tab_id.with("filter_text");

    let mut show_mm1 = ui.data(|d| d.get_temp::<bool>(mm1_id).unwrap_or(true));
    let mut show_mm2 = ui.data(|d| d.get_temp::<bool>(mm2_id).unwrap_or(true));
    let mut show_system = ui.data(|d| d.get_temp::<bool>(system_id).unwrap_or(true));
    let mut filter_text = ui.data(|d| d.get_temp::<String>(filter_text_id).unwrap_or_default());

    ui.horizontal(|ui| {
        ui.label("Filter types:");
        if ui.checkbox(&mut show_mm1, "All Chat (mm1)").changed() {
            ui.data_mut(|d| d.insert_temp(mm1_id, show_mm1));
        }
        if ui.checkbox(&mut show_mm2, "Team Chat (mm2)").changed() {
            ui.data_mut(|d| d.insert_temp(mm2_id, show_mm2));
        }
        if ui.checkbox(&mut show_system, "System Messages").changed() {
            ui.data_mut(|d| d.insert_temp(system_id, show_system));
        }
        
        ui.add_space(20.0);
        ui.label("Search:");
        if ui.text_edit_singleline(&mut filter_text).changed() {
            ui.data_mut(|d| d.insert_temp(filter_text_id, filter_text.clone()));
        }
    });

    ui.separator();
    ui.add_space(4.0);

    let messages = if let Some(analysis) = r {
        &analysis.state.chat_messages
    } else {
        ui.label("No analysis loaded.");
        return;
    };

    if messages.is_empty() {
        ui.label("No chat or system messages found in this demo.");
        return;
    }

    let query = filter_text.to_lowercase();
    let filtered: Vec<_> = messages
        .iter()
        .filter(|msg| {
            match msg.chat_type {
                ChatType::Mm1 => {
                    if !show_mm1 { return false; }
                }
                ChatType::Mm2 => {
                    if !show_mm2 { return false; }
                }
                ChatType::System => {
                    if !show_system { return false; }
                }
            }

            if !query.is_empty() {
                let name_matches = msg.sender_name.as_ref().map(|n| n.to_lowercase().contains(&query)).unwrap_or(false);
                let text_matches = msg.text.to_lowercase().contains(&query);
                if !name_matches && !text_matches {
                    return false;
                }
            }

            true
        })
        .collect();

    if filtered.is_empty() {
        ui.label("No messages match the active filters.");
        return;
    }

    let text_style = egui::TextStyle::Body;
    let row_height = ui.text_style_height(&text_style);

    ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show_rows(ui, row_height, filtered.len(), |ui, row_range| {
            for idx in row_range {
                let msg = filtered[idx];
                ui.horizontal(|ui| {
                    let time_str = format!("[{} / F: {}]", format_game_time(&msg.time.viewdemo_offset), msg.frame_index);
                    ui.colored_label(Color32::from_rgb(140, 140, 140), time_str);

                    if msg.sender_dead {
                        ui.colored_label(Color32::from_rgb(220, 50, 50), "*DEAD*");
                    }

                    match msg.chat_type {
                        ChatType::Mm1 => {
                            ui.colored_label(Color32::from_rgb(160, 160, 160), "(mm1)");
                        }
                        ChatType::Mm2 => {
                            ui.colored_label(Color32::from_rgb(160, 160, 160), "(mm2)");
                        }
                        ChatType::System => {
                            ui.colored_label(Color32::from_rgb(200, 150, 80), "(system)");
                        }
                    }

                    match msg.chat_type {
                        ChatType::System => {
                            ui.label(RichText::new(&msg.text).italics().color(Color32::from_rgb(180, 220, 220)));
                        }
                        _ => {
                            let team_color = match msg.sender_team {
                                Some(Team::Allies) => Color32::from_rgb(34, 139, 34), // Forest Green
                                Some(Team::Axis) => Color32::from_rgb(178, 34, 34), // Firebrick Red
                                Some(Team::Spectators) => Color32::YELLOW, // Spectator yellow
                                _ => Color32::LIGHT_BLUE, // Default / Console
                            };

                            let name = msg.sender_name.as_deref().unwrap_or("Unknown");
                            ui.colored_label(team_color, name);
                            ui.label(" :  ");
                            ui.colored_label(Color32::WHITE, &msg.text);
                        }
                    }
                });
            }
        });
}
