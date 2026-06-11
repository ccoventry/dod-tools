use crate::FileInfo;
use crate::views::{PlayerHighlighting, TABLE_ROW_HEIGHT, t, weapon_name};
use analysis::{Analysis, Player, Weapon};
use egui::{Align, Layout, ScrollArea, Ui};
use egui_extras::{Column, TableBuilder};
use humantime::format_duration;
use std::collections::HashSet;
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

        ui.add_space(4.0);

        // Render weapon filter checkboxes for selected player
        if let Some(ref id) = selected_id {
            if let Some(p) = players_with_streaks.iter().find(|p| &p.id == id) {
                let filter_key = tab_id
                    .with(id.to_string())
                    .with("weapon_filters");

                // Load currently disabled weapons (empty = all enabled)
                let mut disabled_weapons: HashSet<String> =
                    ui.data(|d| d.get_temp(filter_key).unwrap_or_default());

                // Collect all unique weapons across this player's streaks
                let mut all_weapons: Vec<Weapon> = p
                    .kill_streaks
                    .iter()
                    .flat_map(|s| s.kills.iter().map(|(_, w)| w.clone()))
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();
                all_weapons.sort_by_key(|w| weapon_name(w));

                if !all_weapons.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(t("#app_streaks_filter_weapons")).small().weak());
                        if ui.small_button(t("#app_chat_select_all")).clicked() {
                            disabled_weapons.clear();
                            ui.data_mut(|d| d.insert_temp(filter_key, disabled_weapons.clone()));
                        }
                        if ui.small_button(t("#app_chat_clear_all")).clicked() {
                            disabled_weapons = all_weapons
                                .iter()
                                .map(|w| format!("{:?}", w))
                                .collect();
                            ui.data_mut(|d| d.insert_temp(filter_key, disabled_weapons.clone()));
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for weapon in &all_weapons {
                            let name = weapon_name(weapon);
                            let key = format!("{:?}", weapon);
                            let mut enabled = !disabled_weapons.contains(&key);
                            if ui.checkbox(&mut enabled, &name).changed() {
                                if enabled {
                                    disabled_weapons.remove(&key);
                                } else {
                                    disabled_weapons.insert(key);
                                }
                                ui.data_mut(|d| d.insert_temp(filter_key, disabled_weapons.clone()));
                            }
                        }
                    });
                    ui.add_space(4.0);
                }

                ui.separator();
                ui.add_space(4.0);

                ScrollArea::vertical()
                    .id_salt("player_kill_streaks_scroll")
                    .auto_shrink(false)
                    .min_scrolled_height(260.)
                    .show(ui, |ui| {
                        kill_streaks_table_ui(p, &disabled_weapons, ui);
                    });
            }
        }
    } else {
        ui.label(t("#app_chat_no_analysis"));
    }
}

pub fn kill_streaks_table_ui(p: &Player, disabled_weapons: &HashSet<String>, ui: &mut Ui) {
    // Streak summary header
    TableBuilder::new(ui)
        .striped(false)
        .cell_layout(Layout::left_to_right(Align::Center))
        .columns(Column::auto(), 5)
        .header(TABLE_ROW_HEIGHT, |mut row| {
            row.col(|ui| { ui.strong(t("#app_col_wave")); });
            row.col(|ui| { ui.strong(t("#app_col_total_kills")); });
            row.col(|ui| { ui.strong(t("#app_col_start_time")); });
            row.col(|ui| { ui.strong(t("#app_col_duration")); });
            row.col(|ui| { ui.strong(t("#app_col_weapons_used")); });
        })
        .body(|mut body| {
            let mut displayed_wave = 0usize;

            for streak in &p.kill_streaks {
                // Apply kill-level weapon filter
                let filtered_kills: Vec<_> = streak
                    .kills
                    .iter()
                    .filter(|(_, w)| !disabled_weapons.contains(&format!("{:?}", w)))
                    .collect();

                // Skip streaks with no remaining kills after filtering
                if filtered_kills.is_empty() {
                    continue;
                }

                displayed_wave += 1;

                let first = filtered_kills.first().unwrap();
                let last = filtered_kills.last().unwrap();
                let start_dur = Duration::new(first.0.viewdemo_offset.as_secs(), 0);
                let total_dur = Duration::new(
                    last.0.viewdemo_offset
                        .checked_sub(first.0.viewdemo_offset)
                        .unwrap_or(Duration::ZERO)
                        .as_secs(),
                    0,
                );

                // Weapons summary for this filtered streak
                let mut grouped: Vec<(String, usize)> = Vec::new();
                for (_, weapon) in &filtered_kills {
                    let name = weapon_name(weapon);
                    if let Some((last_name, count)) = grouped.last_mut() {
                        if *last_name == name {
                            *count += 1;
                            continue;
                        }
                    }
                    grouped.push((name, 1));
                }
                let weapons_summary = grouped
                    .iter()
                    .map(|(name, count)| {
                        if *count > 1 { format!("{} x{}", name, count) } else { name.clone() }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");

                // Streak summary row
                body.row(TABLE_ROW_HEIGHT, |mut row| {
                    row.col(|ui| { ui.label(displayed_wave.to_string()); });
                    row.col(|ui| { ui.label(filtered_kills.len().to_string()); });
                    row.col(|ui| { ui.label(format_duration(start_dur).to_string()); });
                    row.col(|ui| { ui.label(format_duration(total_dur).to_string()); });
                    row.col(|ui| { ui.label(&weapons_summary); });
                });

                // Per-kill sub-rows with Δms intervals
                for (i, (time, weapon)) in filtered_kills.iter().enumerate() {
                    let delta_label = if i == 0 {
                        "  —".to_string()
                    } else {
                        let prev_time = &filtered_kills[i - 1].0;
                        let delta_ms = time.viewdemo_offset
                            .checked_sub(prev_time.viewdemo_offset)
                            .unwrap_or(Duration::ZERO)
                            .as_millis();
                        format!("  +{} ms", format_with_thousands(delta_ms))
                    };

                    body.row(TABLE_ROW_HEIGHT, |mut row| {
                        row.col(|ui| {
                            ui.label(
                                egui::RichText::new(format!("  ↳ {}", i + 1))
                                    .weak()
                                    .small(),
                            );
                        });
                        row.col(|_ui| {}); // kills count — not applicable per-kill
                        row.col(|ui| {
                            // Absolute time of this kill
                            let abs_ms = time.viewdemo_offset.as_millis();
                            ui.label(
                                egui::RichText::new(format!("  {}s", abs_ms / 1000))
                                    .weak()
                                    .small(),
                            );
                        });
                        row.col(|ui| {
                            ui.label(egui::RichText::new(&delta_label).weak().small());
                        });
                        row.col(|ui| {
                            ui.label(
                                egui::RichText::new(weapon_name(weapon))
                                    .weak()
                                    .small(),
                            );
                        });
                    });
                }
            }
        });
}

/// Format a large integer with thousands separators (e.g. 12345 → "12,345").
fn format_with_thousands(n: u128) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
