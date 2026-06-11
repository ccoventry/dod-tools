use crate::FileInfo;
use crate::views::{PlayerHighlighting, TABLE_ROW_HEIGHT, t};
use analysis::{Analysis, Player, SteamId, Team, translate_key};
use egui::{Align, Layout, Ui};
use egui_extras::{Column, TableBody, TableBuilder};

#[derive(Clone, Copy)]
pub struct ScoreboardSortState {
    pub col_idx: usize,
    pub desc: bool,
}

impl Default for ScoreboardSortState {
    fn default() -> Self {
        Self {
            col_idx: 4,
            desc: true,
        } // Score descending
    }
}

fn team_name(team: &Team) -> String {
    match team {
        Team::Allies => translate_key("#teamname_allies").unwrap_or_else(|| "Allies".to_string()),
        Team::Axis => translate_key("#teamname_axis").unwrap_or_else(|| "Axis".to_string()),
        Team::Spectators => {
            translate_key("#teamname_spectators").unwrap_or_else(|| "Spectators".to_string())
        }
        Team::Unassigned => t("#app_team_unassigned"),
    }
}

pub fn scoreboard_ui(
    file_info: Option<&FileInfo>,
    r: Option<&Analysis>,
    player_highlighting: &mut PlayerHighlighting,
    ui: &mut Ui,
) {
    let match_result_fragment = if let Some(analysis) = r {
        let (allies_score, axis_score) = (
            analysis.state.team_scores.get_team_score(Team::Allies),
            analysis.state.team_scores.get_team_score(Team::Axis),
        );
        let allies = translate_key("#teamname_allies").unwrap_or_else(|| "Allies".to_string());
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

    let sort_id = if let Some(fi) = file_info {
        egui::Id::new(&fi.path).with("scoreboard_sort")
    } else {
        egui::Id::new("blank_scoreboard_sort")
    };

    let mut sort_state = ui.data_mut(|d| {
        d.get_temp::<ScoreboardSortState>(sort_id)
            .unwrap_or_default()
    });

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
                for (i, column) in columns.into_iter().enumerate() {
                    header.col(|ui| {
                        let col_label = if column.starts_with('#') {
                            t(column)
                        } else {
                            column.to_string()
                        };
                        let text = if sort_state.col_idx == i {
                            format!("{} {}", col_label, if sort_state.desc { "⏷" } else { "⏶" })
                        } else {
                            col_label
                        };

                        let resp = ui.add(
                            egui::Label::new(egui::RichText::new(text).strong())
                                .sense(egui::Sense::click()),
                        );

                        if resp.clicked() {
                            if sort_state.col_idx == i {
                                sort_state.desc = !sort_state.desc;
                            } else {
                                sort_state.col_idx = i;
                                sort_state.desc = true;
                            }
                            ui.data_mut(|d| d.insert_temp(sort_id, sort_state));
                        }

                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                    });
                }
            })
            .body(|ref mut body| {
                if let Some(analysis) = r {
                    struct SortablePlayer<'a> {
                        player: &'a Player,
                        steam_id_str: String,
                        team_str: String,
                        class_str: String,
                    }

                    let mut players: Vec<SortablePlayer> = analysis
                        .state
                        .players
                        .iter()
                        .map(|a| {
                            let steam_id_str = SteamId::try_from(&a.id)
                                .map(|s| s.to_string())
                                .unwrap_or_else(|_| a.id.to_string());
                            let team_str = match &a.team {
                                None => t("#app_team_unknown"),
                                Some(team) => team_name(team),
                            };
                            let class_str = a
                                .class
                                .as_ref()
                                .map(|c| format!("{c:?}"))
                                .unwrap_or_else(|| t("#app_team_unknown"));
                            SortablePlayer {
                                player: a,
                                steam_id_str,
                                team_str,
                                class_str,
                            }
                        })
                        .collect();

                    players.sort_by(|a, b| {
                        let cmp = match sort_state.col_idx {
                            0 => a.steam_id_str.cmp(&b.steam_id_str),
                            1 => a.player.name.cmp(&b.player.name),
                            2 => a.team_str.cmp(&b.team_str),
                            3 => a.class_str.cmp(&b.class_str),
                            4 => a.player.stats.0.cmp(&b.player.stats.0),
                            5 => a.player.stats.1.cmp(&b.player.stats.1),
                            6 => a.player.stats.2.cmp(&b.player.stats.2),
                            _ => std::cmp::Ordering::Equal,
                        };

                        if sort_state.desc { cmp.reverse() } else { cmp }
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
