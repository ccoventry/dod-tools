use crate::views::{
    ALLIES_COLOR, AXIS_COLOR, BRITISH_COLOR, TABLE_ROW_HEIGHT, t, weapon_name,
};
use analysis::{Analysis, Team, Weapon};
use egui::{Align, CollapsingHeader, Layout, Ui, Grid};
use egui_extras::{Column, TableBuilder};
use std::collections::HashMap;

pub fn team_details_ui(analysis: Option<&Analysis>, ui: &mut Ui) {
    ui.heading(t("#app_team_details_heading"));
    ui.add_space(8.0);

    let analysis = match analysis {
        Some(a) => a,
        None => {
            ui.label(t("#app_chat_no_analysis"));
            return;
        }
    };

    // 1. Calculate Team Stats
    let is_british = analysis.state.allies_are_british;
    let allies_team = if is_british { Team::British } else { Team::Allies };

    let mut allies_kills = 0;
    let mut allies_deaths = 0;
    let mut allies_players = 0;

    let mut axis_kills = 0;
    let mut axis_deaths = 0;
    let mut axis_players = 0;

    for p in &analysis.state.players {
        if let Some(ref team) = p.team {
            match team {
                Team::Allies | Team::British => {
                    allies_kills += p.stats.1;
                    allies_deaths += p.stats.2;
                    allies_players += 1;
                }
                Team::Axis => {
                    axis_kills += p.stats.1;
                    axis_deaths += p.stats.2;
                    axis_players += 1;
                }
                _ => {}
            }
        }
    }

    let allies_score = analysis.state.team_scores.get_team_score(allies_team.clone());
    let axis_score = analysis.state.team_scores.get_team_score(Team::Axis);

    let allies_kd = if allies_deaths > 0 { allies_kills as f32 / allies_deaths as f32 } else { allies_kills as f32 };
    let axis_kd = if axis_deaths > 0 { axis_kills as f32 / axis_deaths as f32 } else { axis_kills as f32 };

    let allies_label = if is_british { "British" } else { "Allies" };
    let allies_color = if is_british { BRITISH_COLOR } else { ALLIES_COLOR };

    // Draw side-by-side Overview table
    ui.strong("Match Overview");
    ui.add_space(4.0);

    Grid::new("team_overview_grid")
        .striped(true)
        .spacing([24.0, 8.0])
        .show(ui, |ui| {
            // Headers
            ui.label("");
            ui.colored_label(allies_color, egui::RichText::new(allies_label).strong());
            ui.colored_label(AXIS_COLOR, egui::RichText::new("Axis").strong());
            ui.end_row();

            // Round Score
            ui.strong(t("#app_team_details_round_score"));
            ui.label(format!("{}", allies_score));
            ui.label(format!("{}", axis_score));
            ui.end_row();

            // Total Kills
            ui.strong("Total Kills");
            ui.label(format!("{}", allies_kills));
            ui.label(format!("{}", axis_kills));
            ui.end_row();

            // Total Deaths
            ui.strong("Total Deaths");
            ui.label(format!("{}", allies_deaths));
            ui.label(format!("{}", axis_deaths));
            ui.end_row();

            // Team K/D
            ui.strong(t("#app_team_details_team_kd"));
            ui.label(format!("{:.2}", allies_kd));
            ui.label(format!("{:.2}", axis_kd));
            ui.end_row();

            // Active Players
            ui.strong(t("#app_team_details_active_players"));
            ui.label(format!("{}", allies_players));
            ui.label(format!("{}", axis_players));
            ui.end_row();
        });

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    // 2. Team Weapon Breakdowns
    ui.strong("Team Weapon Performance");
    ui.add_space(6.0);

    let mut allies_breakdown = HashMap::new();
    let mut british_breakdown = HashMap::new();
    let mut axis_breakdown = HashMap::new();

    for p in &analysis.state.players {
        if let Some(team) = &p.team {
            let target_map = match team {
                Team::Allies => Some(&mut allies_breakdown),
                Team::British => Some(&mut british_breakdown),
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

    if !allies_breakdown.is_empty() || british_breakdown.is_empty() {
        CollapsingHeader::new(if is_british { "Allies (US)" } else { "Allies" })
            .default_open(true)
            .show(ui, |ui| {
                weapon_breakdown_table_ui(&allies_breakdown, ui);
            });
    }

    if !british_breakdown.is_empty() {
        CollapsingHeader::new("British")
            .default_open(true)
            .show(ui, |ui| {
                weapon_breakdown_table_ui(&british_breakdown, ui);
            });
    }

    CollapsingHeader::new("Axis")
        .default_open(true)
        .show(ui, |ui| {
            weapon_breakdown_table_ui(&axis_breakdown, ui);
        });
}

fn weapon_breakdown_table_ui(
    breakdown: &HashMap<Weapon, (u32, u32)>,
    ui: &mut Ui,
) {
    let mut weapon_breakdown: Vec<(String, (u32, u32))> = breakdown
        .iter()
        .map(|(w, stats)| (weapon_name(w), *stats))
        .collect();

    weapon_breakdown.sort_by(|(name_a, l), (name_b, r)| {
        let cmp = l.cmp(r).reverse();
        if cmp == std::cmp::Ordering::Equal {
            name_a.cmp(name_b)
        } else {
            cmp
        }
    });

    let (total_kills, total_teamkills) = weapon_breakdown
        .iter()
        .fold((0, 0), |(k_sum, tk_sum), (_, (k, tk))| {
            (k_sum + k, tk_sum + tk)
        });

    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .columns(Column::auto(), 5)
        .header(TABLE_ROW_HEIGHT, |mut row| {
            row.col(|ui| { ui.strong(t("#app_col_weapon")); });
            row.col(|ui| { ui.strong(t("#app_col_kills")); });
            row.col(|ui| { ui.strong(t("#app_col_pct_total")); });
            row.col(|ui| { ui.strong(t("#app_col_teamkills")); });
            row.col(|ui| { ui.strong(t("#app_col_pct_total")); });
        })
        .body(|mut body| {
            for (weapon_name, (kills, teamkills)) in weapon_breakdown {
                body.row(TABLE_ROW_HEIGHT, |mut row| {
                    row.col(|ui| { ui.label(weapon_name); });
                    row.col(|ui| { ui.label(format!("{kills}")); });
                    row.col(|ui| {
                        let pct = if total_kills > 0 {
                            (kills as f32 / total_kills as f32) * 100.
                        } else {
                            0.
                        };
                        ui.horizontal(|ui| {
                            ui.label(format!("{:.1}%", pct));
                            ui.add(egui::ProgressBar::new(pct / 100.0).desired_width(40.0));
                        });
                    });
                    row.col(|ui| { ui.label(format!("{teamkills}")); });
                    row.col(|ui| {
                        let pct = if total_teamkills > 0 {
                            (teamkills as f32 / total_teamkills as f32) * 100.
                        } else {
                            0.
                        };
                        ui.horizontal(|ui| {
                            ui.label(format!("{:.1}%", pct));
                            ui.add(egui::ProgressBar::new(pct / 100.0).desired_width(40.0));
                        });
                    });
                });
            }
        });
}
