use crate::FileInfo;
use analysis::Analysis;
use analysis::PlayerGlobalId;
use egui::{Color32, Ui};
use std::collections::HashSet;

pub const TABLE_ROW_HEIGHT: f32 = 18.;
pub const ALLIES_COLOR: Color32 = Color32::DARK_GREEN;
pub const AXIS_COLOR: Color32 = Color32::DARK_RED;
pub const NEUTRAL_COLOR: Color32 = Color32::WHITE;

/// Look up an app UI string by localization key.
/// Falls back to a readable version of the key if no translation is found.
pub fn t(key: &str) -> String {
    analysis::translate_key(key).unwrap_or_else(|| {
        // Strip the leading #app_ prefix and convert underscores to spaces as a
        // readable last-resort fallback (should only happen during development).
        key.trim_start_matches('#')
            .trim_start_matches("app_")
            .replace('_', " ")
    })
}

/// Look up a localized weapon name, falling back to a readable title-case enum name.
pub fn weapon_name(weapon: &analysis::Weapon) -> String {
    let key = format!("#app_weapon_{:?}", weapon).to_lowercase();
    analysis::translate_key(&key).unwrap_or_else(|| {
        let raw = format!("{:?}", weapon);
        let mut result = String::new();
        for (i, c) in raw.chars().enumerate() {
            if i > 0 && c.is_uppercase() {
                result.push(' ');
            }
            result.push(c);
        }
        result
    })
}

#[derive(Default)]
pub struct PlayerHighlighting {
    pub highlighted: HashSet<PlayerGlobalId>,
}

pub mod chat;
pub mod pov;
pub mod rounds;
pub mod scoreboard;
pub mod streaks;
pub mod summary;
pub mod timeline;
pub mod weapons;

pub use chat::chat_log_ui;
pub use pov::pov_analytics_ui;
pub use rounds::rounds_ui;
pub use scoreboard::scoreboard_ui;
pub use streaks::kill_streaks_ui;
pub use summary::header_ui;
pub use timeline::team_score_timeline_ui;
pub use weapons::weapon_breakdowns_ui;

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
    let mut current_tab = ui.data(|d| {
        d.get_temp::<String>(tab_id)
            .unwrap_or_else(|| "Summary".to_string())
    });

    ui.horizontal(|ui| {
        ui.selectable_value(
            &mut current_tab,
            "Summary".to_string(),
            t("#app_tab_summary"),
        );
        ui.selectable_value(
            &mut current_tab,
            "Scoreboard".to_string(),
            t("#app_tab_scoreboard"),
        );
        ui.selectable_value(
            &mut current_tab,
            "Timeline".to_string(),
            t("#app_tab_timeline"),
        );
        ui.selectable_value(&mut current_tab, "Rounds".to_string(), t("#app_tab_rounds"));
        ui.selectable_value(
            &mut current_tab,
            "Weapon Breakdowns".to_string(),
            t("#app_tab_weapons"),
        );
        ui.selectable_value(
            &mut current_tab,
            "Kill Streaks".to_string(),
            t("#app_tab_streaks"),
        );
        ui.selectable_value(&mut current_tab, "Chat Log".to_string(), t("#app_tab_chat"));

        let is_pov = r
            .map(|a| a.demo_info.demo_type.as_str() == "POV")
            .unwrap_or(false);
        if ui
            .add_enabled(
                is_pov,
                egui::Button::new(t("#app_tab_pov")).selected(current_tab == "POV Analytics"),
            )
            .clicked()
        {
            current_tab = "POV Analytics".to_string();
        }
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
        "POV Analytics" => pov_analytics_ui(file_info, r, ui),
        _ => {}
    }
}
