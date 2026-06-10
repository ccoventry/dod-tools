use crate::FileInfo;
use crate::views::{PlayerHighlighting, TABLE_ROW_HEIGHT, t};
use analysis::{Analysis, Player};
use egui::{Align, CollapsingHeader, Layout, ScrollArea, Ui};
use egui_extras::{Column, TableBuilder};
use humantime::format_duration;
use std::time::Duration;

pub fn kill_streaks_ui(
    _file_info: Option<&FileInfo>,
    r: Option<&Analysis>,
    player_highlighting: &PlayerHighlighting,
    ui: &mut Ui,
) {
    ui.heading(t("#app_streaks_heading"));
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
                ui.strong(t("#app_col_wave"));
            });
            row.col(|ui| {
                ui.strong(t("#app_col_total_kills"));
            });
            row.col(|ui| {
                ui.strong(t("#app_col_start_time"));
            });
            row.col(|ui| {
                ui.strong(t("#app_col_duration"));
            });
            row.col(|ui| {
                ui.strong(t("#app_col_weapons_used"));
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
