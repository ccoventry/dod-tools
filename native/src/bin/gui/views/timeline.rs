use crate::FileInfo;
use crate::views::{ALLIES_COLOR, AXIS_COLOR};
use analysis::{Analysis, Team};
use egui::Ui;
use egui_plot::{Corner, Legend, Line, Plot, PlotPoints};
use humantime::format_duration;
use std::time::Duration;

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
