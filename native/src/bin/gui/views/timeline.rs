use crate::FileInfo;
use crate::views::{ALLIES_COLOR, AXIS_COLOR, BRITISH_COLOR, t};
use analysis::{Analysis, Team, translate_key};
use egui::Ui;
use egui_plot::{Corner, Legend, Line, Plot, PlotPoints};
use humantime::format_duration;
use std::time::Duration;

pub fn team_score_timeline_ui(_file_info: Option<&FileInfo>, r: Option<&Analysis>, ui: &mut Ui) {
    ui.heading(t("#app_timeline_heading"));
    ui.add_space(8.0);

    ui.scope(|ui| {
        let (allies_label, allies_color, allies_team) = if let Some(analysis) = r {
            if analysis.state.allies_are_british {
                (
                    translate_key("#teamname_british").unwrap_or_else(|| "British".to_string()),
                    BRITISH_COLOR,
                    Team::British,
                )
            } else {
                (
                    translate_key("#teamname_allies").unwrap_or_else(|| "Allies".to_string()),
                    ALLIES_COLOR,
                    Team::Allies,
                )
            }
        } else {
            (
                translate_key("#teamname_allies").unwrap_or_else(|| "Allies".to_string()),
                ALLIES_COLOR,
                Team::Allies,
            )
        };
        let axis_label = translate_key("#teamname_axis").unwrap_or_else(|| "Axis".to_string());

        let plot_height = (ui.available_height() - 20.0).max(350.0);
        let plot = Plot::new("timeline_plot")
            .allow_scroll(false)
            .height(plot_height)
            .width(ui.max_rect().width())
            .legend(Legend::default().position(Corner::LeftTop))
            .custom_x_axes(vec![]) // Remove the x-axis
            .custom_y_axes(vec![]) // Remove the y-axis
            .label_formatter(move |team, point| {
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
                    analysis
                        .state
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

                let points = team_line_points(allies_team);
                let line =
                    Line::new(allies_label, PlotPoints::from_iter(points)).color(allies_color);

                plot_ui.line(line);

                let points = team_line_points(Team::Axis);
                let line = Line::new(axis_label, PlotPoints::from_iter(points)).color(AXIS_COLOR);

                plot_ui.line(line);
            }
        });
    });
}
