use crate::FileInfo;
use analysis::{Analysis, Player, PlayerGlobalId, Round, SteamId, Team, MortalityState};
use egui::{Align, Color32, CollapsingHeader, Grid, Label, Layout, ScrollArea, Ui};
use egui_extras::{Column, TableBody, TableBuilder};
use egui_plot::{Corner, Legend, Line, Plot, PlotPoints};
use humantime::format_duration;
use std::time::Duration;

pub const TABLE_ROW_HEIGHT: f32 = 18.;
pub const ALLIES_COLOR: Color32 = Color32::DARK_GREEN;
pub const AXIS_COLOR: Color32 = Color32::DARK_RED;
pub const NEUTRAL_COLOR: Color32 = Color32::WHITE;

#[derive(Default)]
pub struct PlayerHighlighting {
    pub highlighted: std::collections::HashSet<PlayerGlobalId>,
}

pub fn report_ui(
    file_info: Option<&FileInfo>,
    r: Option<&Analysis>,
    player_highlighting: &mut PlayerHighlighting,
    ui: &mut Ui,
) {
    let tab_id = if let Some(fi) = file_info {
        egui::Id::new(&fi.path).with("active_tab")
    } else {
        egui::Id::new("blank_report").with("active_tab")
    };
    let mut current_tab = ui.data(|d| d.get_temp::<String>(tab_id).unwrap_or_else(|| "Summary".to_string()));

    ui.horizontal(|ui| {
        ui.selectable_value(&mut current_tab, "Summary".to_string(), "Summary");
        ui.selectable_value(&mut current_tab, "Scoreboard".to_string(), "Scoreboard");
        ui.selectable_value(&mut current_tab, "Timeline".to_string(), "Team score timeline");
        ui.selectable_value(&mut current_tab, "Rounds".to_string(), "Rounds");
        ui.selectable_value(&mut current_tab, "Weapon Breakdowns".to_string(), "Weapon breakdowns");
        ui.selectable_value(&mut current_tab, "Kill Streaks".to_string(), "Kill streaks");
    });
    
    ui.separator();
    
    ui.data_mut(|d| d.insert_temp(tab_id, current_tab.clone()));

    match current_tab.as_str() {
        "Summary" => header_ui(file_info, r, ui),
        "Scoreboard" => scoreboard_ui(file_info, r, player_highlighting, ui),
        "Timeline" => team_score_timeline_ui(file_info, r, ui),
        "Rounds" => rounds_ui(file_info, r, ui),
        "Weapon Breakdowns" => weapon_breakdowns_ui(file_info, r, player_highlighting, ui),
        "Kill Streaks" => kill_streaks_ui(file_info, r, player_highlighting, ui),
        _ => {}
    }
}

pub fn header_ui(file_info: Option<&FileInfo>, analysis: Option<&Analysis>, ui: &mut Ui) {
    Grid::new("header").show(ui, |ui| {
        ui.strong("File path");
        ui.monospace(file_info.map(|fi| fi.path.as_str()).unwrap_or(""));
        ui.end_row();

        ui.strong("File size");
        if let Some(fi) = file_info {
            let size_mb = fi.size_bytes as f64 / 1_048_576.0;
            ui.label(format!("{:.2} MB", size_mb));
        } else {
            ui.label("");
        }
        ui.end_row();

        ui.strong("File created at");
        if let Some(fi) = file_info {
            let formatted_date = chrono::DateTime::<chrono::Local>::from(fi.created_at)
                .format("%Y-%m-%d %I:%M %p")
                .to_string();
            ui.label(formatted_date);
        } else {
            ui.label("");
        }
        ui.end_row();

        ui.strong("Game mod");
        if let Some(a) = analysis {
            let game_dir = &a.demo_info.game_directory;
            let game_str = match game_dir.as_str() {
                "dod" => "Day of Defeat (dod)",
                "cstrike" => "Counter-Strike (cstrike)",
                "valve" => "Half-Life (valve)",
                other => other,
            };
            ui.label(game_str);
        } else {
            ui.label("");
        }
        ui.end_row();

        ui.strong("Map name");
        ui.label(analysis.map(|a| a.demo_info.map_name.as_str()).unwrap_or(""));
        ui.end_row();

        ui.strong("Demo protocol");
        ui.label(analysis.map(|a| a.demo_info.demo_protocol.to_string()).unwrap_or_else(String::new));
        ui.end_row();

        ui.strong("Network protocol");
        ui.label(analysis.map(|a| a.demo_info.network_protocol.to_string()).unwrap_or_else(String::new));
        ui.end_row();

        ui.strong("Game version");
        if let Some(a) = analysis {
            let version_str = match (a.demo_info.game_directory.as_str(), a.demo_info.network_protocol) {
                ("dod", 48) => "v1.3 (Steam release)",
                ("dod", 47) => "v1.0 - v1.2 (WON release)",
                ("cstrike", 48) => "v1.6 (Steam release)",
                ("cstrike", 47) => "v1.5 or earlier (WON release)",
                ("valve", 48) => "v1.1.2.0+ (Steam release)",
                ("valve", 47) => "v1.1.1.0 or earlier (WON release)",
                (_, 48) => "Steam release (Protocol 48)",
                (_, 47) => "WON release (Protocol 47)",
                _ => "Legacy release",
            };
            ui.label(version_str);
        } else {
            ui.label("");
        }
        ui.end_row();

        ui.strong("Analyzer version");
        ui.label(env!("CARGO_PKG_VERSION"));
        ui.end_row();
    });
}

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

pub fn team_score_timeline_ui(_file_info: Option<&FileInfo>, r: Option<&Analysis>, ui: &mut Ui) {
    ui.heading("Team Score Timeline");
    ui.add_space(8.0);
    
    ui.scope(|ui| {
            let plot = Plot::new("timeline_plot")
                .allow_scroll(false)
                .height(200.)
                .width(ui.max_rect().width())
                .legend(Legend::default().position(Corner::LeftTop))
                .custom_x_axes(vec![]) // Remove the x-axis
                .custom_y_axes(vec![]) // Remove the y-axis
                .label_formatter(|team, point| {
                    if !team.is_empty() {
                        let duration = Duration::from_secs_f64(point.x);
                        let duration = Duration::new(duration.as_secs(), 0);

                        format!("{}\n{}: {}", format_duration(duration), team, point.y)
                    } else {
                        String::default()
                    }
                });

            plot.show(ui, |plot_ui| {
                if let Some(analysis) = r {
                    let team_line_points = |team: Team| {
                        analysis.state
                            .team_scores
                            .iter()
                            .filter_map(move |(time, t, score)| {
                                if *t == team {
                                    Some([time.viewdemo_offset.as_secs_f64(), *score as f64])
                                } else {
                                    None
                                }
                            })
                    };

                    let points = team_line_points(Team::Allies);
                    let line = Line::new("Allies", PlotPoints::from_iter(points)).color(ALLIES_COLOR);

                    plot_ui.line(line);

                    let points = team_line_points(Team::Axis);
                    let line = Line::new("Axis", PlotPoints::from_iter(points)).color(AXIS_COLOR);

                    plot_ui.line(line);
                }
            });
    });
}

pub fn rounds_ui(_file_info: Option<&FileInfo>, r: Option<&Analysis>, ui: &mut Ui) {
    ui.heading("Rounds");
    ui.add_space(8.0);
    
    ui.scope(|ui| {
            let table = TableBuilder::new(ui)
                .striped(true)
                .cell_layout(Layout::left_to_right(Align::Center))
                .columns(Column::auto(), 6);

            table
                .header(TABLE_ROW_HEIGHT, |mut ui| {
                    ui.col(|ui| {
                        ui.add_space(ui.style().spacing.indent);
                    });
                    ui.col(|ui| {
                        ui.strong("#");
                    });
                    ui.col(|ui| {
                        ui.strong("Start Time");
                    });
                    ui.col(|ui| {
                        ui.strong("Duration");
                    });
                    ui.col(|ui| {
                        ui.strong("Winner");
                    });
                    ui.col(|ui| {
                        ui.strong("Kills by Winner");
                    });
                })
                .body(|mut ui| {
                    let mut match_duration = Duration::default();

                    if let Some(analysis) = r {
                        for (i, round) in analysis.state.rounds.iter().enumerate() {
                            if let Round::Completed {
                                start_time,
                                end_time,
                                winner_stats,
                            } = round
                            {
                                match_duration += end_time - start_time;

                                ui.row(TABLE_ROW_HEIGHT, |mut row| {
                                    row.col(|ui| {
                                        ui.painter().rect_filled(
                                            ui.max_rect(),
                                            0.0,
                                            match winner_stats {
                                                Some((Team::Allies, _)) => ALLIES_COLOR,
                                                Some((Team::Axis, _)) => AXIS_COLOR,
                                                _ => NEUTRAL_COLOR,
                                            },
                                        );
                                    });

                                    row.col(|ui| {
                                        ui.label((i + 1).to_string());
                                    });

                                    row.col(|ui| {
                                        let start_time = Duration::from_millis(
                                            start_time.viewdemo_offset.as_millis() as u64,
                                        );

                                        ui.label(format_duration(start_time).to_string());
                                    });

                                    row.col(|ui| {
                                        let duration = Duration::from_millis(
                                            (end_time - start_time).as_millis() as u64,
                                        );

                                        ui.label(format_duration(duration).to_string());
                                    });

                                    if let Some((winner, kills)) = winner_stats {
                                        row.col(|ui| {
                                            ui.label(if matches!(winner, Team::Allies) {
                                                "Allies"
                                            } else {
                                                "Axis"
                                            });
                                        });

                                        row.col(|ui| {
                                            ui.label(kills.to_string());
                                        });
                                    } else {
                                        row.col(|_ui| {});
                                        row.col(|_ui| {});
                                    }
                                });
                            }
                        }
                    }

                    if r.is_some() {
                        ui.row(TABLE_ROW_HEIGHT, |mut row| {
                            row.col(|_| {});
                            row.col(|_| {});
                            row.col(|ui| {
                                ui.label(format_duration(match_duration).to_string());
                            });
                            row.col(|_| {});
                        });
                    }
                });
    });
}

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

pub fn kill_streaks_ui(
    _file_info: Option<&FileInfo>,
    r: Option<&Analysis>,
    player_highlighting: &PlayerHighlighting,
    ui: &mut Ui,
) {
    ui.heading("Kill Streaks");
    ui.add_space(8.0);
    
    ui.scope(|ui| {
            if let Some(analysis) = r {
                let mut players = Vec::from_iter(&analysis.state.players);
                players.sort_by(|l, r| l.name.cmp(&r.name));

                ScrollArea::vertical()
                    .id_salt("player_kill_streaks_scroll")
                    .auto_shrink(false)
                    .min_scrolled_height(260.)
                    .show(ui, |ui| {
                        for p in players {
                            if !player_highlighting.highlighted.is_empty()
                                && !player_highlighting.highlighted.contains(&p.id)
                            {
                                continue;
                            }

                            if p.kill_streaks.is_empty() {
                                continue;
                            }

                            CollapsingHeader::new(&p.name)
                                .default_open(false)
                                .show(ui, |ui| {
                                    kill_streaks_table_ui(p, ui);
                                });
                        }
                    });
            }
    });
}

pub fn kill_streaks_table_ui(p: &Player, ui: &mut Ui) {
    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .columns(Column::auto(), 5)
        .header(TABLE_ROW_HEIGHT, |mut row| {
            row.col(|ui| {
                ui.strong("Wave");
            });
            row.col(|ui| {
                ui.strong("Total Kills");
            });
            row.col(|ui| {
                ui.strong("Start Time");
            });
            row.col(|ui| {
                ui.strong("Duration");
            });
            row.col(|ui| {
                ui.strong("Weapons Used");
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
                            let weapons = streak
                                .kills
                                .iter()
                                .map(|(_, weapon)| format!("{weapon:?}"))
                                .collect::<Vec<_>>()
                                .join(", ");

                            ui.label(weapons);
                        });
                    });
                }
            }
        });
}
