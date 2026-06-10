use crate::FileInfo;
use analysis::Analysis;
use egui::{Grid, Ui};

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

        ui.strong("Demo type");
        if let Some(a) = analysis {
            ui.label(&a.demo_info.demo_type);
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
