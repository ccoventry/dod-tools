use crate::FileInfo;
use crate::views::{ALLIES_COLOR, AXIS_COLOR, NEUTRAL_COLOR, TABLE_ROW_HEIGHT};
use analysis::{Analysis, Round, Team};
use egui::{Align, Layout, Ui};
use egui_extras::{Column, TableBuilder};
use humantime::format_duration;
use std::time::Duration;

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
