use crate::FileInfo;
use crate::views::{PlayerHighlighting, TABLE_ROW_HEIGHT, t, weapon_name};
use analysis::{Analysis, Player};
use egui::{Align, Layout, ScrollArea, Ui};
use egui_extras::{Column, TableBuilder};
use humantime::format_duration;
use std::time::Duration;

pub fn kill_streaks_ui(
    file_info: Option<&FileInfo>,
    r: Option<&Analysis>,
    player_highlighting: &mut PlayerHighlighting,
    ui: &mut Ui,
) {
    ui.heading(t("#app_streaks_heading"));
    ui.add_space(8.0);

    if let Some(analysis) = r {
        let mut players_with_streaks: Vec<&Player> = analysis
            .state
            .players
            .iter()
            .filter(|p| !p.kill_streaks.is_empty())
            .collect();
        players_with_streaks.sort_by(|l, r| l.name.cmp(&r.name));

        if players_with_streaks.is_empty() {
            ui.label(t("#app_chat_no_messages"));
            return;
        }

        // Persistent selected player ID
        let tab_id = if let Some(fi) = file_info {
            egui::Id::new(&fi.path).with("kill_streaks")
        } else {
            egui::Id::new("blank_report").with("kill_streaks")
        };
        let selected_player_id_key = tab_id.with("selected_player_id");

        let mut selected_id = ui.data(|d| d.get_temp::<analysis::PlayerGlobalId>(selected_player_id_key));

        // Sync with player_highlighting (Scoreboard selection)
        let highlighted_id = player_highlighting.highlighted.iter().next().cloned();

        let mut current_id = None;
        if let Some(ref h_id) = highlighted_id {
            // Check if highlighted player has streaks
            if players_with_streaks.iter().any(|p| &p.id == h_id) {
                current_id = Some(h_id.clone());
            }
        }

        // If not synced, fall back to egui temp data or first player with streaks
        if current_id.is_none() {
            if let Some(ref sel_id) = selected_id {
                if players_with_streaks.iter().any(|p| &p.id == sel_id) {
                    current_id = Some(sel_id.clone());
                }
            }
        }

        if current_id.is_none() {
            current_id = players_with_streaks.first().map(|p| p.id.clone());
        }

        // Update both if changed
        if selected_id.as_ref() != current_id.as_ref() {
            selected_id = current_id.clone();
            if let Some(ref id) = selected_id {
                ui.data_mut(|d| d.insert_temp(selected_player_id_key, id.clone()));
            }
        }

        // Update player_highlighting to match selected player
        if let Some(ref id) = selected_id {
            if !player_highlighting.highlighted.contains(id) {
                player_highlighting.highlighted.clear();
                player_highlighting.highlighted.insert(id.clone());
            }
        }

        // Render Dropdown
        ui.horizontal(|ui| {
            ui.label(t("#app_streaks_select_player"));
            
            let current_name = selected_id.as_ref()
                .and_then(|id| players_with_streaks.iter().find(|p| &p.id == id))
                .map(|p| p.name.as_str())
                .unwrap_or("");

            egui::ComboBox::from_id_salt("streaks_player_select")
                .selected_text(current_name)
                .show_ui(ui, |ui| {
                    for p in &players_with_streaks {
                        let selected = selected_id.as_ref() == Some(&p.id);
                        if ui.selectable_label(selected, &p.name).clicked() {
                            selected_id = Some(p.id.clone());
                            ui.data_mut(|d| d.insert_temp(selected_player_id_key, p.id.clone()));
                            
                            // Update highlight selection
                            player_highlighting.highlighted.clear();
                            player_highlighting.highlighted.insert(p.id.clone());
                        }
                    }
                });
        });

        ui.add_space(8.0);

        // Render Table for selected player
        if let Some(ref id) = selected_id {
            if let Some(p) = players_with_streaks.iter().find(|p| &p.id == id) {
                ScrollArea::vertical()
                    .id_salt("player_kill_streaks_scroll")
                    .auto_shrink(false)
                    .min_scrolled_height(260.)
                    .show(ui, |ui| {
                        kill_streaks_table_ui(p, ui);
                    });
            }
        }
    } else {
        ui.label(t("#app_chat_no_analysis"));
    }
}

pub fn kill_streaks_table_ui(p: &Player, ui: &mut Ui) {
    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .columns(Column::auto(), 5)
        .header(TABLE_ROW_HEIGHT, |mut row| {
            row.col(|ui| {
                ui.strong(t("#app_col_wave"));
            });
            row.col(|ui| {
                ui.strong(t("#app_col_total_kills"));
            });
            row.col(|ui| {
                ui.strong(t("#app_col_start_time"));
            });
            row.col(|ui| {
                ui.strong(t("#app_col_duration"));
            });
            row.col(|ui| {
                ui.strong(t("#app_col_weapons_used"));
            });
        })
        .body(|mut body| {
            for (wave, streak) in p.kill_streaks.iter().enumerate() {
                if let (Some((start, _)), Some((end, _))) =
                    (streak.kills.first(), streak.kills.last())
                {
                    body.row(TABLE_ROW_HEIGHT, |mut row| {
                        row.col(|ui| {
                            ui.label((wave + 1).to_string());
                        });

                        row.col(|ui| {
                            ui.label(streak.kills.len().to_string());
                        });

                        row.col(|ui| {
                            let start = Duration::new(start.viewdemo_offset.as_secs(), 0);

                            ui.label(format_duration(start).to_string());
                        });

                        row.col(|ui| {
                            let duration = Duration::new((end - start).as_secs(), 0);

                            ui.label(format_duration(duration).to_string());
                        });

                        row.col(|ui| {
                            let mut grouped = Vec::new();
                            for (_, weapon) in &streak.kills {
                                let name = weapon_name(weapon);
                                if let Some((last_name, count)) = grouped.last_mut() {
                                    if *last_name == name {
                                        *count += 1;
                                        continue;
                                    }
                                }
                                grouped.push((name, 1));
                            }
                            let weapons = grouped
                                .into_iter()
                                .map(|(name, count)| {
                                    if count > 1 {
                                        format!("{} x{}", name, count)
                                    } else {
                                        name
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join(", ");

                            ui.label(weapons);
                        });
                    });
                }
            }
        });
}
