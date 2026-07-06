use crate::FileInfo;
use analysis::Analysis;
use analysis::PlayerGlobalId;
use egui::{Color32, Ui};
use std::collections::HashSet;

pub const TABLE_ROW_HEIGHT: f32 = 18.;
pub const ALLIES_COLOR: Color32 = Color32::DARK_GREEN;
pub const BRITISH_COLOR: Color32 = Color32::from_rgb(218, 165, 32);
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
    // 1. Try to find translation using the official dod_english.txt keys first
    let official_key = match weapon {
        analysis::Weapon::Garand => Some("#wpn_garand"),
        analysis::Weapon::M1Carbine => Some("#wpn_carbine"),
        analysis::Weapon::Thompson => Some("#wpn_tommy"),
        analysis::Weapon::GreaseGun => Some("#wpn_grease"),
        analysis::Weapon::Springfield => Some("#wpn_spring"),
        analysis::Weapon::Bar => Some("#wpn_bar"),
        analysis::Weapon::Browning30Cal => Some("#wpn_30cal"),
        analysis::Weapon::LeeEnfield => Some("#wpn_enfield"),
        analysis::Weapon::Sten => Some("#wpn_sten"),
        analysis::Weapon::ScopedLeeEnfield => Some("#wpn_enfields"),
        analysis::Weapon::Bren => Some("#wpn_bren"),
        analysis::Weapon::K98 => Some("#wpn_k98"),
        analysis::Weapon::K43 => Some("#wpn_k43"),
        analysis::Weapon::Mp40 => Some("#wpn_mp40"),
        analysis::Weapon::Stg44 => Some("#wpn_mp44"),
        analysis::Weapon::ScopedK98 => Some("#wpn_k98s"),
        analysis::Weapon::Mg34 => Some("#wpn_mg34"),
        analysis::Weapon::Mg42 => Some("#wpn_mg42"),
        analysis::Weapon::Fg42 => Some("#wpn_fg42"),
        analysis::Weapon::ScopedFg42 => Some("#wpn_fg42s"),
        analysis::Weapon::Bazooka => Some("#wpn_bazooka"),
        analysis::Weapon::Panzerschreck => Some("#wpn_pschreck"),
        analysis::Weapon::Piat => Some("#wpn_piat"),
        analysis::Weapon::Mortar => Some("#wpn_mortar"),
        _ => None,
    };

    if let Some(key) = official_key {
        if let Some(translation) = analysis::translate_key(key) {
            return translation;
        }
    }

    // 2. Fallback to our custom keys in dod_tools_english.txt or readable title case
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
pub mod player_details;
pub mod pov;
pub mod rounds;
pub mod scoreboard;
pub mod summary;
pub mod team_details;
pub mod timeline;
pub mod batch_queue;
pub mod capture_studio;
#[cfg(not(target_arch = "wasm32"))]
pub mod browser;

#[cfg(not(target_arch = "wasm32"))]
pub mod auditor;

#[cfg(not(target_arch = "wasm32"))]
pub mod capture;

pub use chat::chat_log_ui;
pub use player_details::player_details_ui;
#[allow(unused_imports)]
pub use pov::pov_analytics_ui;
pub use rounds::rounds_ui;
pub use scoreboard::scoreboard_ui;
pub use summary::header_ui;
pub use team_details::team_details_ui;
pub use timeline::team_score_timeline_ui;
#[allow(unused_imports)]
pub use batch_queue::batch_queue_ui;

pub fn report_ui(
    file_info: Option<&FileInfo>,
    r: Option<&Analysis>,
    player_highlighting: &mut PlayerHighlighting,
    scoreboard_cache: &mut crate::ScoreboardCache,
    chat_cache: &mut crate::ChatCache,
    player_details_cache: &mut crate::PlayerDetailsCache,
    _export_queue: &mut Vec<crate::QueuedStreakExport>,
    _settings: &mut crate::AppSettings,
    ui: &mut Ui,
) {
    let tab_id = egui::Id::new("active_report_tab");
    let mut current_tab = ui.data(|d| {
        d.get_temp::<String>(tab_id)
            .unwrap_or_else(|| "Summary".to_string())
    });

    ui.horizontal(|ui| {
        let tabs = [
            ("Summary", t("#app_tab_summary")),
            ("Scoreboard", t("#app_tab_scoreboard")),
            ("Player Details", t("#app_tab_player_details")),
            ("Team Details", t("#app_tab_team_details")),
            ("Timeline", t("#app_tab_timeline")),
            ("Rounds", t("#app_tab_rounds")),
            ("Chat Log", t("#app_tab_chat")),
        ];

        for (val, label) in tabs {
            let is_active = current_tab == val;
            let text = egui::RichText::new(&label);
            let text = if is_active {
                text.color(ui.visuals().selection.bg_fill).strong()
            } else {
                text.color(ui.visuals().widgets.noninteractive.text_color())
            };
            
            let response = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked() {
                current_tab = val.to_string();
            }
            if is_active {
                let rect = response.rect;
                let stroke = egui::Stroke::new(2.0, ui.visuals().selection.bg_fill);
                ui.painter().hline(rect.left()..=rect.right(), rect.bottom(), stroke);
            }
            ui.add_space(12.0);
        }
    });

    ui.separator();

    ui.data_mut(|d| d.insert_temp(tab_id, current_tab.clone()));

    match current_tab.as_str() {
        "Summary" => header_ui(file_info, r, ui),
        "Scoreboard" => scoreboard_ui(r, player_highlighting, scoreboard_cache, ui),
        "Player Details" => player_details_ui(r, player_highlighting, player_details_cache, ui),
        "Team Details" => team_details_ui(r, ui),
        "Timeline" => team_score_timeline_ui(file_info, r, ui),
        "Rounds" => rounds_ui(file_info, r, ui),
        "Chat Log" => chat_log_ui(file_info, r, chat_cache, ui),
        _ => {}
    }
}
