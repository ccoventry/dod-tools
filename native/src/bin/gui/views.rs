use crate::FileInfo;
use analysis::Analysis;
use egui::{Color32, Ui};
use std::collections::HashSet;
use analysis::PlayerGlobalId;

pub const TABLE_ROW_HEIGHT: f32 = 18.;
pub const ALLIES_COLOR: Color32 = Color32::DARK_GREEN;
pub const AXIS_COLOR: Color32 = Color32::DARK_RED;
pub const NEUTRAL_COLOR: Color32 = Color32::WHITE;

#[derive(Default)]
pub struct PlayerHighlighting {
    pub highlighted: HashSet<PlayerGlobalId>,
}

pub mod summary;
pub mod scoreboard;
pub mod timeline;
pub mod rounds;
pub mod weapons;
pub mod streaks;
pub mod chat;

pub use summary::header_ui;
pub use scoreboard::scoreboard_ui;
pub use timeline::team_score_timeline_ui;
pub use rounds::rounds_ui;
pub use weapons::weapon_breakdowns_ui;
pub use streaks::kill_streaks_ui;
pub use chat::chat_log_ui;

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
        ui.selectable_value(&mut current_tab, "Chat Log".to_string(), "Chat log");
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
        "Chat Log" => chat_log_ui(file_info, r, ui),
        _ => {}
    }
}
