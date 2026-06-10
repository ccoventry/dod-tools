use crate::FileInfo;
use crate::views::{ALLIES_COLOR, AXIS_COLOR, NEUTRAL_COLOR, TABLE_ROW_HEIGHT, t};
use analysis::{Analysis, Round, Team, translate_key};
use egui::{Align, Layout, Ui};
use egui_extras::{Column, TableBuilder};
use humantime::format_duration;
use std::time::Duration;

pub fn rounds_ui(_file_info: Option<&FileInfo>, r: Option<&Analysis>, ui: &mut Ui) {
    ui.heading(t("#app_rounds_heading"));
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
                        ui.strong(t("#app_col_round_num"));
                    });
                    ui.col(|ui| {
                        ui.strong(t("#app_col_start_time"));
                    });
                    ui.col(|ui| {
                        ui.strong(t("#app_col_duration"));
                    });
                    ui.col(|ui| {
                        ui.strong(t("#app_col_winner"));
                    });
                    ui.col(|ui| {
                        ui.strong(t("#app_col_winner_kills"));
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
                                            let name = match winner {
                                                Team::Allies => translate_key("#teamname_allies")
                                                    .unwrap_or_else(|| "Allies".to_string()),
                                                Team::Axis => translate_key("#teamname_axis")
                                                    .unwrap_or_else(|| "Axis".to_string()),
                                                Team::Spectators => translate_key("#teamname_spectators")
                                                    .unwrap_or_else(|| "Spectators".to_string()),
                                                Team::Unassigned => t("#app_team_unassigned"),
                                            };
                                            ui.label(name);
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
