use crate::FileInfo;
use crate::views::t;
use analysis::{Analysis, ChatType, Team};
use egui::{Color32, RichText, ScrollArea, Ui};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerStatusFilter {
    All,
    Alive,
    Dead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerTeamFilter {
    All,
    Allies,
    British,
    Axis,
    Spectators,
}

fn format_game_time(d: &std::time::Duration) -> String {
    let total_secs = d.as_secs();
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    let millis = d.subsec_millis();
    let centis = millis / 10;
    format!("{:02}:{:02}:{:02}", mins, secs, centis)
}

pub fn chat_log_ui(
    file_info: Option<&FileInfo>,
    r: Option<&Analysis>,
    chat_cache: &mut crate::ChatCache,
    ui: &mut Ui,
) {
    ui.heading(t("#app_chat_heading"));
    ui.add_space(8.0);

    let tab_id = if let Some(fi) = file_info {
        egui::Id::new(&fi.path).with("chat_log")
    } else {
        egui::Id::new("blank_report").with("chat_log")
    };

    let show_mm1_id = tab_id.with("show_mm1");
    let show_mm2_id = tab_id.with("show_mm2");
    let show_status_id = tab_id.with("show_status");
    let show_team_filter_id = tab_id.with("show_team_filter");
    let show_joins_id = tab_id.with("show_joins");
    let show_teams_id = tab_id.with("show_teams");
    let show_gameplay_id = tab_id.with("show_gameplay");
    let show_other_sys_id = tab_id.with("show_other_sys");
    let filter_text_id = tab_id.with("filter_text");

    let mut show_mm1 = ui.data(|d| d.get_temp::<bool>(show_mm1_id).unwrap_or(true));
    let mut show_mm2 = ui.data(|d| d.get_temp::<bool>(show_mm2_id).unwrap_or(true));
    let mut show_status = ui.data(|d| {
        d.get_temp::<PlayerStatusFilter>(show_status_id)
            .unwrap_or(PlayerStatusFilter::All)
    });
    let mut show_team_filter = ui.data(|d| {
        d.get_temp::<PlayerTeamFilter>(show_team_filter_id)
            .unwrap_or(PlayerTeamFilter::All)
    });
    let mut show_joins = ui.data(|d| d.get_temp::<bool>(show_joins_id).unwrap_or(true));
    let mut show_teams = ui.data(|d| d.get_temp::<bool>(show_teams_id).unwrap_or(true));
    let mut show_gameplay = ui.data(|d| d.get_temp::<bool>(show_gameplay_id).unwrap_or(true));
    let mut show_other_sys = ui.data(|d| d.get_temp::<bool>(show_other_sys_id).unwrap_or(true));
    let mut filter_text = ui.data(|d| d.get_temp::<String>(filter_text_id).unwrap_or_default());

    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(t("#app_chat_filter_actions"));
        if ui.button(t("#app_chat_select_all")).clicked() {
            show_mm1 = true;
            show_mm2 = true;
            show_status = PlayerStatusFilter::All;
            show_team_filter = PlayerTeamFilter::All;
            show_joins = true;
            show_teams = true;
            show_gameplay = true;
            show_other_sys = true;
            changed = true;
        }
        if ui.button(t("#app_chat_clear_all")).clicked() {
            show_mm1 = false;
            show_mm2 = false;
            show_joins = false;
            show_teams = false;
            show_gameplay = false;
            show_other_sys = false;
            changed = true;
        }
    });

    ui.horizontal(|ui| {
        ui.label(t("#app_chat_player_chat"));
        if ui
            .checkbox(&mut show_mm1, t("#app_chat_all_chat"))
            .changed()
            || changed
        {
            ui.data_mut(|d| d.insert_temp(show_mm1_id, show_mm1));
        }
        if ui
            .checkbox(&mut show_mm2, t("#app_chat_team_chat"))
            .changed()
            || changed
        {
            ui.data_mut(|d| d.insert_temp(show_mm2_id, show_mm2));
        }
        if ui
            .radio_value(&mut show_status, PlayerStatusFilter::All, t("#app_chat_filter_all"))
            .changed()
            || changed
        {
            ui.data_mut(|d| d.insert_temp(show_status_id, show_status));
        }
        if ui
            .radio_value(&mut show_status, PlayerStatusFilter::Alive, t("#app_chat_filter_alive"))
            .changed()
            || changed
        {
            ui.data_mut(|d| d.insert_temp(show_status_id, show_status));
        }
        if ui
            .radio_value(&mut show_status, PlayerStatusFilter::Dead, t("#app_chat_filter_dead"))
            .changed()
            || changed
        {
            ui.data_mut(|d| d.insert_temp(show_status_id, show_status));
        }
    });

    ui.horizontal(|ui| {
        ui.label(t("#app_chat_team_filter"));
        if ui
            .radio_value(&mut show_team_filter, PlayerTeamFilter::All, t("#app_chat_team_filter_all"))
            .changed()
            || changed
        {
            ui.data_mut(|d| d.insert_temp(show_team_filter_id, show_team_filter));
        }
        if ui
            .radio_value(&mut show_team_filter, PlayerTeamFilter::Allies, t("#app_chat_team_filter_allies"))
            .changed()
            || changed
        {
            ui.data_mut(|d| d.insert_temp(show_team_filter_id, show_team_filter));
        }
        if ui
            .radio_value(&mut show_team_filter, PlayerTeamFilter::British, t("#app_chat_team_filter_british"))
            .changed()
            || changed
        {
            ui.data_mut(|d| d.insert_temp(show_team_filter_id, show_team_filter));
        }
        if ui
            .radio_value(&mut show_team_filter, PlayerTeamFilter::Axis, t("#app_chat_team_filter_axis"))
            .changed()
            || changed
        {
            ui.data_mut(|d| d.insert_temp(show_team_filter_id, show_team_filter));
        }
        if ui
            .radio_value(&mut show_team_filter, PlayerTeamFilter::Spectators, t("#app_chat_team_filter_spectators"))
            .changed()
            || changed
        {
            ui.data_mut(|d| d.insert_temp(show_team_filter_id, show_team_filter));
        }
    });

    ui.horizontal(|ui| {
        ui.label(t("#app_chat_system_logs"));
        if ui
            .checkbox(&mut show_joins, t("#app_chat_joins_leaves"))
            .changed()
            || changed
        {
            ui.data_mut(|d| d.insert_temp(show_joins_id, show_joins));
        }
        if ui
            .checkbox(&mut show_teams, t("#app_chat_team_changes"))
            .changed()
            || changed
        {
            ui.data_mut(|d| d.insert_temp(show_teams_id, show_teams));
        }
        if ui
            .checkbox(&mut show_gameplay, t("#app_chat_gameplay"))
            .changed()
            || changed
        {
            ui.data_mut(|d| d.insert_temp(show_gameplay_id, show_gameplay));
        }
        if ui
            .checkbox(&mut show_other_sys, t("#app_chat_other_system"))
            .changed()
            || changed
        {
            ui.data_mut(|d| d.insert_temp(show_other_sys_id, show_other_sys));
        }

        ui.add_space(20.0);
        ui.label(t("#app_chat_search"));
        if ui.text_edit_singleline(&mut filter_text).changed() {
            ui.data_mut(|d| d.insert_temp(filter_text_id, filter_text.clone()));
        }
    });

    if changed {
        ui.data_mut(|d| {
            d.insert_temp(show_mm1_id, show_mm1);
            d.insert_temp(show_mm2_id, show_mm2);
            d.insert_temp(show_status_id, show_status);
            d.insert_temp(show_team_filter_id, show_team_filter);
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
        ui.label(t("#app_chat_no_analysis"));
        return;
    };

    if messages.is_empty() {
        ui.label(t("#app_chat_no_messages"));
        return;
    }

    let current_path = file_info.map(|fi| fi.path.clone());
    let current_filter = crate::ChatFilterState {
        show_mm1,
        show_mm2,
        show_status,
        show_team_filter,
        show_joins,
        show_teams,
        show_gameplay,
        show_other_sys,
        filter_text: filter_text.clone(),
    };

    let cache_invalid = chat_cache.path != current_path || chat_cache.filter_state.as_ref() != Some(&current_filter);

    if cache_invalid {
        chat_cache.path = current_path;
        chat_cache.filter_state = Some(current_filter);

        let query = filter_text.to_lowercase();
        chat_cache.filtered_indices = messages
            .iter()
            .enumerate()
            .filter(|(_, msg)| {
                match msg.chat_type {
                    ChatType::Mm1 => {
                        if !show_mm1 {
                            return false;
                        }
                        let alive_dead_ok = match show_status {
                            PlayerStatusFilter::All => true,
                            PlayerStatusFilter::Alive => !msg.sender_dead,
                            PlayerStatusFilter::Dead => msg.sender_dead,
                        };
                        if !alive_dead_ok {
                            return false;
                        }
                        let team_ok = match show_team_filter {
                            PlayerTeamFilter::All => true,
                            PlayerTeamFilter::Allies => msg.sender_team == Some(Team::Allies),
                            PlayerTeamFilter::British => msg.sender_team == Some(Team::British),
                            PlayerTeamFilter::Axis => msg.sender_team == Some(Team::Axis),
                            PlayerTeamFilter::Spectators => msg.sender_team == Some(Team::Spectators),
                        };
                        if !team_ok {
                            return false;
                        }
                    }
                    ChatType::Mm2 => {
                        if !show_mm2 {
                            return false;
                        }
                        let alive_dead_ok = match show_status {
                            PlayerStatusFilter::All => true,
                            PlayerStatusFilter::Alive => !msg.sender_dead,
                            PlayerStatusFilter::Dead => msg.sender_dead,
                        };
                        if !alive_dead_ok {
                            return false;
                        }
                        let team_ok = match show_team_filter {
                            PlayerTeamFilter::All => true,
                            PlayerTeamFilter::Allies => msg.sender_team == Some(Team::Allies),
                            PlayerTeamFilter::British => msg.sender_team == Some(Team::British),
                            PlayerTeamFilter::Axis => msg.sender_team == Some(Team::Axis),
                            PlayerTeamFilter::Spectators => msg.sender_team == Some(Team::Spectators),
                        };
                        if !team_ok {
                            return false;
                        }
                    }
                    ChatType::System => {
                        let sys_category = if let Some(ref token) = msg.system_token {
                            let token_lower = token.to_lowercase();
                            if token_lower.contains("connect")
                                || token_lower.contains("join_game")
                                || token_lower.contains("joined_game")
                                || token_lower.contains("kick")
                                || token_lower.contains("disconnect")
                            {
                                "join_leave"
                            } else if token_lower.contains("joined_team")
                                || token_lower.contains("team")
                            {
                                "team_change"
                            } else if token_lower.contains("score")
                                || token_lower.contains("capture")
                                || token_lower.contains("cap")
                                || token_lower.contains("reinforce")
                            {
                                "gameplay"
                            } else {
                                "other"
                            }
                        } else {
                            "other"
                        };

                        match sys_category {
                            "join_leave" => {
                                if !show_joins {
                                    return false;
                                }
                            }
                            "team_change" => {
                                if !show_teams {
                                    return false;
                                }
                            }
                            "gameplay" => {
                                if !show_gameplay {
                                    return false;
                                }
                            }
                            _ => {
                                if !show_other_sys {
                                    return false;
                                }
                            }
                        }
                    }
                }

                if !query.is_empty() {
                    let name_matches = msg
                        .sender_name
                        .as_ref()
                        .map(|n| n.to_lowercase().contains(&query))
                        .unwrap_or(false);
                    let text_matches = msg.text.to_lowercase().contains(&query);
                    if !name_matches && !text_matches {
                        return false;
                    }
                }

                true
            })
            .map(|(idx, _)| idx)
            .collect();
    }

    if chat_cache.filtered_indices.is_empty() {
        ui.label(t("#app_chat_no_match"));
        return;
    }

    let text_style = egui::TextStyle::Body;
    let row_height = ui.text_style_height(&text_style);

    ScrollArea::vertical().auto_shrink([false; 2]).show_rows(
        ui,
        row_height,
        chat_cache.filtered_indices.len(),
        |ui, row_range| {
            for i in row_range {
                let msg_idx = chat_cache.filtered_indices[i];
                let msg = &messages[msg_idx];
                ui.horizontal(|ui| {
                    let time_str = format!("[{}]", format_game_time(&msg.time.viewdemo_offset));
                    ui.colored_label(Color32::from_rgb(140, 140, 140), time_str);

                    if msg.sender_dead {
                        ui.colored_label(
                            Color32::from_rgb(220, 50, 50),
                            t("#app_chat_dead_prefix"),
                        );
                    }

                    match msg.chat_type {
                        ChatType::Mm2 => {
                            let team_color = match msg.sender_team {
                                Some(Team::Allies) => Color32::from_rgb(34, 139, 34), // Forest Green
                                Some(Team::British) => crate::views::BRITISH_COLOR,   // Goldenrod/Gold
                                Some(Team::Axis) => Color32::from_rgb(178, 34, 34), // Firebrick Red
                                Some(Team::Spectators) => Color32::YELLOW, // Spectator yellow
                                _ => Color32::LIGHT_BLUE,                  // Default / Console
                            };
                            ui.colored_label(team_color, t("#app_chat_team_prefix"));
                        }
                        ChatType::System => {
                            ui.colored_label(
                                Color32::from_rgb(200, 150, 80),
                                t("#app_chat_system_prefix"),
                            );
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
                            render_system_message(ui, display_text.trim());
                        }
                        _ => {
                            let team_color = match msg.sender_team {
                                Some(Team::Allies) => Color32::from_rgb(34, 139, 34), // Forest Green
                                Some(Team::British) => crate::views::BRITISH_COLOR,   // Goldenrod/Gold
                                Some(Team::Axis) => Color32::from_rgb(178, 34, 34), // Firebrick Red
                                Some(Team::Spectators) => Color32::YELLOW, // Spectator yellow
                                _ => Color32::LIGHT_BLUE,                  // Default / Console
                            };

                            let name_fallback = t("#app_chat_unknown_sender");
                            let name = msg.sender_name.as_deref().unwrap_or(&name_fallback).trim();

                            let old_spacing = ui.spacing().item_spacing.x;
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.colored_label(team_color, name);
                            ui.label(": ");
                            ui.colored_label(Color32::WHITE, msg.text.trim());
                            ui.spacing_mut().item_spacing.x = old_spacing;
                        }
                    }
                });
            }
        },
    );
}

fn render_system_message(ui: &mut Ui, text: &str) {
    let sys_color = Color32::from_rgb(180, 220, 220);

    struct TeamStyle {
        patterns: &'static [&'static str],
        color: Color32,
    }

    let team_styles = &[
        TeamStyle {
            patterns: &["allies", "allied"],
            color: Color32::from_rgb(34, 139, 34), // Forest Green
        },
        TeamStyle {
            patterns: &["axis"],
            color: Color32::from_rgb(178, 34, 34), // Firebrick Red
        },
        TeamStyle {
            patterns: &["spectators", "spectator", "spec"],
            color: Color32::YELLOW, // Spectator yellow
        },
    ];

    let old_spacing = ui.spacing().item_spacing.x;
    ui.spacing_mut().item_spacing.x = 0.0;

    let mut remainder = text;

    loop {
        if remainder.is_empty() {
            break;
        }

        let remainder_lower = remainder.to_lowercase();
        let mut earliest_match: Option<(usize, usize, Color32)> = None;

        for style in team_styles {
            for pattern in style.patterns {
                if let Some(idx) = remainder_lower.find(pattern) {
                    let is_earlier = match earliest_match {
                        None => true,
                        Some((existing_idx, _, _)) => idx < existing_idx,
                    };
                    if is_earlier {
                        earliest_match = Some((idx, pattern.len(), style.color));
                    }
                }
            }
        }

        match earliest_match {
            Some((idx, len, color)) => {
                if idx > 0 {
                    ui.label(RichText::new(&remainder[..idx]).italics().color(sys_color));
                }
                let matched_text = &remainder[idx..idx + len];
                ui.label(RichText::new(matched_text).italics().color(color));
                remainder = &remainder[idx + len..];
            }
            None => {
                ui.label(RichText::new(remainder).italics().color(sys_color));
                break;
            }
        }
    }

    ui.spacing_mut().item_spacing.x = old_spacing;
}
