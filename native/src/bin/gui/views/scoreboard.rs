use crate::FileInfo;
use crate::views::PlayerHighlighting;
use crate::views::TABLE_ROW_HEIGHT;
use analysis::{Analysis, Player, SteamId, Team, MortalityState};
use egui::{Align, Label, Layout, Ui};
use egui_extras::{Column, TableBody, TableBuilder};

#[derive(Clone, Copy)]
pub struct ScoreboardSortState {
    pub col_idx: usize,
    pub desc: bool,
}

impl Default for ScoreboardSortState {
    fn default() -> Self {
        Self { col_idx: 5, desc: true } // Score descending
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
        format!(
            ": Allies ({}) {} Axis ({})",
            allies_score,
            if allies_score > axis_score { ">" } else { "<" },
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

    ui.heading(format!("Scoreboard{match_result_fragment}"));
    ui.add_space(8.0);
    
    ui.scope(|ui| {
            let columns = [
                "",
                "ID",
                "Name",
                "Team",
                "Class",
                "Score",
                "Kills",
                "Deaths",
                "Avg. Life",
                "Min. Life",
                "Max. Life",
            ];

            let table = TableBuilder::new(ui)
                .striped(true)
                .cell_layout(Layout::left_to_right(Align::Center))
                .max_scroll_height(260.)
                .column(Column::auto())
                .column(Column::auto_with_initial_suggestion(150.))
                .columns(Column::auto(), columns.len());

            table
                .header(TABLE_ROW_HEIGHT, |mut header| {
                    for (i, column) in columns.into_iter().enumerate() {
                        header.col(|ui| {
                            if i == 0 {
                                ui.strong(column);
                            } else {
                                let text = if sort_state.col_idx == i {
                                    format!("{} {}", column, if sort_state.desc { "⏷" } else { "⏶" })
                                } else {
                                    column.to_string()
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
                            }
                        });
                    }
                })
                .body(|ref mut body| {
                    if let Some(analysis) = r {
                        struct SortablePlayer<'a> {
                            player: &'a Player,
                            steam_id_str: String,
                            team_str: &'static str,
                            class_str: String,
                        }

                        let mut players: Vec<SortablePlayer> = analysis.state.players
                            .iter()
                            .map(|a| {
                                let steam_id_str = SteamId::try_from(&a.id)
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|_| a.id.to_string());
                                let team_str = match &a.team {
                                    None => "Unknown",
                                    Some(Team::Allies) => "Allies",
                                    Some(Team::Axis) => "Axis",
                                    Some(Team::Spectators) => "Spectators",
                                    Some(Team::Unassigned) => "Unassigned",
                                };
                                let class_str = a.class.as_ref()
                                    .map(|c| format!("{c:?}"))
                                    .unwrap_or_else(|| "Unknown".to_string());
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
                                1 => a.steam_id_str.cmp(&b.steam_id_str),
                                2 => a.player.name.cmp(&b.player.name),
                                3 => a.team_str.cmp(b.team_str),
                                4 => a.class_str.cmp(&b.class_str),
                                5 => a.player.stats.0.cmp(&b.player.stats.0),
                                6 => a.player.stats.1.cmp(&b.player.stats.1),
                                7 => a.player.stats.2.cmp(&b.player.stats.2),
                                8 => a.player.avg_lifespan().cmp(&b.player.avg_lifespan()),
                                9 => a.player.min_lifespan().cmp(&b.player.min_lifespan()),
                                10 => a.player.max_lifespan().cmp(&b.player.max_lifespan()),
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
    let row_label = |ui: &mut Ui, str: &str| {
        ui.add(Label::new(str).extend());
    };

    body.row(TABLE_ROW_HEIGHT, |mut row| {
        let mut is_checked = player_highlighting.highlighted.contains(&p.id);

        row.set_selected(is_checked);

        row.col(|ui| {
            if ui.checkbox(&mut is_checked, "").changed() {
                if is_checked {
                    player_highlighting.highlighted.insert(p.id.clone());
                } else {
                    player_highlighting.highlighted.remove(&p.id);
                }
            }
        });

        row.col(|ui| match SteamId::try_from(&p.id) {
            Ok(steam_id) => {
                let link_text = steam_id.to_string();
                let link_url = format!("https://steamcommunity.com/profiles/{}", p.id);

                ui.hyperlink_to(link_text, link_url);
            }
            _ => {
                ui.label(p.id.to_string());
            }
        });

        row.col(|ui| {
            row_label(ui, &p.name);
        });

        row.col(|ui| {
            ui.label(match &p.team {
                None => "Unknown",
                Some(Team::Allies) => "Allies",
                Some(Team::Axis) => "Axis",
                Some(Team::Spectators) => "Spectators",
                Some(Team::Unassigned) => "Unassigned",
            });
        });

        row.col(|ui| {
            ui.label(match &p.class {
                None => "Unknown".to_string(),
                Some(x) => format!("{x:?}"),
            });
        });

        row.col(|ui| {
            ui.label(p.stats.0.to_string());
        });

        row.col(|ui| {
            ui.label(p.stats.1.to_string());
        });

        row.col(|ui| {
            ui.label(p.stats.2.to_string());
        });

        row.col(|ui| {
            ui.label(format!("{}s", p.avg_lifespan().as_secs()));
        });

        row.col(|ui| {
            ui.label(format!("{}s", p.min_lifespan().as_secs()));
        });

        row.col(|ui| {
            ui.label(format!("{}s", p.max_lifespan().as_secs()));
        });
    });
}
