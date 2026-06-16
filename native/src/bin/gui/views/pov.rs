use crate::FileInfo;
use crate::views::{TABLE_ROW_HEIGHT, t, weapon_name};
use analysis::{Analysis, Weapon};
use egui::{Align, Layout, Ui};
use egui_extras::{Column, TableBuilder};

#[allow(dead_code)]
pub fn pov_analytics_ui(_file_info: Option<&FileInfo>, r: Option<&Analysis>, ui: &mut Ui) {
    ui.heading(t("#app_pov_heading"));
    ui.add_space(8.0);

    let analysis = match r {
        Some(a) => a,
        None => {
            ui.label(t("#app_chat_no_analysis"));
            return;
        }
    };

    if analysis.demo_info.demo_type != "POV" {
        ui.colored_label(egui::Color32::GRAY, t("#app_pov_no_stats"));
        return;
    }

    let stats = &analysis.state.pov_stats;

    // 1. Overview grid
    ui.strong(t("#app_pov_metrics_sub"));
    ui.add_space(4.0);

    egui::Grid::new("pov_overview_grid")
        .striped(true)
        .show(ui, |ui| {
            ui.label(t("#app_pov_hits_taken"));
            ui.label(format!("{}", stats.hits_taken));
            ui.end_row();

            ui.label(t("#app_pov_damage_taken"));
            ui.label(format!("{}", stats.total_damage_taken));
            ui.end_row();

            ui.label(t("#app_pov_avg_dmg_taken"));
            let avg_dmg = if stats.hits_taken > 0 {
                stats.total_damage_taken as f32 / stats.hits_taken as f32
            } else {
                0.0
            };
            ui.label(format!("{:.1}", avg_dmg));
            ui.end_row();

            ui.label(t("#app_pov_suicides"));
            ui.label(format!("{}", stats.suicides));
            ui.end_row();

            ui.label(t("#app_pov_tk_committed"));
            ui.label(format!("{}", stats.teamkills_committed));
            ui.end_row();

            ui.label(t("#app_pov_tk_suffered"));
            ui.label(format!("{}", stats.teamkills_suffered));
            ui.end_row();
        });

    ui.add_space(16.0);

    // 2. Weapons statistics table
    ui.strong(t("#app_pov_weapons_sub"));
    ui.add_space(4.0);

    let mut weapon_list: Vec<(&Weapon, &analysis::WeaponPovStats)> =
        stats.weapon_stats.iter().collect();
    // Sort by kills descending, then name
    weapon_list.sort_by(|(w_a, s_a), (w_b, s_b)| {
        let cmp = s_b.kills.cmp(&s_a.kills);
        if cmp == std::cmp::Ordering::Equal {
            format!("{:?}", w_a).cmp(&format!("{:?}", w_b))
        } else {
            cmp
        }
    });

    let selected_weapon_id = ui.make_persistent_id("pov_selected_weapon");
    let mut selected_weapon: Option<Weapon> = ui.data_mut(|d| d.get_temp(selected_weapon_id));

    if selected_weapon.is_none() && !weapon_list.is_empty() {
        selected_weapon = Some(weapon_list[0].0.clone());
    }

    ui.columns(2, |cols| {
        // Left Column: Table
        let ui_left = &mut cols[0];
        TableBuilder::new(ui_left)
            .striped(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::remainder())
            .column(Column::initial(60.0).resizable(true))
            .column(Column::initial(80.0).resizable(true))
            .column(Column::initial(80.0).resizable(true))
            .header(TABLE_ROW_HEIGHT, |mut row| {
                row.col(|ui| {
                    ui.strong(t("#app_col_weapon"));
                });
                row.col(|ui| {
                    ui.strong(t("#app_col_kills"));
                });
                row.col(|ui| {
                    ui.strong(t("#app_pov_col_bullets"));
                });
                row.col(|ui| {
                    ui.strong(t("#app_pov_col_reloads"));
                });
            })
            .body(|mut body| {
                for (weapon, w_stats) in &weapon_list {
                    let weapon = *weapon;
                    let is_selected = selected_weapon.as_ref() == Some(weapon);
                    body.row(TABLE_ROW_HEIGHT, |mut row| {
                        row.col(|ui| {
                            if ui
                                .selectable_label(is_selected, weapon_name(&weapon))
                                .clicked()
                            {
                                selected_weapon = Some(weapon.clone());
                            }
                        });
                        row.col(|ui| {
                            if ui
                                .selectable_label(is_selected, format!("{}", w_stats.kills))
                                .clicked()
                            {
                                selected_weapon = Some(weapon.clone());
                            }
                        });
                        row.col(|ui| {
                            if ui
                                .selectable_label(is_selected, format!("{}", w_stats.bullets_fired))
                                .clicked()
                            {
                                selected_weapon = Some(weapon.clone());
                            }
                        });
                        row.col(|ui| {
                            if ui
                                .selectable_label(is_selected, format!("{}", w_stats.reloads))
                                .clicked()
                            {
                                selected_weapon = Some(weapon.clone());
                            }
                        });
                    });
                }
            });

        // Right Column: Details Panel
        let ui_right = &mut cols[1];
        if let Some(weapon) = &selected_weapon {
            if let Some(w_stats) = stats.weapon_stats.get(weapon) {
                let is_sniper = matches!(
                    weapon,
                    Weapon::Springfield
                        | Weapon::ScopedK98
                        | Weapon::ScopedFg42
                        | Weapon::ScopedLeeEnfield
                );

                ui_right.vertical(|ui| {
                    let title = t("#app_pov_detail_title").replace("%s1", &weapon_name(weapon));
                    ui.strong(title);
                    ui.add_space(8.0);

                    egui::Grid::new("pov_weapon_detail_grid")
                        .striped(true)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(t("#app_col_kills"));
                            ui.label(format!("{}", w_stats.kills));
                            ui.end_row();

                            ui.label(t("#app_pov_col_bullets"));
                            ui.label(format!("{}", w_stats.bullets_fired));
                            ui.end_row();

                            ui.label(t("#app_pov_col_reloads"));
                            ui.label(format!("{}", w_stats.reloads));
                            ui.end_row();

                            if is_sniper {
                                ui.label(t("#app_pov_detail_scoped"));
                                ui.label(format!("{}", w_stats.scoped_kills));
                                ui.end_row();

                                ui.label(t("#app_pov_detail_noscopes"));
                                ui.label(format!("{}", w_stats.noscopes));
                                ui.end_row();

                                ui.label(t("#app_pov_detail_ratio"));
                                let scoped_pct = if w_stats.kills > 0 {
                                    w_stats.scoped_kills as f32 / w_stats.kills as f32
                                } else {
                                    0.0
                                };
                                let bar_text = format!("{:.1}% Scoped", scoped_pct * 100.0);
                                ui.add(egui::ProgressBar::new(scoped_pct).text(bar_text));
                                ui.end_row();
                            }
                        });
                });
            }
        } else {
            ui_right.colored_label(egui::Color32::GRAY, t("#app_pov_select_weapon"));
        }
    });

    // Save the selected weapon state back to temp data
    ui.data_mut(|d| d.insert_temp(selected_weapon_id, selected_weapon));
}
