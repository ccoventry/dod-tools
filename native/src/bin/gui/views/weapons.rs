use crate::FileInfo;
use crate::views::{PlayerHighlighting, TABLE_ROW_HEIGHT};
use analysis::{Analysis, Team};
use egui::{Align, CollapsingHeader, Layout, ScrollArea, Ui};
use egui_extras::{Column, TableBuilder};

pub fn weapon_breakdowns_ui(
    _file_info: Option<&FileInfo>,
    r: Option<&Analysis>,
    player_highlighting: &PlayerHighlighting,
    ui: &mut Ui,
) {
    ui.heading("Weapon Breakdowns");
    ui.add_space(8.0);
    
    ui.scope(|ui| {
            team_weapon_breakdown_ui(r, ui);

            CollapsingHeader::new("Player Breakdowns")
                .default_open(false)
                .show(ui, |ui| {
                    if let Some(analysis) = r {
                        let mut players = Vec::from_iter(&analysis.state.players);
                        players.sort_by(|l, r| l.name.cmp(&r.name));

                        ScrollArea::vertical()
                            .id_salt("player_weapons_scroll")
                            .auto_shrink(false)
                            .min_scrolled_height(260.)
                            .show(ui, |ui| {
                                for p in players {
                                    if !player_highlighting.highlighted.is_empty()
                                        && !player_highlighting.highlighted.contains(&p.id)
                                    {
                                        continue;
                                    }

                                    CollapsingHeader::new(&p.name)
                                        .default_open(false)
                                        .show(ui, |ui| {
                                            weapon_breakdown_table_ui(&p.weapon_breakdown, ui);
                                        });
                                }
                            });
                    }
                });
    });
}

pub fn team_weapon_breakdown_ui(r: Option<&Analysis>, ui: &mut Ui) {
    CollapsingHeader::new("Team Breakdowns")
        .default_open(false)
        .show(ui, |ui| {
            let mut allies_breakdown = std::collections::HashMap::new();
            let mut axis_breakdown = std::collections::HashMap::new();

            if let Some(analysis) = r {
                for p in &analysis.state.players {
                    if let Some(team) = &p.team {
                        let target_map = match team {
                            Team::Allies => Some(&mut allies_breakdown),
                            Team::Axis => Some(&mut axis_breakdown),
                            _ => None,
                        };

                        if let Some(target_map) = target_map {
                            for (weapon, (kills, teamkills)) in &p.weapon_breakdown {
                                let entry = target_map.entry(weapon.clone()).or_insert((0, 0));
                                entry.0 += kills;
                                entry.1 += teamkills;
                            }
                        }
                    }
                }
            }

            CollapsingHeader::new("Allies")
                .default_open(true)
                .show(ui, |ui| {
                    weapon_breakdown_table_ui(&allies_breakdown, ui);
                });

            CollapsingHeader::new("Axis")
                .default_open(true)
                .show(ui, |ui| {
                    weapon_breakdown_table_ui(&axis_breakdown, ui);
                });
        });
}

pub fn weapon_breakdown_table_ui<W: std::fmt::Debug>(
    breakdown: &std::collections::HashMap<W, (u32, u32)>,
    ui: &mut Ui,
) {
    let mut weapon_breakdown: Vec<(String, (u32, u32))> = breakdown
        .iter()
        .map(|(w, stats)| (format!("{:?}", w), *stats))
        .collect();

    weapon_breakdown.sort_by(|(name_a, l), (name_b, r)| {
        let cmp = l.cmp(r).reverse();
        if cmp == std::cmp::Ordering::Equal {
            name_a.cmp(name_b)
        } else {
            cmp
        }
    });

    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .columns(Column::auto(), 5)
        .header(TABLE_ROW_HEIGHT, |mut row| {
            row.col(|ui| {
                ui.strong("Weapon");
            });
            row.col(|ui| {
                ui.strong("Kills");
            });
            row.col(|ui| {
                ui.strong("% of Total");
            });
            row.col(|ui| {
                ui.strong("Team Kills");
            });
            row.col(|ui| {
                ui.strong("% of Total");
            });
        })
        .body(|mut body| {
            let (total_kills, total_teamkills) = weapon_breakdown
                .iter()
                .fold((0, 0), |(k_sum, tk_sum), (_, (k, tk))| {
                    (k_sum + k, tk_sum + tk)
                });

            for (weapon_name, (kills, teamkills)) in weapon_breakdown {
                body.row(TABLE_ROW_HEIGHT, |mut row| {
                    row.col(|ui| {
                        ui.label(weapon_name);
                    });

                    row.col(|ui| {
                        ui.label(format!("{kills}"));
                    });

                    row.col(|ui| {
                        let pct_of_total = if kills + total_kills > 0 {
                            ((kills as f32 / total_kills as f32) * 100.).floor()
                        } else {
                            0.
                        };

                        ui.label(format!("{pct_of_total}%"));
                    });

                    row.col(|ui| {
                        ui.label(format!("{teamkills}"));
                    });

                    row.col(|ui| {
                        let pct_of_total = if teamkills + total_teamkills > 0 {
                            ((teamkills as f32 / total_teamkills as f32) * 100.).floor()
                        } else {
                            0.
                        };

                        ui.label(format!("{pct_of_total}%"));
                    });
                });
            }
        });
}
