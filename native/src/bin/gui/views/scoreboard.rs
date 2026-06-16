use crate::views::{PlayerHighlighting, TABLE_ROW_HEIGHT, t, ALLIES_COLOR, BRITISH_COLOR, AXIS_COLOR};
use analysis::{Analysis, Player, Team, translate_key};
use egui::{Align, Layout, Ui};
use egui_extras::{Column, TableBody, TableBuilder};

fn format_time_left(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{:02}:{:02}", mins, secs)
}


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

fn get_team_color(team: &Team) -> egui::Color32 {
    match team {
        Team::Allies => ALLIES_COLOR,
        Team::British => BRITISH_COLOR,
        Team::Axis => AXIS_COLOR,
        Team::Spectators => egui::Color32::YELLOW,
        Team::Unassigned => egui::Color32::LIGHT_GRAY,
    }
}

pub fn scoreboard_ui(
    analysis: Option<&Analysis>,
    player_highlighting: &mut PlayerHighlighting,
    scoreboard_cache: &crate::ScoreboardCache,
    ui: &mut Ui,
) {
    let total_width = ui.available_width();
    let desired_width = (total_width * 0.5).max(750.0).min(total_width);
    let padding = (total_width - desired_width) / 2.0;

    ui.horizontal(|ui| {
        if padding > 0.0 {
            ui.add_space(padding);
        }
        ui.vertical(|ui| {
            ui.set_max_width(desired_width);

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

            if let Some(analysis) = analysis {
                if analysis.state.started_late || analysis.state.ended_early {
                    let banner_text = if analysis.state.started_late && analysis.state.ended_early {
                        let first_str = analysis.state.first_time_left.map(format_time_left).unwrap_or_else(|| "??:??".to_string());
                        let last_str = analysis.state.last_time_left.map(format_time_left).unwrap_or_else(|| "??:??".to_string());
                        t("#app_partial_demo_both")
                            .replace("%s1", &first_str)
                            .replace("%s2", &last_str)
                    } else if analysis.state.started_late {
                        let first_str = analysis.state.first_time_left.map(format_time_left).unwrap_or_else(|| "??:??".to_string());
                        t("#app_partial_demo_start")
                            .replace("%s1", &first_str)
                    } else {
                        let last_str = analysis.state.last_time_left.map(format_time_left).unwrap_or_else(|| "??:??".to_string());
                        t("#app_partial_demo_end")
                            .replace("%s2", &last_str)
                    };

                    let banner_color = egui::Color32::from_rgb(251, 191, 36); // Amber-400
                    let bg_color = egui::Color32::from_rgba_unmultiplied(251, 191, 36, 15); // Amber-400 with opacity
                    egui::Frame::NONE
                        .fill(bg_color)
                        .stroke(egui::Stroke::new(1.0, banner_color))
                        .corner_radius(4.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.colored_label(banner_color, banner_text);
                            });
                        });
                    ui.add_space(8.0);
                }
            }

            ui.scope(|ui| {
                let columns = [
                    "#app_col_name",
                    "#app_col_class",
                    "#app_col_score",
                    "#app_col_kills",
                    "#app_col_deaths",
                ];

                let name_w = 180.0;
                let score_w = 140.0;
                let kills_w = 140.0;
                let deaths_w = 140.0;
                let class_w = (desired_width - name_w - score_w - kills_w - deaths_w).max(120.0);
                let total_table_width = name_w + class_w + score_w + kills_w + deaths_w + 4.0 * ui.spacing().item_spacing.x;

                let parent_clip_rect = ui.clip_rect();
                let table_painter = ui.painter().clone();
                let table = TableBuilder::new(ui)
                    .striped(false)
                    .vscroll(false)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .column(Column::exact(name_w))
                    .column(Column::remainder())
                    .column(Column::exact(score_w))
                    .column(Column::exact(kills_w))
                    .column(Column::exact(deaths_w));


                 table
                     .header(TABLE_ROW_HEIGHT, |mut header| {
                         for (idx, column) in columns.iter().enumerate() {
                             header.col(|ui| {
                                 let col_label = if column.starts_with('#') {
                                     t(column)
                                 } else {
                                     column.to_string()
                                 };
                                 if idx >= 2 {
                                     ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                         ui.strong(col_label);
                                     });
                                 } else {
                                     ui.strong(col_label);
                                 }
                             });
                         }
                     })
                    .body(|ref mut body| {
                        // Spacer row between column headers and the first team row
                        body.row(12.0, |mut row| {
                            for _ in 0..5 {
                                row.col(|_ui| {});
                            }
                        });

                        if let Some(analysis) = analysis {
                            let is_british = analysis.state.allies_are_british;
                            let allies_team = if is_british { Team::British } else { Team::Allies };

                            struct TeamGroup {
                                team: Team,
                                title: String,
                                player_indices: Vec<usize>,
                                total_score: i32,
                                total_kills: i32,
                                total_deaths: i32,
                            }

                            let allies_group = TeamGroup {
                                team: allies_team.clone(),
                                title: team_name(&allies_team),
                                player_indices: scoreboard_cache.allies_players.clone(),
                                total_score: scoreboard_cache.allies_totals.0,
                                total_kills: scoreboard_cache.allies_totals.1,
                                total_deaths: scoreboard_cache.allies_totals.2,
                            };
                            let axis_group = TeamGroup {
                                team: Team::Axis,
                                title: team_name(&Team::Axis),
                                player_indices: scoreboard_cache.axis_players.clone(),
                                total_score: scoreboard_cache.axis_totals.0,
                                total_kills: scoreboard_cache.axis_totals.1,
                                total_deaths: scoreboard_cache.axis_totals.2,
                            };
                            let spec_group = TeamGroup {
                                team: Team::Spectators,
                                title: team_name(&Team::Spectators),
                                player_indices: scoreboard_cache.spec_players.clone(),
                                total_score: scoreboard_cache.spec_totals.0,
                                total_kills: scoreboard_cache.spec_totals.1,
                                total_deaths: scoreboard_cache.spec_totals.2,
                            };
                            let unassigned_group = TeamGroup {
                                team: Team::Unassigned,
                                title: team_name(&Team::Unassigned),
                                player_indices: scoreboard_cache.unassigned_players.clone(),
                                total_score: scoreboard_cache.unassigned_totals.0,
                                total_kills: scoreboard_cache.unassigned_totals.1,
                                total_deaths: scoreboard_cache.unassigned_totals.2,
                            };

                            let mut groups = vec![allies_group, axis_group];
                            if !spec_group.player_indices.is_empty() {
                                groups.push(spec_group);
                            }
                            if !unassigned_group.player_indices.is_empty() {
                                groups.push(unassigned_group);
                            }

                            let mut first = true;
                            for group in groups {
                                if !first {
                                    // Visual spacer row of height 12.0
                                    body.row(12.0, |mut row| {
                                        for _ in 0..5 {
                                            row.col(|_ui| {});
                                        }
                                    });
                                }
                                first = false;

                                let count = group.player_indices.len();
                                let label_text = format!(
                                    "{}  -  {} {}",
                                    group.title,
                                    count,
                                    if count == 1 { "player" } else { "players" }
                                );
                                let team_color = get_team_color(&group.team);

                                 // Render team header row
                                body.row(24.0, |mut row| {
                                    row.col(|ui| {
                                        ui.vertical(|ui| {
                                            ui.strong(egui::RichText::new(&label_text).color(team_color));
                                            ui.add_space(2.0);
                                            
                                            // Draw a single continuous line across the entire table width
                                            let rect = ui.max_rect();
                                            let line_y = rect.max.y - 1.0;
                                            let line_rect = egui::Rect::from_min_max(
                                                egui::pos2(rect.min.x, line_y),
                                                egui::pos2(rect.min.x + total_table_width, line_y + 1.0),
                                            );
                                            table_painter.rect_filled(line_rect, 0.0, team_color);
                                        });
                                    });
                                    row.col(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label("");
                                            ui.add_space(2.0);
                                        });
                                    });
                                    row.col(|ui| {
                                        ui.vertical(|ui| {
                                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                ui.strong(egui::RichText::new(group.total_score.to_string()).color(team_color));
                                            });
                                            ui.add_space(2.0);
                                        });
                                    });
                                    row.col(|ui| {
                                        ui.vertical(|ui| {
                                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                ui.strong(egui::RichText::new(group.total_kills.to_string()).color(team_color));
                                            });
                                            ui.add_space(2.0);
                                        });
                                    });
                                    row.col(|ui| {
                                        ui.vertical(|ui| {
                                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                ui.strong(egui::RichText::new(group.total_deaths.to_string()).color(team_color));
                                            });
                                            ui.add_space(2.0);
                                        });
                                    });
                                });

                                // Render player rows
                                for p_idx in group.player_indices {
                                    let p = &analysis.state.players[p_idx];
                                    scoreboard_row_ui(analysis, p, player_highlighting, &group.team, total_table_width, table_painter.clone(), parent_clip_rect, body);
                                }
                            }
                        }
                    });
            });
        });
    });
}

pub fn scoreboard_row_ui(
    analysis: &Analysis,
    p: &Player,
    player_highlighting: &mut PlayerHighlighting,
    team: &Team,
    total_table_width: f32,
    table_painter: egui::Painter,
    parent_clip_rect: egui::Rect,
    body: &mut TableBody,
) {
    let is_pov_demo = analysis.demo_info.demo_type == "POV";
    let is_recorder = is_pov_demo && {
        if let Some(pov_idx) = analysis.state.pov_player_index {
            let conn_matches = match p.connection {
                analysis::Connection::Connected { client_id } => client_id == pov_idx,
                _ => false,
            };
            if conn_matches {
                true
            } else if let Some(rec) = analysis.state.players.iter().find(|pl| match pl.connection {
                analysis::Connection::Connected { client_id } => client_id == pov_idx,
                _ => false,
            }) {
                p.id == rec.id || p.name == rec.name
            } else {
                false
            }
        } else {
            false
        }
    };

    let is_selected = player_highlighting.highlighted.contains(&p.id);
    let team_color = if is_selected {
        egui::Color32::WHITE
    } else {
        get_team_color(team)
    };
    let cell_label = |ui: &mut Ui, text: &str, hovered: bool| {
        ui.add(egui::Label::new(egui::RichText::new(text).color(team_color)).sense(egui::Sense::empty()));
        if hovered {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    };

    let mut clicked = false;
    let mut row_hovered = false;

    body.row(TABLE_ROW_HEIGHT, |mut row| {
        row.set_selected(false);

        // Column 1: Name
        row.col(|ui| {
            let rect = ui.max_rect();
            let row_y = rect.y_range();
            let table_x_min = rect.min.x;
            let table_x_max = table_x_min + total_table_width;

            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                if parent_clip_rect.contains(pos) && row_y.contains(pos.y) && pos.x >= table_x_min && pos.x <= table_x_max {
                    row_hovered = true;
                    if ui.input(|i| i.pointer.primary_clicked()) {
                        clicked = true;
                    }
                }
            }

            let row_rect = egui::Rect::from_min_max(
                rect.min,
                egui::pos2(rect.min.x + total_table_width, rect.max.y),
            );

            // Handle selection and hover drawing
            if is_selected {
                table_painter.rect_filled(row_rect, 0.0, ui.visuals().selection.bg_fill);
            } else if row_hovered {
                table_painter.rect_filled(row_rect, 0.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 15));
            }

            if row_hovered {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }

            ui.horizontal(|ui| {
                cell_label(ui, &p.name, row_hovered);
                if is_recorder {
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new("🎥"));
                }
                if p.has_reconnected {
                    ui.add_space(2.0);
                    ui.colored_label(egui::Color32::from_rgb(251, 191, 36), "🔄")
                        .on_hover_text(t("#app_player_reconnected_desc"));
                }
                if p.has_pre_demo_activity {
                    ui.add_space(2.0);
                    ui.colored_label(egui::Color32::from_rgb(251, 191, 36), "*")
                        .on_hover_text(t("#app_player_pre_demo_desc"));
                }
            });
        });

        // Column 2: Class
        row.col(|ui| {
            let class_str = match &p.class {
                None => t("#app_team_unknown"),
                Some(x) => format!("{x:?}"),
            };
            cell_label(ui, &class_str, row_hovered);
        });

        // Column 3: Score
        row.col(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                cell_label(ui, &p.stats.0.to_string(), row_hovered);
            });
        });

        // Column 4: Kills
        row.col(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                cell_label(ui, &p.stats.1.to_string(), row_hovered);
            });
        });

        // Column 5: Deaths
        row.col(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                cell_label(ui, &p.stats.2.to_string(), row_hovered);
            });
        });
    });

    if clicked {
        if player_highlighting.highlighted.contains(&p.id) {
            player_highlighting.highlighted.remove(&p.id);
        } else {
            player_highlighting.highlighted.clear();
            player_highlighting.highlighted.insert(p.id.clone());
        }
    }
}
