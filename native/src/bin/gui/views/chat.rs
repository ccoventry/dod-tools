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

    let show_mm1_id = tab_id.with("show_mm1");
    let show_mm2_id = tab_id.with("show_mm2");
    let show_alive_id = tab_id.with("show_alive");
    let show_dead_id = tab_id.with("show_dead");
    let show_joins_id = tab_id.with("show_joins");
    let show_teams_id = tab_id.with("show_teams");
    let show_gameplay_id = tab_id.with("show_gameplay");
    let show_other_sys_id = tab_id.with("show_other_sys");
    let filter_text_id = tab_id.with("filter_text");

    let mut show_mm1 = ui.data(|d| d.get_temp::<bool>(show_mm1_id).unwrap_or(true));
    let mut show_mm2 = ui.data(|d| d.get_temp::<bool>(show_mm2_id).unwrap_or(true));
    let mut show_alive = ui.data(|d| d.get_temp::<bool>(show_alive_id).unwrap_or(true));
    let mut show_dead = ui.data(|d| d.get_temp::<bool>(show_dead_id).unwrap_or(true));
    let mut show_joins = ui.data(|d| d.get_temp::<bool>(show_joins_id).unwrap_or(true));
    let mut show_teams = ui.data(|d| d.get_temp::<bool>(show_teams_id).unwrap_or(true));
    let mut show_gameplay = ui.data(|d| d.get_temp::<bool>(show_gameplay_id).unwrap_or(true));
    let mut show_other_sys = ui.data(|d| d.get_temp::<bool>(show_other_sys_id).unwrap_or(true));
    let mut filter_text = ui.data(|d| d.get_temp::<String>(filter_text_id).unwrap_or_default());

    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Filter Actions:");
        if ui.button("Select All").clicked() {
            show_mm1 = true;
            show_mm2 = true;
            show_alive = true;
            show_dead = true;
            show_joins = true;
            show_teams = true;
            show_gameplay = true;
            show_other_sys = true;
            changed = true;
        }
        if ui.button("Clear All").clicked() {
            show_mm1 = false;
            show_mm2 = false;
            show_alive = false;
            show_dead = false;
            show_joins = false;
            show_teams = false;
            show_gameplay = false;
            show_other_sys = false;
            changed = true;
        }
    });

    ui.horizontal(|ui| {
        ui.label("Player Chat:");
        if ui.checkbox(&mut show_mm1, "All Chat").changed() || changed {
            ui.data_mut(|d| d.insert_temp(show_mm1_id, show_mm1));
        }
        if ui.checkbox(&mut show_mm2, "Team Chat").changed() || changed {
            ui.data_mut(|d| d.insert_temp(show_mm2_id, show_mm2));
        }
        if ui.checkbox(&mut show_alive, "Alive Players").changed() || changed {
            ui.data_mut(|d| d.insert_temp(show_alive_id, show_alive));
        }
        if ui.checkbox(&mut show_dead, "Dead Players").changed() || changed {
            ui.data_mut(|d| d.insert_temp(show_dead_id, show_dead));
        }
    });

    ui.horizontal(|ui| {
        ui.label("System Logs:");
        if ui.checkbox(&mut show_joins, "Joins & Leaves").changed() || changed {
            ui.data_mut(|d| d.insert_temp(show_joins_id, show_joins));
        }
        if ui.checkbox(&mut show_teams, "Team Changes").changed() || changed {
            ui.data_mut(|d| d.insert_temp(show_teams_id, show_teams));
        }
        if ui.checkbox(&mut show_gameplay, "Gameplay & Scoring").changed() || changed {
            ui.data_mut(|d| d.insert_temp(show_gameplay_id, show_gameplay));
        }
        if ui.checkbox(&mut show_other_sys, "Other System").changed() || changed {
            ui.data_mut(|d| d.insert_temp(show_other_sys_id, show_other_sys));
        }
        
        ui.add_space(20.0);
        ui.label("Search:");
        if ui.text_edit_singleline(&mut filter_text).changed() {
            ui.data_mut(|d| d.insert_temp(filter_text_id, filter_text.clone()));
        }
    });

    if changed {
        ui.data_mut(|d| {
            d.insert_temp(show_mm1_id, show_mm1);
            d.insert_temp(show_mm2_id, show_mm2);
            d.insert_temp(show_alive_id, show_alive);
            d.insert_temp(show_dead_id, show_dead);
            d.insert_temp(show_joins_id, show_joins);
            d.insert_temp(show_teams_id, show_teams);
            d.insert_temp(show_gameplay_id, show_gameplay);
            d.insert_temp(show_other_sys_id, show_other_sys);
        });
    }

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
                    let alive_dead_ok = if msg.sender_dead { show_dead } else { show_alive };
                    if !alive_dead_ok { return false; }
                }
                ChatType::Mm2 => {
                    if !show_mm2 { return false; }
                    let alive_dead_ok = if msg.sender_dead { show_dead } else { show_alive };
                    if !alive_dead_ok { return false; }
                }
                ChatType::System => {
                    let sys_category = if let Some(ref token) = msg.system_token {
                        let token_lower = token.to_lowercase();
                        if token_lower.contains("connect") || token_lower.contains("join_game") || token_lower.contains("joined_game") || token_lower.contains("kick") || token_lower.contains("disconnect") {
                            "join_leave"
                        } else if token_lower.contains("joined_team") || token_lower.contains("team") {
                            "team_change"
                        } else if token_lower.contains("score") || token_lower.contains("capture") || token_lower.contains("cap") || token_lower.contains("reinforce") {
                            "gameplay"
                        } else {
                            "other"
                        }
                    } else {
                        "other"
                    };

                    match sys_category {
                        "join_leave" => { if !show_joins { return false; } }
                        "team_change" => { if !show_teams { return false; } }
                        "gameplay" => { if !show_gameplay { return false; } }
                        _ => { if !show_other_sys { return false; } }
                    }
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
                        ChatType::Mm2 => {
                            let team_color = match msg.sender_team {
                                Some(Team::Allies) => Color32::from_rgb(34, 139, 34), // Forest Green
                                Some(Team::Axis) => Color32::from_rgb(178, 34, 34), // Firebrick Red
                                Some(Team::Spectators) => Color32::YELLOW, // Spectator yellow
                                _ => Color32::LIGHT_BLUE, // Default / Console
                            };
                            ui.colored_label(team_color, "(Team)");
                        }
                        ChatType::System => {
                            ui.colored_label(Color32::from_rgb(200, 150, 80), "(system)");
                        }
                        _ => {}
                    }

                    match msg.chat_type {
                        ChatType::System => {
                            let display_text = if let Some(ref token) = msg.system_token {
                                analysis::translate_system_message(
                                    token,
                                    msg.system_args.get(0).and_then(|o| o.as_deref()),
                                    msg.system_args.get(1).and_then(|o| o.as_deref()),
                                    msg.system_args.get(2).and_then(|o| o.as_deref()),
                                    msg.system_args.get(3).and_then(|o| o.as_deref()),
                                )
                            } else {
                                msg.text.clone()
                            };
                            ui.label(RichText::new(&display_text).italics().color(Color32::from_rgb(180, 220, 220)));
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
