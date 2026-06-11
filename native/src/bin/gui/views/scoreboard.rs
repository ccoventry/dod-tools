use crate::views::{PlayerHighlighting, TABLE_ROW_HEIGHT, t};
use analysis::{Analysis, Player, SteamId, Team, translate_key};
use egui::{Align, Layout, Ui};
use egui_extras::{Column, TableBody, TableBuilder};

fn team_name(team: &Team) -> String {
    match team {
        Team::Allies => translate_key("#teamname_allies").unwrap_or_else(|| "Allies".to_string()),
        Team::British => translate_key("#teamname_british").unwrap_or_else(|| "British".to_string()),
        Team::Axis => translate_key("#teamname_axis").unwrap_or_else(|| "Axis".to_string()),
        Team::Spectators => {
            translate_key("#teamname_spectators").unwrap_or_else(|| "Spectators".to_string())
        }
        Team::Unassigned => t("#app_team_unassigned"),
    }
}

/// Stable numeric rank for team ordering (lower = higher in the table).
/// Using an integer avoids locale-dependent string comparisons.
fn team_sort_rank(team: Option<&Team>) -> u8 {
    match team {
        Some(Team::Allies) | Some(Team::British) => 0,
        Some(Team::Axis) => 1,
        Some(Team::Spectators) => 2,
        Some(Team::Unassigned) | None => 3,
    }
}

pub fn scoreboard_ui(
    analysis: Option<&Analysis>,
    player_highlighting: &mut PlayerHighlighting,
    ui: &mut Ui,
) {
    let match_result_fragment = if let Some(analysis) = analysis {
        let is_british = analysis.state.allies_are_british;
        let allies_team = if is_british { Team::British } else { Team::Allies };
        let allies_score = analysis.state.team_scores.get_team_score(allies_team);
        let axis_score = analysis.state.team_scores.get_team_score(Team::Axis);
        let allies_key = if is_british { "#teamname_british" } else { "#teamname_allies" };
        let allies_default = if is_british { "British" } else { "Allies" };
        let allies = translate_key(allies_key).unwrap_or_else(|| allies_default.to_string());
        let axis = translate_key("#teamname_axis").unwrap_or_else(|| "Axis".to_string());
        format!(
            ": {} ({}) {} {} ({})",
            allies,
            allies_score,
            if allies_score > axis_score { ">" } else { "<" },
            axis,
            axis_score
        )
    } else {
        String::new()
    };

    ui.heading(format!(
        "{}{match_result_fragment}",
        t("#app_scoreboard_heading")
    ));
    ui.add_space(8.0);

    ui.scope(|ui| {
        let columns = [
            "#app_col_id",
            "#app_col_name",
            "#app_col_team",
            "#app_col_class",
            "#app_col_score",
            "#app_col_kills",
            "#app_col_deaths",
        ];

        let table = TableBuilder::new(ui)
            .striped(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .max_scroll_height(260.)
            .column(Column::auto_with_initial_suggestion(150.))
            .columns(Column::auto(), columns.len() - 1);

        table
            .header(TABLE_ROW_HEIGHT, |mut header| {
                for column in columns {
                    header.col(|ui| {
                        let col_label = if column.starts_with('#') {
                            t(column)
                        } else {
                            column.to_string()
                        };
                        ui.strong(col_label);
                    });
                }
            })
            .body(|ref mut body| {
                if let Some(analysis) = analysis {
                    struct SortablePlayer<'a> {
                        player: &'a Player,
                        steam_id_str: String,
                    }

                    let mut players: Vec<SortablePlayer> = analysis
                        .state
                        .players
                        .iter()
                        .map(|p| {
                            let steam_id_str = SteamId::try_from(&p.id)
                                .map(|s| s.to_string())
                                .unwrap_or_else(|_| p.id.to_string());
                            SortablePlayer { player: p, steam_id_str }
                        })
                        .collect();

                    // Fixed sort order: Team ASC, Score DESC, Kills DESC,
                    // Deaths ASC, Name ASC, ID ASC.
                    players.sort_by(|a, b| {
                        team_sort_rank(a.player.team.as_ref())
                            .cmp(&team_sort_rank(b.player.team.as_ref()))
                            .then_with(|| b.player.stats.0.cmp(&a.player.stats.0)) // Score DESC
                            .then_with(|| b.player.stats.1.cmp(&a.player.stats.1)) // Kills DESC
                            .then_with(|| a.player.stats.2.cmp(&b.player.stats.2)) // Deaths ASC
                            .then_with(|| a.player.name.cmp(&b.player.name))       // Name ASC
                            .then_with(|| a.steam_id_str.cmp(&b.steam_id_str))     // ID ASC
                    });

                    for sp in players {
                        scoreboard_row_ui(sp.player, player_highlighting, body);
                    }
                }
            });
    });
}

pub fn scoreboard_row_ui(
    p: &Player,
    player_highlighting: &mut PlayerHighlighting,
    body: &mut TableBody,
) {
    let cell_label = |ui: &mut Ui, text: &str| -> egui::Response {
        ui.add(egui::Label::new(text).sense(egui::Sense::click()))
    };

    let clicked = std::cell::Cell::new(false);

    body.row(TABLE_ROW_HEIGHT, |mut row| {
        let is_selected = player_highlighting.highlighted.contains(&p.id);
        row.set_selected(is_selected);

        row.col(|ui| {
            match SteamId::try_from(&p.id) {
                Ok(steam_id) => {
                    let link_text = steam_id.to_string();
                    let link_url = format!("https://steamcommunity.com/profiles/{}", p.id);
                    let link_resp = ui.hyperlink_to(link_text, link_url);
                    if link_resp.clicked() {
                        clicked.set(true);
                    }
                }
                _ => {
                    let resp = ui.add(egui::Label::new(p.id.to_string()).sense(egui::Sense::click()));
                    if resp.clicked() {
                        clicked.set(true);
                    }
                }
            };
        });

        row.col(|ui| {
            let resp = cell_label(ui, &p.name);
            if resp.clicked() {
                clicked.set(true);
            }
        });

        row.col(|ui| {
            let team_str = match &p.team {
                None => t("#app_team_unknown"),
                Some(team) => team_name(team),
            };
            let resp = cell_label(ui, &team_str);
            if resp.clicked() {
                clicked.set(true);
            }
        });

        row.col(|ui| {
            let class_str = match &p.class {
                None => t("#app_team_unknown"),
                Some(x) => format!("{x:?}"),
            };
            let resp = cell_label(ui, &class_str);
            if resp.clicked() {
                clicked.set(true);
            }
        });

        row.col(|ui| {
            let resp = cell_label(ui, &p.stats.0.to_string());
            if resp.clicked() {
                clicked.set(true);
            }
        });

        row.col(|ui| {
            let resp = cell_label(ui, &p.stats.1.to_string());
            if resp.clicked() {
                clicked.set(true);
            }
        });

        row.col(|ui| {
            let resp = cell_label(ui, &p.stats.2.to_string());
            if resp.clicked() {
                clicked.set(true);
            }
        });
    });

    if clicked.get() {
        if player_highlighting.highlighted.contains(&p.id) {
            player_highlighting.highlighted.remove(&p.id);
        } else {
            player_highlighting.highlighted.clear();
            player_highlighting.highlighted.insert(p.id.clone());
        }
    }
}
