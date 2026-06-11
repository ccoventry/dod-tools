use crate::FileInfo;
use crate::views::t;
use analysis::Analysis;
use egui::{Grid, Ui};
use humantime::format_duration;

pub fn header_ui(file_info: Option<&FileInfo>, analysis: Option<&Analysis>, ui: &mut Ui) {
    Grid::new("header").show(ui, |ui| {
        ui.strong(t("#app_summary_file_path"));
        ui.monospace(file_info.map(|fi| fi.path.as_str()).unwrap_or(""));
        ui.end_row();

        ui.strong(t("#app_summary_file_size"));
        if let Some(fi) = file_info {
            let size_mb = fi.size_bytes as f64 / 1_048_576.0;
            ui.label(format!("{:.2} MB", size_mb));
        } else {
            ui.label("");
        }
        ui.end_row();

        ui.strong(t("#app_summary_file_created"));
        if let Some(fi) = file_info {
            let formatted_date = chrono::DateTime::<chrono::Local>::from(fi.created_at)
                .format("%Y-%m-%d %I:%M %p")
                .to_string();
            ui.label(formatted_date);
        } else {
            ui.label("");
        }
        ui.end_row();

        ui.strong(t("#app_summary_demo_duration"));
        if let Some(a) = analysis {
            let total_dur = std::time::Duration::from_secs_f32(a.demo_info.playback_time);
            ui.label(format_duration(std::time::Duration::from_secs(total_dur.as_secs())).to_string());
        } else {
            ui.label("");
        }
        ui.end_row();

        ui.strong(t("#app_summary_match_duration"));
        if let Some(a) = analysis {
            let first_round_start = a.state.rounds.first().map(|r| match r {
                analysis::Round::Active { start_time, .. } => start_time.viewdemo_offset,
                analysis::Round::Completed { start_time, .. } => start_time.viewdemo_offset,
            }).unwrap_or(std::time::Duration::ZERO);

            let last_round_end = a.state.rounds.last().map(|r| match r {
                analysis::Round::Active { .. } => a.state.current_time.viewdemo_offset,
                analysis::Round::Completed { end_time, .. } => end_time.viewdemo_offset,
            }).unwrap_or(std::time::Duration::ZERO);

            let match_dur = last_round_end.checked_sub(first_round_start).unwrap_or(std::time::Duration::ZERO);
            ui.label(format_duration(std::time::Duration::from_secs(match_dur.as_secs())).to_string());
        } else {
            ui.label("");
        }
        ui.end_row();

        ui.strong(t("#app_summary_demo_type"));
        if let Some(a) = analysis {
            ui.label(&a.demo_info.demo_type);
        } else {
            ui.label("");
        }
        ui.end_row();

        ui.strong(t("#app_summary_game_mod"));
        if let Some(a) = analysis {
            let game_dir = &a.demo_info.game_directory;
            let game_str = match game_dir.as_str() {
                "dod" => t("#app_game_dod"),
                "cstrike" => t("#app_game_cstrike"),
                "valve" => t("#app_game_valve"),
                other => other.to_string(),
            };
            ui.label(game_str);
        } else {
            ui.label("");
        }
        ui.end_row();

        ui.strong(t("#app_summary_map_name"));
        ui.label(
            analysis
                .map(|a| a.demo_info.map_name.as_str())
                .unwrap_or(""),
        );
        ui.end_row();

        ui.strong(t("#app_summary_demo_protocol"));
        ui.label(
            analysis
                .map(|a| a.demo_info.demo_protocol.to_string())
                .unwrap_or_else(String::new),
        );
        ui.end_row();

        ui.strong(t("#app_summary_net_protocol"));
        ui.label(
            analysis
                .map(|a| a.demo_info.network_protocol.to_string())
                .unwrap_or_else(String::new),
        );
        ui.end_row();

        ui.strong(t("#app_summary_game_version"));
        if let Some(a) = analysis {
            let version_str = match (
                a.demo_info.game_directory.as_str(),
                a.demo_info.network_protocol,
            ) {
                ("dod", 48) => t("#app_ver_dod_13"),
                ("dod", 47) => t("#app_ver_dod_10_12"),
                ("cstrike", 48) => t("#app_ver_cs_16"),
                ("cstrike", 47) => t("#app_ver_cs_15"),
                ("valve", 48) => t("#app_ver_hl_steam"),
                ("valve", 47) => t("#app_ver_hl_won"),
                (_, 48) => t("#app_ver_steam_48"),
                (_, 47) => t("#app_ver_won_47"),
                _ => t("#app_ver_legacy"),
            };
            ui.label(version_str);
        } else {
            ui.label("");
        }
        ui.end_row();
    });
}
