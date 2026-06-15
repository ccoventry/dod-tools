//! Demo analyzer entry point with an interactive directory browser.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod explorer;
mod views;

use analysis::Analysis;
use clap::Parser;
use egui::{Align, CentralPanel, Context, Frame, Layout, ScrollArea, SidePanel, TopBottomPanel};
use egui_extras::{Column, TableBuilder};
use native::FileInfo;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use views::{PlayerHighlighting, report_ui, t};
use views::chat::{PlayerStatusFilter, PlayerTeamFilter};

#[cfg(not(target_arch = "wasm32"))]
use egui_file_dialog::FileDialog;
#[cfg(not(target_arch = "wasm32"))]
use native::run_analyzer_with_progress;

#[cfg(not(target_arch = "wasm32"))]
use explorer::{DemoListItem, get_native_roots, render_native_dir_node, scan_dir_async, scan_demo_folders_async, count_demo_files};
#[cfg(target_arch = "wasm32")]
use explorer::{DirNode, SendWrapper, WebFile, build_web_tree, render_web_dir_node};
use explorer::{DemoCache, CachedDemo};

#[derive(Debug, Parser)]
struct Args {
    demo_paths: Vec<PathBuf>,
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_maximized(true),
        ..Default::default()
    };

    let title = format!("dod-tools v{}", env!("CARGO_PKG_VERSION"));
    eframe::run_native(
        &title,
        options,
        Box::new(|_cc| {
            Ok(Box::new(
                Gui::default().with_initial_files(Args::parse().demo_paths),
            ))
        }),
    )
    .expect("Could not run the GUI");
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn selectFolder() -> Result<js_sys::Array, JsValue>;
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let web_options = eframe::WebOptions::default();

    let document = web_sys::window()
        .and_then(|win| win.document())
        .ok_or_else(|| JsValue::from_str("Failed to get document"))?;
    let canvas = document
        .get_element_by_id("the_canvas_id")
        .ok_or_else(|| JsValue::from_str("Canvas element with id 'the_canvas_id' not found"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| JsValue::from_str("Element with id 'the_canvas_id' is not a canvas"))?;

    eframe::WebRunner::new()
        .start(
            canvas,
            web_options,
            Box::new(|_cc| Ok(Box::new(Gui::default()))),
        )
        .await?;

    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}

#[derive(Debug, Clone)]
struct AppSettings {
    language: String,
    scan_folders_for_demos: bool,
    demo_folder_history: Vec<std::path::PathBuf>,
    pinned_folders: Vec<std::path::PathBuf>,
    capture_init_commands: String,
    capture_start_commands: String,
    capture_stop_commands: String,
    capture_initial_delay: f32,
    capture_fast_forward_speed: f32,
    capture_pre_record_buffer: f32,
    capture_record_start_lead: f32,
    capture_record_stop_trail: f32,
    capture_post_record_buffer: f32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "auto".to_string(),
            scan_folders_for_demos: false,
            demo_folder_history: Vec::new(),
            pinned_folders: Vec::new(),
            capture_init_commands: "".to_string(),
            capture_start_commands: "r_decals 5555; hud_deathnotice_time 5555; hud_draw 1".to_string(),
            capture_stop_commands: "r_decals 0; hud_deathnotice_time 5; hud_draw 0".to_string(),
            capture_initial_delay: 3.0,
            capture_fast_forward_speed: 0.2,
            capture_pre_record_buffer: 6.0,
            capture_record_start_lead: 2.0,
            capture_record_stop_trail: 2.0,
            capture_post_record_buffer: 4.0,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn load_settings() -> AppSettings {
    let path = std::path::PathBuf::from("settings.json");
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                let language = val
                    .get("language")
                    .and_then(|v| v.as_str())
                    .unwrap_or("auto")
                    .to_string();
                let scan_folders_for_demos = val
                    .get("scan_folders_for_demos")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let demo_folder_history = val
                    .get("demo_folder_history")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(std::path::PathBuf::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let pinned_folders = val
                    .get("pinned_folders")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(std::path::PathBuf::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let capture_init_commands = val
                    .get("capture_init_commands")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let capture_start_commands = val
                    .get("capture_start_commands")
                    .and_then(|v| v.as_str())
                    .unwrap_or("r_decals 5555; hud_deathnotice_time 5555; hud_draw 1")
                    .to_string();
                let capture_stop_commands = val
                    .get("capture_stop_commands")
                    .and_then(|v| v.as_str())
                    .unwrap_or("r_decals 0; hud_deathnotice_time 5; hud_draw 0")
                    .to_string();
                let capture_initial_delay = val.get("capture_initial_delay").and_then(|v| v.as_f64()).unwrap_or(3.0) as f32;
                let capture_fast_forward_speed = val.get("capture_fast_forward_speed").and_then(|v| v.as_f64()).unwrap_or(0.2) as f32;
                let capture_pre_record_buffer = val.get("capture_pre_record_buffer").and_then(|v| v.as_f64()).unwrap_or(6.0) as f32;
                let capture_record_start_lead = val.get("capture_record_start_lead").and_then(|v| v.as_f64()).unwrap_or(2.0) as f32;
                let capture_record_stop_trail = val.get("capture_record_stop_trail").and_then(|v| v.as_f64()).unwrap_or(2.0) as f32;
                let capture_post_record_buffer = val.get("capture_post_record_buffer").and_then(|v| v.as_f64()).unwrap_or(4.0) as f32;
                return AppSettings {
                    language,
                    scan_folders_for_demos,
                    demo_folder_history,
                    pinned_folders,
                    capture_init_commands,
                    capture_start_commands,
                    capture_stop_commands,
                    capture_initial_delay,
                    capture_fast_forward_speed,
                    capture_pre_record_buffer,
                    capture_record_start_lead,
                    capture_record_stop_trail,
                    capture_post_record_buffer,
                };
            }
        }
    }
    AppSettings::default()
}

#[cfg(target_arch = "wasm32")]
fn load_settings() -> AppSettings {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            if let Ok(Some(content)) = storage.get_item("settings") {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    let language = val
                        .get("language")
                        .and_then(|v| v.as_str())
                        .unwrap_or("auto")
                        .to_string();
                    let scan_folders_for_demos = val
                        .get("scan_folders_for_demos")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let demo_folder_history = val
                        .get("demo_folder_history")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(std::path::PathBuf::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let pinned_folders = val
                        .get("pinned_folders")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(std::path::PathBuf::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let capture_init_commands = val
                        .get("capture_init_commands")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let capture_start_commands = val
                        .get("capture_start_commands")
                        .and_then(|v| v.as_str())
                        .unwrap_or("r_decals 5555; hud_deathnotice_time 5555; hud_draw 1")
                        .to_string();
                    let capture_stop_commands = val
                        .get("capture_stop_commands")
                        .and_then(|v| v.as_str())
                        .unwrap_or("r_decals 0; hud_deathnotice_time 5; hud_draw 0")
                        .to_string();
                    let capture_initial_delay = val.get("capture_initial_delay").and_then(|v| v.as_f64()).unwrap_or(3.0) as f32;
                    let capture_fast_forward_speed = val.get("capture_fast_forward_speed").and_then(|v| v.as_f64()).unwrap_or(0.2) as f32;
                    let capture_pre_record_buffer = val.get("capture_pre_record_buffer").and_then(|v| v.as_f64()).unwrap_or(6.0) as f32;
                    let capture_record_start_lead = val.get("capture_record_start_lead").and_then(|v| v.as_f64()).unwrap_or(2.0) as f32;
                    let capture_record_stop_trail = val.get("capture_record_stop_trail").and_then(|v| v.as_f64()).unwrap_or(2.0) as f32;
                    let capture_post_record_buffer = val.get("capture_post_record_buffer").and_then(|v| v.as_f64()).unwrap_or(4.0) as f32;
                    return AppSettings {
                        language,
                        scan_folders_for_demos,
                        demo_folder_history,
                        pinned_folders,
                        capture_init_commands,
                        capture_start_commands,
                        capture_stop_commands,
                        capture_initial_delay,
                        capture_fast_forward_speed,
                        capture_pre_record_buffer,
                        capture_record_start_lead,
                        capture_record_stop_trail,
                        capture_post_record_buffer,
                    };
                }
            }
        }
    }
    AppSettings::default()
}

#[cfg(not(target_arch = "wasm32"))]
fn save_settings(settings: &AppSettings) {
    let mut map = serde_json::Map::new();
    map.insert(
        "language".to_string(),
        serde_json::Value::String(settings.language.clone()),
    );
    map.insert(
        "scan_folders_for_demos".to_string(),
        serde_json::Value::Bool(settings.scan_folders_for_demos),
    );
    map.insert(
        "demo_folder_history".to_string(),
        serde_json::Value::Array(
            settings
                .demo_folder_history
                .iter()
                .map(|p| serde_json::Value::String(p.to_string_lossy().into_owned()))
                .collect(),
        ),
    );
    map.insert(
        "pinned_folders".to_string(),
        serde_json::Value::Array(
            settings
                .pinned_folders
                .iter()
                .map(|p| serde_json::Value::String(p.to_string_lossy().into_owned()))
                .collect(),
        ),
    );
    map.insert(
        "capture_init_commands".to_string(),
        serde_json::Value::String(settings.capture_init_commands.clone()),
    );
    map.insert(
        "capture_start_commands".to_string(),
        serde_json::Value::String(settings.capture_start_commands.clone()),
    );
    map.insert(
        "capture_stop_commands".to_string(),
        serde_json::Value::String(settings.capture_stop_commands.clone()),
    );
    map.insert(
        "capture_initial_delay".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_initial_delay as f64).unwrap()),
    );
    map.insert(
        "capture_fast_forward_speed".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_fast_forward_speed as f64).unwrap()),
    );
    map.insert(
        "capture_pre_record_buffer".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_pre_record_buffer as f64).unwrap()),
    );
    map.insert(
        "capture_record_start_lead".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_record_start_lead as f64).unwrap()),
    );
    map.insert(
        "capture_record_stop_trail".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_record_stop_trail as f64).unwrap()),
    );
    map.insert(
        "capture_post_record_buffer".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_post_record_buffer as f64).unwrap()),
    );
    let val = serde_json::Value::Object(map);
    if let Ok(content) = serde_json::to_string_pretty(&val) {
        let _ = std::fs::write("settings.json", content);
    }
}

#[cfg(target_arch = "wasm32")]
fn save_settings(settings: &AppSettings) {
    let mut map = serde_json::Map::new();
    map.insert(
        "language".to_string(),
        serde_json::Value::String(settings.language.clone()),
    );
    map.insert(
        "scan_folders_for_demos".to_string(),
        serde_json::Value::Bool(settings.scan_folders_for_demos),
    );
    map.insert(
        "demo_folder_history".to_string(),
        serde_json::Value::Array(
            settings
                .demo_folder_history
                .iter()
                .map(|p| serde_json::Value::String(p.to_string_lossy().into_owned()))
                .collect(),
        ),
    );
    map.insert(
        "pinned_folders".to_string(),
        serde_json::Value::Array(
            settings
                .pinned_folders
                .iter()
                .map(|p| serde_json::Value::String(p.to_string_lossy().into_owned()))
                .collect(),
        ),
    );
    map.insert(
        "capture_init_commands".to_string(),
        serde_json::Value::String(settings.capture_init_commands.clone()),
    );
    map.insert(
        "capture_start_commands".to_string(),
        serde_json::Value::String(settings.capture_start_commands.clone()),
    );
    map.insert(
        "capture_stop_commands".to_string(),
        serde_json::Value::String(settings.capture_stop_commands.clone()),
    );
    map.insert(
        "capture_initial_delay".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_initial_delay as f64).unwrap()),
    );
    map.insert(
        "capture_fast_forward_speed".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_fast_forward_speed as f64).unwrap()),
    );
    map.insert(
        "capture_pre_record_buffer".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_pre_record_buffer as f64).unwrap()),
    );
    map.insert(
        "capture_record_start_lead".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_record_start_lead as f64).unwrap()),
    );
    map.insert(
        "capture_record_stop_trail".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_record_stop_trail as f64).unwrap()),
    );
    map.insert(
        "capture_post_record_buffer".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_post_record_buffer as f64).unwrap()),
    );
    let val = serde_json::Value::Object(map);
    if let Ok(content) = serde_json::to_string_pretty(&val) {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let _ = storage.set_item("settings", &content);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn detect_os_language() -> String {
    for var in &["LANG", "LC_ALL", "LC_MESSAGES"] {
        if let Ok(val) = std::env::var(var) {
            let val_lower = val.to_lowercase();
            if val_lower.starts_with("de") {
                return "german".to_string();
            }
            if val_lower.starts_with("fr") {
                return "french".to_string();
            }
            if val_lower.starts_with("es") {
                return "spanish".to_string();
            }
            if val_lower.starts_with("ru") {
                return "russian".to_string();
            }
            if val_lower.starts_with("en") {
                return "english".to_string();
            }
        }
    }

    if let Ok(output) = std::process::Command::new("reg")
        .args(&[
            "query",
            "HKCU\\Control Panel\\International",
            "/v",
            "LocaleName",
        ])
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).to_lowercase();
            for line in s.lines() {
                if line.contains("localename") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if let Some(locale) = parts.last() {
                        let loc = locale.to_lowercase();
                        if loc.starts_with("de") {
                            return "german".to_string();
                        }
                        if loc.starts_with("fr") {
                            return "french".to_string();
                        }
                        if loc.starts_with("es") {
                            return "spanish".to_string();
                        }
                        if loc.starts_with("ru") {
                            return "russian".to_string();
                        }
                        if loc.starts_with("sr") {
                            return "serbian".to_string();
                        }
                        if loc.starts_with("tr") {
                            return "turkish".to_string();
                        }
                        if loc.starts_with("pl") {
                            return "polish".to_string();
                        }
                    }
                }
            }
        }
    }

    if let Ok(output) = std::process::Command::new("powershell")
        .args(&[
            "-NoProfile",
            "-Command",
            "[System.Globalization.CultureInfo]::CurrentUICulture.Name",
        ])
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_lowercase();
            if s.starts_with("de") {
                return "german".to_string();
            }
            if s.starts_with("fr") {
                return "french".to_string();
            }
            if s.starts_with("es") {
                return "spanish".to_string();
            }
            if s.starts_with("ru") {
                return "russian".to_string();
            }
            if s.starts_with("sr") {
                return "serbian".to_string();
            }
            if s.starts_with("tr") {
                return "turkish".to_string();
            }
            if s.starts_with("pl") {
                return "polish".to_string();
            }
        }
    }

    "english".to_string()
}

#[cfg(not(target_os = "windows"))]
fn detect_os_language() -> String {
    for var in &["LANG", "LC_ALL", "LC_MESSAGES"] {
        if let Ok(val) = std::env::var(var) {
            let val_lower = val.to_lowercase();
            if val_lower.starts_with("de") {
                return "german".to_string();
            }
            if val_lower.starts_with("fr") {
                return "french".to_string();
            }
            if val_lower.starts_with("es") {
                return "spanish".to_string();
            }
            if val_lower.starts_with("ru") {
                return "russian".to_string();
            }
            if val_lower.starts_with("en") {
                return "english".to_string();
            }
        }
    }
    "english".to_string()
}

fn apply_language_setting(settings_lang: &str) {
    let target_lang = if settings_lang == "auto" {
        detect_os_language()
    } else {
        settings_lang.to_string()
    };
    let static_lang = match target_lang.as_str() {
        "german" => "german",
        "french" => "french",
        "spanish" => "spanish",
        "russian" => "russian",
        "serbian" => "serbian",
        "polish" => "polish",
        "turkish" => "turkish",
        _ => "english",
    };
    analysis::set_active_language(static_lang);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortColumn {
    Name,
    Type,
    Map,
    Date,
}

#[derive(Default)]
pub struct ScoreboardCache {
    pub path: Option<String>,
    pub allies_players: Vec<usize>, // indices in analysis.state.players
    pub axis_players: Vec<usize>,
    pub spec_players: Vec<usize>,
    pub unassigned_players: Vec<usize>,
    pub allies_totals: (i32, i32, i32), // score, kills, deaths
    pub axis_totals: (i32, i32, i32),
    pub spec_totals: (i32, i32, i32),
    pub unassigned_totals: (i32, i32, i32),
    pub player_steam_ids: HashMap<analysis::PlayerGlobalId, String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ChatFilterState {
    pub show_mm1: bool,
    pub show_mm2: bool,
    pub show_status: PlayerStatusFilter,
    pub show_team_filter: PlayerTeamFilter,
    pub show_joins: bool,
    pub show_teams: bool,
    pub show_gameplay: bool,
    pub show_other_sys: bool,
    pub filter_text: String,
}

#[derive(Default)]
pub struct ChatCache {
    pub path: Option<String>,
    pub filter_state: Option<ChatFilterState>,
    pub filtered_indices: Vec<usize>, // indices in analysis.state.chat_messages
}

#[derive(Clone, Debug)]
pub struct ExportRequest {
    pub start_time: f32,
    pub stop_time: f32,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
struct PendingStreakExport {
    input_path: std::path::PathBuf,
    player_id: analysis::PlayerGlobalId,
    start_time: f32,
    stop_time: f32,
}

#[derive(Default)]
pub struct PlayerDetailsCache {
    pub path: Option<String>,
    pub player_id: Option<analysis::PlayerGlobalId>,
    pub disabled_weapons: std::collections::HashSet<analysis::Weapon>,
    pub sorted_weapons: Vec<analysis::Weapon>,
    pub sorted_weapon_breakdown: Vec<(analysis::Weapon, (u32, u32))>,
    pub filtered_streaks: Vec<(usize, Vec<usize>)>, // (streak_index, Vec<kill_index>)
    pub export_request: Option<ExportRequest>,
}

struct Gui {
    analyses: HashMap<String, (FileInfo, Analysis)>,
    selected_analysis_path: Option<String>,
    cache: DemoCache,
    player_highlight: PlayerHighlighting,
    error_message: Option<String>,
    settings: AppSettings,
    show_settings_window: bool,
    show_about_window: bool,

    filter_query: String,
    filter_type: String,
    filter_map: String,
    filter_date_start: String,
    filter_date_end: String,

    sort_column: Option<SortColumn>,
    sort_ascending: bool,

    rx: mpsc::Receiver<GuiMessage>,
    tx: mpsc::Sender<GuiMessage>,

    #[cfg(not(target_arch = "wasm32"))]
    file_picker: FileDialog,
    #[cfg(not(target_arch = "wasm32"))]
    capture_export_picker: FileDialog,
    #[cfg(not(target_arch = "wasm32"))]
    pending_streak_export: Option<PendingStreakExport>,
    #[cfg(not(target_arch = "wasm32"))]
    root_dir: Option<PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    current_dir: Option<PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    initial_files: Vec<PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    subdir_cache: HashMap<PathBuf, Vec<PathBuf>>,
    #[cfg(not(target_arch = "wasm32"))]
    explorer_demo_cache: HashMap<PathBuf, usize>,

    #[cfg(not(target_arch = "wasm32"))]
    desktop_files: Vec<DemoListItem>,
    #[cfg(not(target_arch = "wasm32"))]
    last_scanned_dir: Option<PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    scanning_dir: bool,

    #[cfg(not(target_arch = "wasm32"))]
    demo_folders: Vec<(PathBuf, usize)>,
    #[cfg(not(target_arch = "wasm32"))]
    scanning_demo_folders: bool,
    #[cfg(not(target_arch = "wasm32"))]
    current_scan_id: usize,

    #[cfg(target_arch = "wasm32")]
    web_files: Vec<WebFile>,
    #[cfg(target_arch = "wasm32")]
    demo_folders: Vec<(String, usize)>,

    loading_path: Option<String>,
    loading_progress: Option<f32>,
    loading_elapsed: Option<f32>,
    loading_eta: Option<f32>,
    #[cfg(target_arch = "wasm32")]
    selected_web_folder: String,
    #[cfg(target_arch = "wasm32")]
    web_tree: Option<DirNode>,
    #[cfg(target_arch = "wasm32")]
    parser_worker: Option<web_sys::Worker>,

    scoreboard_cache: ScoreboardCache,
    chat_cache: ChatCache,
    player_details_cache: PlayerDetailsCache,
}

#[allow(dead_code)]
enum GuiMessage {
    Idle,
    AnalyzerStart {
        _files: usize,
    },
    AnalyzerProgress {
        file_info: FileInfo,
        _progress: (usize, usize),
        analysis: Box<Analysis>,
    },
    AnalyzerError {
        path: String,
        error: String,
    },
    DemoParsingProgress {
        path: String,
        progress: f32,
        elapsed_sec: f32,
        eta_sec: Option<f32>,
    },
    #[cfg(not(target_arch = "wasm32"))]
    DirScanComplete {
        dir: PathBuf,
        files: Vec<DemoListItem>,
    },
    #[cfg(not(target_arch = "wasm32"))]
    DemoFoldersScanComplete {
        scan_id: usize,
        folders: Vec<(PathBuf, usize)>,
    },
    #[cfg(target_arch = "wasm32")]
    WebFolderLoaded(Vec<WebFile>),
    #[cfg(target_arch = "wasm32")]
    WebFileParsed {
        path: String,
        file_info: FileInfo,
        analysis: Box<Analysis>,
    },
}

impl Gui {
    fn check_scoreboard_cache(&mut self) {
        let path = match &self.selected_analysis_path {
            Some(p) => p.clone(),
            None => {
                self.scoreboard_cache = ScoreboardCache::default();
                return;
            }
        };

        if self.scoreboard_cache.path.as_ref() == Some(&path) {
            return;
        }

        let Some((_, analysis)) = self.analyses.get(&path) else {
            self.scoreboard_cache = ScoreboardCache::default();
            return;
        };

        use analysis::Team;
        use std::collections::HashMap;

        let mut player_steam_ids = HashMap::new();
        struct SortablePlayer {
            index: usize,
            score: i32,
            kills: i32,
            deaths: i32,
            name: String,
            steam_id_str: String,
        }

        let mut sorted_players: Vec<SortablePlayer> = analysis.state.players.iter().enumerate().map(|(idx, p)| {
            let steam_id_str = analysis::SteamId::try_from(&p.id)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| p.id.to_string());
            player_steam_ids.insert(p.id.clone(), steam_id_str.clone());
            SortablePlayer {
                index: idx,
                score: p.stats.0,
                kills: p.stats.1,
                deaths: p.stats.2,
                name: p.name.clone(),
                steam_id_str,
            }
        }).collect();

        // Sort by Score DESC, Kills DESC, Deaths ASC, Name ASC, ID ASC.
        sorted_players.sort_by(|a, b| {
            b.score.cmp(&a.score)
                .then_with(|| b.kills.cmp(&a.kills))
                .then_with(|| a.deaths.cmp(&b.deaths))
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.steam_id_str.cmp(&b.steam_id_str))
        });

        let is_british = analysis.state.allies_are_british;
        let allies_team = if is_british { Team::British } else { Team::Allies };

        let mut allies_players = Vec::new();
        let mut axis_players = Vec::new();
        let mut spec_players = Vec::new();
        let mut unassigned_players = Vec::new();

        let mut allies_totals = (analysis.state.team_scores.get_team_score(allies_team.clone()), 0, 0);
        let mut axis_totals = (analysis.state.team_scores.get_team_score(Team::Axis), 0, 0);
        let mut spec_totals = (0, 0, 0);
        let mut unassigned_totals = (0, 0, 0);

        for sp in sorted_players {
            let p = &analysis.state.players[sp.index];
            match p.team.as_ref() {
                Some(Team::Allies) | Some(Team::British) => {
                    allies_totals.1 += sp.kills;
                    allies_totals.2 += sp.deaths;
                    allies_players.push(sp.index);
                }
                Some(Team::Axis) => {
                    axis_totals.1 += sp.kills;
                    axis_totals.2 += sp.deaths;
                    axis_players.push(sp.index);
                }
                Some(Team::Spectators) => {
                    spec_totals.0 += sp.score;
                    spec_totals.1 += sp.kills;
                    spec_totals.2 += sp.deaths;
                    spec_players.push(sp.index);
                }
                Some(Team::Unassigned) | None => {
                    unassigned_totals.0 += sp.score;
                    unassigned_totals.1 += sp.kills;
                    unassigned_totals.2 += sp.deaths;
                    unassigned_players.push(sp.index);
                }
            }
        }

        self.scoreboard_cache = ScoreboardCache {
            path: Some(path),
            allies_players,
            axis_players,
            spec_players,
            unassigned_players,
            allies_totals,
            axis_totals,
            spec_totals,
            unassigned_totals,
            player_steam_ids,
        };
    }

    fn save_cache(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Ok(content) = serde_json::to_string_pretty(&self.cache) {
                let _ = std::fs::write(".dod-tools-cache.json", content);
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            if let Ok(content) = serde_json::to_string(&self.cache) {
                if let Some(window) = web_sys::window() {
                    if let Ok(Some(storage)) = window.local_storage() {
                        let _ = storage.set_item("demo_cache", &content);
                    }
                }
            }
        }
    }

    fn add_analysis_to_cache(&mut self, file_info: &FileInfo, analysis: &Analysis) {
        let path = file_info.path.clone();
        
        let date_str = {
            let duration = file_info.created_at
                .duration_since(web_time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default();
            let secs = duration.as_secs() as i64;
            let nsecs = duration.subsec_nanos();
            let dt = chrono::DateTime::from_timestamp(secs, nsecs).unwrap_or_default();
            #[cfg(not(target_arch = "wasm32"))]
            {
                chrono::DateTime::<chrono::Local>::from(dt)
                    .format("%Y-%m-%d %I:%M %p")
                    .to_string()
            }
            #[cfg(target_arch = "wasm32")]
            {
                dt.format("%Y-%m-%d").to_string()
            }
        };
        
        let modified_ms = file_info.created_at
            .duration_since(web_time::SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let cached = CachedDemo {
            path: path.clone(),
            name: file_info.name.clone(),
            map_name: analysis.demo_info.map_name.clone(),
            date: date_str,
            demo_type: analysis.demo_info.demo_type.clone(),
            size_bytes: file_info.size_bytes,
            modified_ms,
        };

        self.cache.demos.insert(path, cached);
        self.save_cache();
    }

    fn filter_demo(&self, name: &str, map_name: &str, date: &str, path_str: &str) -> bool {
        if !self.filter_query.is_empty() {
            let q = self.filter_query.to_lowercase();
            if !name.to_lowercase().contains(&q)
                && !map_name.to_lowercase().contains(&q)
                && !path_str.to_lowercase().contains(&q)
            {
                return false;
            }
        }

        if self.filter_type != "All" {
            let demo_type = if let Some((_, analysis)) = self.analyses.get(path_str) {
                analysis.demo_info.demo_type.as_str()
            } else if let Some(cached) = self.cache.demos.get(path_str) {
                cached.demo_type.as_str()
            } else if name.to_lowercase().contains("hltv") {
                "HLTV"
            } else {
                "POV"
            };
            if demo_type != self.filter_type {
                return false;
            }
        }

        if !self.filter_map.is_empty() {
            let m = self.filter_map.to_lowercase();
            if !map_name.to_lowercase().contains(&m) {
                return false;
            }
        }

        if self.filter_date_start.len() == 10 {
            if date.len() >= 10 {
                if &date[..10] < self.filter_date_start.as_str() {
                    return false;
                }
            } else {
                return false;
            }
        }
        if self.filter_date_end.len() == 10 {
            if date.len() >= 10 {
                if &date[..10] > self.filter_date_end.as_str() {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    fn toggle_sort(&mut self, col: SortColumn) {
        if self.sort_column == Some(col) {
            self.sort_ascending = !self.sort_ascending;
        } else {
            self.sort_column = Some(col);
            self.sort_ascending = true;
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn with_initial_files(mut self, files: Vec<PathBuf>) -> Self {
        self.initial_files = files;
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn trigger_dir_scan(&mut self, ctx: &Context) {
        if let Some(dir) = &self.current_dir {
            self.scanning_dir = true;
            scan_dir_async(ctx.clone(), self.tx.clone(), dir.clone());
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn trigger_demo_folders_scan(&mut self, ctx: &Context) {
        let root = self.root_dir.clone().or_else(|| self.current_dir.clone());
        if let Some(dir) = root {
            self.current_scan_id += 1;
            self.scanning_demo_folders = true;
            scan_demo_folders_async(ctx.clone(), self.tx.clone(), dir, self.current_scan_id);
        }
    }
}

impl Default for Gui {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let settings = load_settings();
        apply_language_setting(&settings.language);

        #[cfg(not(target_arch = "wasm32"))]
        let cache = {
            let cache_path = std::path::Path::new(".dod-tools-cache.json");
            if cache_path.exists() {
                std::fs::read_to_string(cache_path)
                    .ok()
                    .and_then(|content| serde_json::from_str::<DemoCache>(&content).ok())
                    .unwrap_or_default()
            } else {
                DemoCache::default()
            }
        };
        #[cfg(target_arch = "wasm32")]
        let cache = {
            let mut c = DemoCache::default();
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(Some(content)) = storage.get_item("demo_cache") {
                        if let Ok(loaded) = serde_json::from_str::<DemoCache>(&content) {
                            c = loaded;
                        }
                    }
                }
            }
            c
        };

        Self {
            analyses: HashMap::default(),
            selected_analysis_path: None,
            cache,
            player_highlight: PlayerHighlighting::default(),
            error_message: None,
            rx,
            tx,
            settings,
            show_settings_window: false,
            show_about_window: false,

            filter_query: String::new(),
            filter_type: "All".to_string(),
            filter_map: String::new(),
            filter_date_start: String::new(),
            filter_date_end: String::new(),

            sort_column: Some(SortColumn::Name),
            sort_ascending: true,

            #[cfg(not(target_arch = "wasm32"))]
            file_picker: FileDialog::default(),
            #[cfg(not(target_arch = "wasm32"))]
            capture_export_picker: FileDialog::default(),
            #[cfg(not(target_arch = "wasm32"))]
            pending_streak_export: None,
            #[cfg(not(target_arch = "wasm32"))]
            root_dir: std::env::current_dir().ok(),
            #[cfg(not(target_arch = "wasm32"))]
            current_dir: std::env::current_dir().ok(),
            #[cfg(not(target_arch = "wasm32"))]
            initial_files: Vec::default(),
            #[cfg(not(target_arch = "wasm32"))]
            subdir_cache: HashMap::default(),
            #[cfg(not(target_arch = "wasm32"))]
            explorer_demo_cache: HashMap::default(),

            #[cfg(not(target_arch = "wasm32"))]
            desktop_files: Vec::default(),
            #[cfg(not(target_arch = "wasm32"))]
            last_scanned_dir: None,
            #[cfg(not(target_arch = "wasm32"))]
            scanning_dir: false,

            #[cfg(not(target_arch = "wasm32"))]
            demo_folders: Vec::default(),
            #[cfg(not(target_arch = "wasm32"))]
            scanning_demo_folders: false,
            #[cfg(not(target_arch = "wasm32"))]
            current_scan_id: 0,

            #[cfg(target_arch = "wasm32")]
            web_files: Vec::default(),
            #[cfg(target_arch = "wasm32")]
            demo_folders: Vec::default(),

            loading_path: None,
            loading_progress: None,
            loading_elapsed: None,
            loading_eta: None,
            #[cfg(target_arch = "wasm32")]
            selected_web_folder: ".".to_string(),
            #[cfg(target_arch = "wasm32")]
            web_tree: None,
            #[cfg(target_arch = "wasm32")]
            parser_worker: None,

            scoreboard_cache: ScoreboardCache::default(),
            chat_cache: ChatCache::default(),
            player_details_cache: PlayerDetailsCache::default(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn pick_web_folder(ctx: Context, tx: mpsc::Sender<GuiMessage>) {
    wasm_bindgen_futures::spawn_local(async move {
        if let Ok(array) = selectFolder().await {
            let mut files = vec![];
            for val in array.iter() {
                let name = js_sys::Reflect::get(&val, &JsValue::from_str("name"))
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_default();
                let path = js_sys::Reflect::get(&val, &JsValue::from_str("path"))
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_default();
                if let Ok(file_val) = js_sys::Reflect::get(&val, &JsValue::from_str("file")) {
                    if let Ok(file) = file_val.dyn_into::<web_sys::File>() {
                        files.push(WebFile {
                            name,
                            path,
                            js_file: SendWrapper(file),
                        });
                    }
                }
            }
            // Sort files alphabetically by path
            files.sort_by(|a, b| a.path.cmp(&b.path));
            tx.send(GuiMessage::WebFolderLoaded(files)).ok();
            ctx.request_repaint();
        }
    });
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn init_worker() {
    console_error_panic_hook::set_once();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn handle_worker_message(data: wasm_bindgen::JsValue) {
    let global = js_sys::global();
    let post_message = js_sys::Reflect::get(&global, &"postMessage".into())
        .unwrap()
        .dyn_into::<js_sys::Function>()
        .unwrap();
        
    let type_val = js_sys::Reflect::get(&data, &"type".into())
        .unwrap()
        .as_string()
        .unwrap_or_default();
        
    if type_val == "parse" {
        let path = js_sys::Reflect::get(&data, &"path".into())
            .unwrap()
            .as_string()
            .unwrap_or_default();
        let name = js_sys::Reflect::get(&data, &"name".into())
            .unwrap()
            .as_string()
            .unwrap_or_default();
        let last_modified = js_sys::Reflect::get(&data, &"lastModified".into())
            .unwrap()
            .as_f64()
            .unwrap_or(0.0);
        let size = js_sys::Reflect::get(&data, &"size".into())
            .unwrap()
            .as_f64()
            .unwrap_or(0.0);
            
        let bytes_val = js_sys::Reflect::get(&data, &"bytes".into()).unwrap();
        let uint8_array = js_sys::Uint8Array::new(&bytes_val);
        let bytes = uint8_array.to_vec();
        
        let path_clone = path.clone();
        let post_message_clone = post_message.clone();
        let start_time = web_time::SystemTime::now();
        
        let progress_cb = move |processed: usize, total: usize| {
            if total > 0 {
                let progress = processed as f32 / total as f32;
                let elapsed_sec = start_time.elapsed().map(|d| d.as_secs_f32()).unwrap_or(0.0);
                let eta_sec = if progress > 0.01 {
                    let total_estimated_sec = elapsed_sec / progress;
                    Some(total_estimated_sec - elapsed_sec)
                } else {
                    None
                };
                
                let progress_obj = js_sys::Object::new();
                js_sys::Reflect::set(&progress_obj, &"type".into(), &"progress".into()).unwrap();
                js_sys::Reflect::set(&progress_obj, &"path".into(), &path_clone.clone().into()).unwrap();
                js_sys::Reflect::set(&progress_obj, &"progress".into(), &progress.into()).unwrap();
                js_sys::Reflect::set(&progress_obj, &"elapsedSec".into(), &elapsed_sec.into()).unwrap();
                if let Some(eta) = eta_sec {
                    js_sys::Reflect::set(&progress_obj, &"etaSec".into(), &eta.into()).unwrap();
                }
                
                let _ = post_message_clone.call1(&js_sys::global(), &progress_obj);
            }
        };
        
        match Analysis::try_from_bytes_with_progress(&bytes, progress_cb) {
            Ok(analysis) => {
                if let Ok(serialized) = serde_json::to_string(&analysis) {
                    let success_obj = js_sys::Object::new();
                    js_sys::Reflect::set(&success_obj, &"type".into(), &"success".into()).unwrap();
                    js_sys::Reflect::set(&success_obj, &"path".into(), &path.into()).unwrap();
                    js_sys::Reflect::set(&success_obj, &"name".into(), &name.into()).unwrap();
                    js_sys::Reflect::set(&success_obj, &"lastModified".into(), &last_modified.into()).unwrap();
                    js_sys::Reflect::set(&success_obj, &"size".into(), &size.into()).unwrap();
                    js_sys::Reflect::set(&success_obj, &"analysisJson".into(), &serialized.into()).unwrap();
                    
                    let _ = post_message.call1(&js_sys::global(), &success_obj);
                } else {
                    let error_obj = js_sys::Object::new();
                    js_sys::Reflect::set(&error_obj, &"type".into(), &"error".into()).unwrap();
                    js_sys::Reflect::set(&error_obj, &"path".into(), &path.into()).unwrap();
                    js_sys::Reflect::set(&error_obj, &"error".into(), &"Failed to serialize Analysis".into()).unwrap();
                    
                    let _ = post_message.call1(&js_sys::global(), &error_obj);
                }
            }
            Err(err) => {
                let error_obj = js_sys::Object::new();
                js_sys::Reflect::set(&error_obj, &"type".into(), &"error".into()).unwrap();
                js_sys::Reflect::set(&error_obj, &"path".into(), &path.into()).unwrap();
                js_sys::Reflect::set(&error_obj, &"error".into(), &err.into()).unwrap();
                
                let _ = post_message.call1(&js_sys::global(), &error_obj);
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Gui {
    fn get_or_spawn_worker(&mut self, ctx: &Context) -> Option<&web_sys::Worker> {
        if self.parser_worker.is_some() {
            return self.parser_worker.as_ref();
        }
        
        let window = web_sys::window()?;
        let document = window.document()?;
        let origin = window.location().origin().unwrap_or_default();
        
        let mut js_url = String::new();
        let mut wasm_url = String::new();
        
        // 1. Try modulepreload link or link[href*="dod-tools-gui"]
        if let Ok(Some(link)) = document.query_selector("link[rel=\"modulepreload\"]") {
            if let Ok(href) = js_sys::Reflect::get(&link, &"href".into()) {
                if let Some(href_str) = href.as_string() {
                    js_url = href_str;
                }
            }
        }
        if js_url.is_empty() {
            if let Ok(Some(link)) = document.query_selector("link[href*=\"dod-tools-gui\"][href*=\".js\"]") {
                if let Ok(href) = js_sys::Reflect::get(&link, &"href".into()) {
                    if let Some(href_str) = href.as_string() {
                        js_url = href_str;
                    }
                }
            }
        }
        
        // 2. Try link[href*=".wasm"]
        if let Ok(Some(link)) = document.query_selector("link[href*=\".wasm\"]") {
            if let Ok(href) = js_sys::Reflect::get(&link, &"href".into()) {
                if let Some(href_str) = href.as_string() {
                    wasm_url = href_str;
                }
            }
        }
        
        // 3. Fallback: scan scripts content or src
        let scripts = document.scripts();
        for i in 0..scripts.length() {
            if let Some(script) = scripts.item(i) {
                if let Ok(src) = js_sys::Reflect::get(&script, &"src".into()) {
                    if let Some(src_str) = src.as_string() {
                        if src_str.contains("dod-tools-gui") {
                            if js_url.is_empty() {
                                js_url = src_str;
                            }
                            continue;
                        }
                    }
                }
                
                if let Ok(text) = js_sys::Reflect::get(&script, &"textContent".into()) {
                    if let Some(text_str) = text.as_string() {
                        if text_str.contains("dod-tools-gui") {
                            if js_url.is_empty() {
                                if let Some(start) = text_str.find("from '") {
                                    let rest = &text_str[start + 6..];
                                    if let Some(end) = rest.find('\'') {
                                        js_url = rest[..end].to_string();
                                    }
                                } else if let Some(start) = text_str.find("from \"") {
                                    let rest = &text_str[start + 6..];
                                    if let Some(end) = rest.find('"') {
                                        js_url = rest[..end].to_string();
                                    }
                                }
                            }
                            
                            if wasm_url.is_empty() {
                                if let Some(start) = text_str.find("init('") {
                                    let rest = &text_str[start + 6..];
                                    if let Some(end) = rest.find('\'') {
                                        wasm_url = rest[..end].to_string();
                                    }
                                } else if let Some(start) = text_str.find("init(\"") {
                                    let rest = &text_str[start + 6..];
                                    if let Some(end) = rest.find('"') {
                                        wasm_url = rest[..end].to_string();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        if js_url.is_empty() {
            js_url = "dod-tools-gui.js".to_string();
        }
        if wasm_url.is_empty() {
            wasm_url = "dod-tools-gui_bg.wasm".to_string();
        }
        
        let make_absolute = |url: String, origin: &str| -> String {
            if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("blob:") {
                url
            } else if url.starts_with('/') {
                format!("{}{}", origin, url)
            } else {
                format!("{}/{}", origin, url)
            }
        };
        
        let js_url_abs = make_absolute(js_url, &origin);
        let wasm_url_abs = make_absolute(wasm_url, &origin);
        
        let blob_code = format!(
            r#"
            self.onmessage = async function(e) {{
                const {{ type, jsUrl, wasmUrl }} = e.data;
                if (type === 'init') {{
                    try {{
                        const wasm_bindgen = await import(jsUrl);
                        await wasm_bindgen.default(wasmUrl);
                        wasm_bindgen.init_worker();
                        self.postMessage({{ type: 'ready' }});
                    }} catch (err) {{
                        self.postMessage({{ type: 'error', error: err.toString() }});
                    }}
                }}
            }};
            "#
        );
        
        let blob = web_sys::Blob::new_with_str_sequence(
            &js_sys::Array::of1(&wasm_bindgen::JsValue::from_str(&blob_code))
        ).ok()?;
        let blob_url = web_sys::Url::create_object_url_with_blob(&blob).ok()?;
        
        // Spawn the Worker as an ES module worker: new Worker(blob_url, { type: "module" })
        let options = js_sys::Object::new();
        js_sys::Reflect::set(&options, &"type".into(), &"module".into()).unwrap();
        let args = js_sys::Array::of2(&blob_url.clone().into(), &options.into());
        let global = js_sys::global();
        let worker_constructor = js_sys::Reflect::get(&global, &"Worker".into())
            .ok()
            .and_then(|v| v.dyn_into::<js_sys::Function>().ok())?;
        
        let worker: web_sys::Worker = js_sys::Reflect::construct(&worker_constructor, &args)
            .ok()
            .and_then(|w| w.dyn_into::<web_sys::Worker>().ok())?;
        
        let init_obj = js_sys::Object::new();
        js_sys::Reflect::set(&init_obj, &"type".into(), &"init".into()).unwrap();
        js_sys::Reflect::set(&init_obj, &"jsUrl".into(), &js_url_abs.into()).unwrap();
        js_sys::Reflect::set(&init_obj, &"wasmUrl".into(), &wasm_url_abs.into()).unwrap();
        let _ = worker.post_message(&init_obj);
        
        let tx = self.tx.clone();
        let ctx_for_repaint = ctx.clone();
        
        let onmessage_callback = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            let data = event.data();
            let type_val = js_sys::Reflect::get(&data, &"type".into())
                .unwrap()
                .as_string()
                .unwrap_or_default();
                
            if type_val == "progress" {
                let path = js_sys::Reflect::get(&data, &"path".into())
                    .unwrap()
                    .as_string()
                    .unwrap_or_default();
                let progress = js_sys::Reflect::get(&data, &"progress".into())
                    .unwrap()
                    .as_f64()
                    .unwrap_or(0.0) as f32;
                let elapsed_sec = js_sys::Reflect::get(&data, &"elapsedSec".into())
                    .unwrap()
                    .as_f64()
                    .unwrap_or(0.0) as f32;
                let eta_sec = js_sys::Reflect::get(&data, &"etaSec".into())
                    .ok()
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32);
                
                let _ = tx.send(GuiMessage::DemoParsingProgress {
                    path,
                    progress,
                    elapsed_sec,
                    eta_sec,
                });
                ctx_for_repaint.request_repaint();
            } else if type_val == "success" {
                let path = js_sys::Reflect::get(&data, &"path".into())
                    .unwrap()
                    .as_string()
                    .unwrap_or_default();
                let name = js_sys::Reflect::get(&data, &"name".into())
                    .unwrap()
                    .as_string()
                    .unwrap_or_default();
                let last_modified = js_sys::Reflect::get(&data, &"lastModified".into())
                    .unwrap()
                    .as_f64()
                    .unwrap_or(0.0);
                let size = js_sys::Reflect::get(&data, &"size".into())
                    .unwrap()
                    .as_f64()
                    .unwrap_or(0.0);
                let analysis_json = js_sys::Reflect::get(&data, &"analysisJson".into())
                    .unwrap()
                    .as_string()
                    .unwrap_or_default();
                
                if let Ok(analysis) = serde_json::from_str::<Analysis>(&analysis_json) {
                    let created_at = web_time::SystemTime::UNIX_EPOCH
                        + std::time::Duration::from_millis(last_modified as u64);
                    let file_info = FileInfo {
                        created_at,
                        name,
                        path: path.clone(),
                        size_bytes: size as u64,
                    };
                    
                    let _ = tx.send(GuiMessage::WebFileParsed {
                        path,
                        file_info,
                        analysis: Box::new(analysis),
                    });
                }
                ctx_for_repaint.request_repaint();
            } else if type_val == "error" {
                let path = js_sys::Reflect::get(&data, &"path".into())
                    .unwrap()
                    .as_string()
                    .unwrap_or_default();
                let error = js_sys::Reflect::get(&data, &"error".into())
                    .unwrap()
                    .as_string()
                    .unwrap_or_default();
                
                let _ = tx.send(GuiMessage::AnalyzerError {
                    path,
                    error,
                });
                ctx_for_repaint.request_repaint();
            }
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);
        
        worker.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();
        
        self.parser_worker = Some(worker);
        self.parser_worker.as_ref()
    }

    #[cfg(target_arch = "wasm32")]
    fn parse_web_file(&mut self, ctx: &Context, web_file: WebFile) {
        let file = &web_file.js_file.0;
        let last_modified_ms = js_sys::Reflect::get(
            file.as_ref(),
            &wasm_bindgen::JsValue::from_str("lastModified"),
        )
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
        let size_bytes = file.size() as f64;
        let path = web_file.path.clone();
        let name = web_file.name.clone();
        
        let promise = file.array_buffer();
        
        if let Some(worker) = self.get_or_spawn_worker(ctx) {
            let worker_clone = worker.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(array_buffer_val) = wasm_bindgen_futures::JsFuture::from(promise).await {
                    let parse_obj = js_sys::Object::new();
                    js_sys::Reflect::set(&parse_obj, &"type".into(), &"parse".into()).unwrap();
                    js_sys::Reflect::set(&parse_obj, &"path".into(), &path.into()).unwrap();
                    js_sys::Reflect::set(&parse_obj, &"name".into(), &name.into()).unwrap();
                    js_sys::Reflect::set(&parse_obj, &"lastModified".into(), &last_modified_ms.into()).unwrap();
                    js_sys::Reflect::set(&parse_obj, &"size".into(), &size_bytes.into()).unwrap();
                    js_sys::Reflect::set(&parse_obj, &"bytes".into(), &array_buffer_val.into()).unwrap();
                    
                    let _ = worker_clone.post_message(&parse_obj);
                }
            });
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn parse_bytes_via_worker(&mut self, ctx: &Context, name: String, bytes: Vec<u8>) {
        if let Some(worker) = self.get_or_spawn_worker(ctx) {
            let parse_obj = js_sys::Object::new();
            js_sys::Reflect::set(&parse_obj, &"type".into(), &"parse".into()).unwrap();
            js_sys::Reflect::set(&parse_obj, &"path".into(), &name.clone().into()).unwrap();
            js_sys::Reflect::set(&parse_obj, &"name".into(), &name.clone().into()).unwrap();
            js_sys::Reflect::set(&parse_obj, &"lastModified".into(), &0.0.into()).unwrap();
            js_sys::Reflect::set(&parse_obj, &"size".into(), &(bytes.len() as f64).into()).unwrap();
            
            let uint8_array = js_sys::Uint8Array::from(bytes.as_slice());
            let array_buffer = uint8_array.buffer();
            js_sys::Reflect::set(&parse_obj, &"bytes".into(), &array_buffer.into()).unwrap();
            
            let _ = worker.post_message(&parse_obj);
        }
    }
}


impl eframe::App for Gui {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let modal_open = self.show_settings_window || self.show_about_window;

        if self.loading_path.is_some() {
            ctx.request_repaint();
        }

        // Update native file picker
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.file_picker.update(ctx);
            if let Some(path) = self.file_picker.take_picked() {
                self.root_dir = Some(path.clone());
                self.current_dir = Some(path);
                self.subdir_cache.clear();
                self.selected_analysis_path = None;
                self.error_message = None;
                self.trigger_demo_folders_scan(ctx);
            }

            self.capture_export_picker.update(ctx);
            if let Some(save_path) = self.capture_export_picker.take_picked() {
                if let Some(export_info) = self.pending_streak_export.take() {
                    match std::fs::read(&export_info.input_path) {
                        Ok(demo_bytes) => {
                            let hltv_spec_player = if let Some(analysis) = self.analyses.get(&export_info.input_path.to_string_lossy().into_owned()).map(|(_, a)| a) {
                                if analysis.demo_info.demo_type == "HLTV" {
                                    analysis.state.players.iter()
                                        .find(|p| p.id == export_info.player_id)
                                        .map(|p| p.name.clone())
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                            let options = native::patch::PatchOptions {
                                exit_on_finish: false,
                                init_commands: self.settings.capture_init_commands.lines().map(String::from).collect(),
                                start_commands: self.settings.capture_start_commands.lines().map(String::from).collect(),
                                stop_commands: self.settings.capture_stop_commands.lines().map(String::from).collect(),
                                fast_forward_speed: Some(self.settings.capture_fast_forward_speed),
                                hltv_spec_player,
                                initial_delay: Some(self.settings.capture_initial_delay),
                                pre_record_buffer: Some(self.settings.capture_pre_record_buffer),
                                record_start_lead: Some(self.settings.capture_record_start_lead),
                                record_stop_trail: Some(self.settings.capture_record_stop_trail),
                                post_record_buffer: Some(self.settings.capture_post_record_buffer),
                            };
                            let intervals = &[(export_info.start_time, export_info.stop_time)];
                            match native::patch::patch_demo_highlights(&demo_bytes, intervals, &options) {
                                Ok(patched_bytes) => {
                                    if let Err(e) = std::fs::write(&save_path, patched_bytes) {
                                        self.error_message = Some(format!("Failed to write patched demo: {}", e));
                                    }
                                }
                                Err(e) => {
                                    self.error_message = Some(format!("Failed to patch demo: {}", e));
                                }
                            }
                        }
                        Err(e) => {
                            self.error_message = Some(format!("Failed to read source demo: {}", e));
                        }
                    }
                }
            }
        }

        // Trigger directory scan if current_dir changed
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.current_dir != self.last_scanned_dir {
                let is_first_run = self.last_scanned_dir.is_none();
                self.last_scanned_dir = self.current_dir.clone();
                self.desktop_files.clear();
                self.trigger_dir_scan(ctx);
                if is_first_run {
                    self.trigger_demo_folders_scan(ctx);
                }
            }
        }

        // Handle incoming messages
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                GuiMessage::Idle => {}
                GuiMessage::AnalyzerStart { .. } => {}
                GuiMessage::AnalyzerProgress {
                    file_info,
                    analysis,
                    ..
                } => {
                    let path = file_info.path.clone();
                    self.add_analysis_to_cache(&file_info, &analysis);
                    self.selected_analysis_path = Some(path.clone());
                    self.analyses.insert(path, (file_info, *analysis));
                    self.loading_path = None;
                    self.loading_progress = None;
                    self.loading_elapsed = None;
                    self.loading_eta = None;
                }
                GuiMessage::AnalyzerError { path, error } => {
                    self.loading_path = None;
                    self.loading_progress = None;
                    self.loading_elapsed = None;
                    self.loading_eta = None;
                    self.error_message = Some(format!("Failed to analyze {}: {}", path, error));
                }
                GuiMessage::DemoParsingProgress {
                    path,
                    progress,
                    elapsed_sec,
                    eta_sec,
                } => {
                    if self.loading_path.as_deref() == Some(path.as_str()) {
                        self.loading_progress = Some(progress);
                        self.loading_elapsed = Some(elapsed_sec);
                        self.loading_eta = eta_sec;
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                GuiMessage::DirScanComplete { dir, files } => {
                    if self.current_dir.as_ref() == Some(&dir) {
                        self.desktop_files = files;
                        self.scanning_dir = false;

                        // Add to persistent history if there are demos in this directory!
                        if !self.desktop_files.is_empty() {
                            self.settings.demo_folder_history.retain(|p| p != &dir);
                            self.settings.demo_folder_history.insert(0, dir.clone());
                            if self.settings.demo_folder_history.len() > 10 {
                                self.settings.demo_folder_history.truncate(10);
                            }
                            save_settings(&self.settings);
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                GuiMessage::DemoFoldersScanComplete { scan_id, folders } => {
                    if scan_id == self.current_scan_id {
                        self.demo_folders = folders;
                        self.scanning_demo_folders = false;
                    }
                }
                #[cfg(target_arch = "wasm32")]
                GuiMessage::WebFolderLoaded(files) => {
                    self.web_files = files;
                    self.selected_web_folder = ".".to_string();
                    self.web_tree = Some(build_web_tree(&self.web_files));
                    self.selected_analysis_path = None;
                    self.error_message = None;

                    // Compute demo folders list for WASM
                    let mut folders = std::collections::HashMap::new();
                    for file in &self.web_files {
                        if file.name.ends_with(".dem") {
                            let relative_path = &file.path;
                            let folder = if let Some(pos) = relative_path.rfind('/') {
                                relative_path[..pos].to_string()
                            } else {
                                ".".to_string()
                            };
                            *folders.entry(folder).or_insert(0) += 1;
                        }
                    }
                    let mut folder_list: Vec<(String, usize)> = folders.into_iter().collect();
                    folder_list.sort_by(|a, b| a.0.cmp(&b.0));
                    self.demo_folders = folder_list;
                }
                #[cfg(target_arch = "wasm32")]
                GuiMessage::WebFileParsed {
                    path,
                    file_info,
                    analysis,
                } => {
                    self.add_analysis_to_cache(&file_info, &analysis);
                    self.loading_path = None;
                    self.loading_progress = None;
                    self.loading_elapsed = None;
                    self.loading_eta = None;
                    self.selected_analysis_path = Some(path.clone());
                    self.analyses.insert(path, (file_info, *analysis));
                }
            }
        }

        // Native initial file load logic
        #[cfg(not(target_arch = "wasm32"))]
        if !self.initial_files.is_empty() {
            if let Some(first) = self.initial_files.first() {
                if let Some(parent) = first.parent() {
                    self.root_dir = Some(parent.to_path_buf());
                    self.current_dir = Some(parent.to_path_buf());
                }
            }
            analyze_files_async(ctx.clone(), self.tx.clone(), self.initial_files.clone());
            self.initial_files.clear();
        }

        // Drag & Drop event listener for files
        ctx.input(|i| {
            for file in &i.raw.dropped_files {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(path) = &file.path {
                    let path_str = path.to_string_lossy().into_owned();

                    // Set current_dir to the file's parent folder
                    if let Some(parent) = path.parent() {
                        self.current_dir = Some(parent.to_path_buf());
                    }

                    if self.analyses.contains_key(&path_str) {
                        self.selected_analysis_path = Some(path_str);
                        self.error_message = None;
                    } else {
                        self.selected_analysis_path = None;
                        self.error_message = None;
                        self.loading_path = Some(path_str);
                        self.loading_progress = Some(0.0);
                        self.loading_elapsed = Some(0.0);
                        self.loading_eta = None;
                        analyze_files_async(ctx.clone(), self.tx.clone(), vec![path.clone()]);
                    }
                }

                #[cfg(target_arch = "wasm32")]
                if let Some(bytes) = &file.bytes {
                    let name = file.name.clone();
                    if self.analyses.contains_key(&name) {
                        self.selected_analysis_path = Some(name);
                        self.error_message = None;
                    } else {
                        self.selected_analysis_path = None;
                        self.error_message = None;
                        self.loading_path = Some(name.clone());
                        self.loading_progress = Some(0.0);
                        self.loading_elapsed = Some(0.0);
                        self.loading_eta = None;

                        self.parse_bytes_via_worker(ctx, name, bytes.to_vec());
                    }
                }
            }
        });

        // Top control Panel
        TopBottomPanel::top("controls")
            .frame(Frame::side_top_panel(&ctx.style()).inner_margin(6.))
            .show(ctx, |ui| {
                if modal_open {
                    ui.disable();
                }
                ui.horizontal(|ui| {
                    ui.menu_button(format!("{} ⏷", t("#app_menu_file")), |ui| {
                        #[cfg(target_arch = "wasm32")]
                        if ui.button(t("#app_menu_select_folder")).clicked() {
                            pick_web_folder(ctx.clone(), self.tx.clone());
                            ui.close();
                        }

                        ui.separator();

                        if ui.button(t("#app_menu_clear_cache")).clicked() {
                            self.analyses.clear();
                            self.selected_analysis_path = None;
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                self.subdir_cache.clear();
                                self.explorer_demo_cache.clear();
                                self.demo_folders.clear();
                                self.settings.demo_folder_history.clear();
                                save_settings(&self.settings);
                            }
                            #[cfg(target_arch = "wasm32")]
                            {
                                self.web_files.clear();
                                self.web_tree = None;
                                self.selected_web_folder = ".".to_string();
                                self.demo_folders.clear();
                            }
                            ui.close();
                        }

                        if ui.button(t("#app_menu_preferences")).clicked() {
                            self.show_settings_window = true;
                            ui.close();
                        }

                        #[cfg(not(target_arch = "wasm32"))]
                        if ui.button(t("#app_menu_quit")).clicked() {
                            std::process::exit(0);
                        }
                    });

                    ui.menu_button(format!("{} ⏷", t("#app_menu_help")), |ui| {
                        if ui.button(t("#app_menu_about")).clicked() {
                            self.show_about_window = true;
                            ui.close();
                        }
                    });

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        egui::widgets::global_theme_preference_buttons(ui);
                    });
                });
            });

        #[cfg(target_arch = "wasm32")]
        let mut filtered_web_files = vec![];
        #[cfg(target_arch = "wasm32")]
        {
            let current_selected_web_folder = &self.selected_web_folder;
            for file in &self.web_files {
                let relative_path = &file.path;
                let belongs = if current_selected_web_folder == "." {
                    !relative_path.contains('/')
                } else {
                    if let Some(pos) = relative_path.rfind('/') {
                        &relative_path[..pos] == current_selected_web_folder
                    } else {
                        false
                    }
                };
                if belongs {
                    filtered_web_files.push(file.clone());
                }
            }
        }

        // Shared action hook variables for the event loop
        #[cfg(not(target_arch = "wasm32"))]
        let mut next_dir = None;
        #[cfg(target_arch = "wasm32")]
        let mut temp_web_folder = self.selected_web_folder.clone();
        #[cfg(not(target_arch = "wasm32"))]
        let mut analyze_target_file = None;
        #[cfg(target_arch = "wasm32")]
        let mut parse_file_target = None;

        // Sidebar Explorer panel (Folder Tree only)
        SidePanel::left("explorer_panel")
            .default_width(260.)
            .min_width(200.)
            .max_width(400.)
            .frame(Frame::side_top_panel(&ctx.style()).inner_margin(6.))
            .show(ctx, |ui| {
                if modal_open {
                    ui.disable();
                }
                ui.heading(t("#app_panel_explorer"));
                ui.separator();

                // --- Quick Links Widget ---
                ui.label(egui::RichText::new(t("#app_quick_links")).strong());
                
                // Native Quick Links (Pinned, Recent, Local)
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let root = self.root_dir.clone().or_else(|| self.current_dir.clone());
                    
                    ScrollArea::vertical()
                        .max_height(140.0)
                        .id_salt("quick_links_scroll")
                        .show(ui, |ui| {
                            let mut has_any_links = false;

                            // 1. Pinned Folders
                            if !self.settings.pinned_folders.is_empty() {
                                has_any_links = true;
                                ui.label(egui::RichText::new("📌 Pinned").small().weak());
                                for folder in self.settings.pinned_folders.clone() {
                                    let count = *self.explorer_demo_cache.entry(folder.clone()).or_insert_with(|| {
                                        count_demo_files(&folder)
                                    });

                                    let display_path = if let Some(ref r) = root {
                                        if let Ok(rel) = folder.strip_prefix(r) {
                                            if rel.as_os_str().is_empty() {
                                                ".".to_string()
                                            } else {
                                                rel.to_string_lossy().into_owned()
                                            }
                                        } else {
                                            folder.to_string_lossy().into_owned()
                                        }
                                    } else {
                                        folder.to_string_lossy().into_owned()
                                    };
                                    let display_path = display_path.replace('\\', "/");

                                    ui.horizontal(|ui| {
                                        if ui.selectable_label(true, "📌").on_hover_text("Unpin this folder").clicked() {
                                            self.settings.pinned_folders.retain(|p| p != &folder);
                                            save_settings(&self.settings);
                                        }
                                        if ui.selectable_label(self.current_dir.as_ref() == Some(&folder), format!("{} ({})", display_path, count)).clicked() {
                                            if folder.exists() {
                                                next_dir = Some(folder);
                                            } else {
                                                self.settings.pinned_folders.retain(|p| p != &folder);
                                                self.settings.demo_folder_history.retain(|p| p != &folder);
                                                save_settings(&self.settings);
                                                self.error_message = Some(format!("Directory no longer exists: {}", folder.display()));
                                            }
                                        }
                                    });
                                }
                                ui.add_space(2.0);
                            }

                            // 2. Recent Folders
                            let recent_folders: Vec<PathBuf> = self.settings.demo_folder_history.iter()
                                .filter(|p| !self.settings.pinned_folders.contains(p))
                                .cloned()
                                .collect();

                            if !recent_folders.is_empty() {
                                has_any_links = true;
                                ui.label(egui::RichText::new("🕒 Recent").small().weak());
                                for folder in recent_folders {
                                    let count = *self.explorer_demo_cache.entry(folder.clone()).or_insert_with(|| {
                                        count_demo_files(&folder)
                                    });

                                    let display_path = if let Some(ref r) = root {
                                        if let Ok(rel) = folder.strip_prefix(r) {
                                            if rel.as_os_str().is_empty() {
                                                ".".to_string()
                                            } else {
                                                rel.to_string_lossy().into_owned()
                                            }
                                        } else {
                                            folder.to_string_lossy().into_owned()
                                        }
                                    } else {
                                        folder.to_string_lossy().into_owned()
                                    };
                                    let display_path = display_path.replace('\\', "/");

                                    ui.horizontal(|ui| {
                                        if ui.selectable_label(false, "📌").on_hover_text("Pin this folder").clicked() {
                                            self.settings.pinned_folders.push(folder.clone());
                                            save_settings(&self.settings);
                                        }
                                        if ui.selectable_label(self.current_dir.as_ref() == Some(&folder), format!("{} ({})", display_path, count)).clicked() {
                                            if folder.exists() {
                                                next_dir = Some(folder);
                                            } else {
                                                self.settings.pinned_folders.retain(|p| p != &folder);
                                                self.settings.demo_folder_history.retain(|p| p != &folder);
                                                save_settings(&self.settings);
                                                self.error_message = Some(format!("Directory no longer exists: {}", folder.display()));
                                            }
                                        }
                                    });
                                }
                                ui.add_space(2.0);
                            }

                            // 3. Local Folders
                            let local_folders: Vec<(PathBuf, usize)> = self.demo_folders.iter()
                                .filter(|(p, _)| !self.settings.pinned_folders.contains(p))
                                .cloned()
                                .collect();

                            if !local_folders.is_empty() {
                                has_any_links = true;
                                ui.label(egui::RichText::new("📂 Local").small().weak());
                                for (folder, count) in local_folders {
                                    let display_path = if let Some(ref r) = root {
                                        if let Ok(rel) = folder.strip_prefix(r) {
                                            if rel.as_os_str().is_empty() {
                                                ".".to_string()
                                            } else {
                                                rel.to_string_lossy().into_owned()
                                            }
                                        } else {
                                            folder.to_string_lossy().into_owned()
                                        }
                                    } else {
                                        folder.to_string_lossy().into_owned()
                                    };
                                    let display_path = display_path.replace('\\', "/");

                                    ui.horizontal(|ui| {
                                        if ui.selectable_label(false, "📌").on_hover_text("Pin this folder").clicked() {
                                            self.settings.pinned_folders.push(folder.clone());
                                            save_settings(&self.settings);
                                        }
                                        if ui.selectable_label(self.current_dir.as_ref() == Some(&folder), format!("{} ({})", display_path, count)).clicked() {
                                            if folder.exists() {
                                                next_dir = Some(folder);
                                            } else {
                                                self.settings.pinned_folders.retain(|p| p != &folder);
                                                self.settings.demo_folder_history.retain(|p| p != &folder);
                                                save_settings(&self.settings);
                                                self.error_message = Some(format!("Directory no longer exists: {}", folder.display()));
                                            }
                                        }
                                    });
                                }
                            }

                            if !has_any_links {
                                if self.scanning_demo_folders {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.weak("Scanning workspace...");
                                    });
                                } else {
                                    ui.weak("No demo folders found.");
                                }
                            }
                        });
                }

                // Web Assembly Quick Links
                #[cfg(target_arch = "wasm32")]
                {
                    ScrollArea::vertical()
                        .max_height(140.0)
                        .id_salt("web_quick_links_scroll")
                        .show(ui, |ui| {
                            let mut has_any_links = false;

                            // 1. Pinned Folders (WASM session)
                            if !self.settings.pinned_folders.is_empty() {
                                has_any_links = true;
                                ui.label(egui::RichText::new("📌 Pinned").small().weak());
                                for folder in self.settings.pinned_folders.clone() {
                                    let folder_str = folder.to_string_lossy().into_owned();
                                    let count = self.web_files.iter()
                                        .filter(|f| {
                                            if folder_str == "." {
                                                !f.path.contains('/')
                                            } else {
                                                f.path.starts_with(&format!("{}/", folder_str))
                                            }
                                        })
                                        .count();

                                    ui.horizontal(|ui| {
                                        if ui.selectable_label(true, "📌").clicked() {
                                            self.settings.pinned_folders.retain(|p| p != &folder);
                                        }
                                        if ui.selectable_label(self.selected_web_folder == folder_str, format!("{} ({})", folder_str, count)).clicked() {
                                            temp_web_folder = folder_str;
                                        }
                                    });
                                }
                                ui.add_space(2.0);
                            }

                            // 2. Local Folders (excluding pinned)
                            let local_folders: Vec<(String, usize)> = self.demo_folders.iter()
                                .filter(|(p, _)| !self.settings.pinned_folders.contains(&std::path::PathBuf::from(p)))
                                .cloned()
                                .collect();

                            if !local_folders.is_empty() {
                                has_any_links = true;
                                ui.label(egui::RichText::new("📂 Folders").small().weak());
                                for (folder, count) in local_folders {
                                    let folder_pb = std::path::PathBuf::from(&folder);
                                    ui.horizontal(|ui| {
                                        if ui.selectable_label(false, "📌").clicked() {
                                            self.settings.pinned_folders.push(folder_pb);
                                        }
                                        if ui.selectable_label(self.selected_web_folder == folder, format!("{} ({})", folder, count)).clicked() {
                                            temp_web_folder = folder;
                                        }
                                    });
                                }
                            }

                            if !has_any_links {
                                ui.weak("No demo folders found.");
                            }
                        });
                }
                ui.separator();

                // Native Directory Browser
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ui.horizontal(|ui| {
                        if ui.small_button(t("#app_panel_refresh")).clicked() {
                            self.subdir_cache.clear();
                            self.explorer_demo_cache.clear();
                            self.trigger_dir_scan(ctx);
                            self.trigger_demo_folders_scan(ctx);
                        }
                    });
                    ui.add_space(4.0);

                    ScrollArea::vertical().show(ui, |ui| {
                        let mut cache = std::mem::take(&mut self.subdir_cache);
                        let mut demo_cache = std::mem::take(&mut self.explorer_demo_cache);

                        let collapsing_id = ui.make_persistent_id("this_pc");
                        let mut state =
                            egui::collapsing_header::CollapsingState::load_with_default_open(
                                ui.ctx(),
                                collapsing_id,
                                true,
                            );
                        let is_open = state.is_open();

                        ui.horizontal(|ui| {
                            let symbol = if is_open { "⏷" } else { "⏵" };
                            if ui.selectable_label(false, symbol).clicked() {
                                state.toggle(ui);
                                state.store(ui.ctx());
                            }
                            ui.label(t("#app_this_pc"));
                        });

                        if is_open {
                            ui.indent(ui.make_persistent_id("this_pc_indent"), |ui| {
                                let roots = get_native_roots();
                                for root in roots {
                                    render_native_dir_node(
                                        ui,
                                        &root,
                                        self.current_dir.as_deref(),
                                        &mut next_dir,
                                        &mut cache,
                                        self.settings.scan_folders_for_demos,
                                        &mut demo_cache,
                                    );
                                    ui.add_space(2.0);
                                }
                            });
                        }

                        self.subdir_cache = cache;
                        self.explorer_demo_cache = demo_cache;
                    });
                }

                // Web Assembly Folder Listing
                #[cfg(target_arch = "wasm32")]
                {
                    if let Some(tree) = &self.web_tree {
                        ui.horizontal(|ui| {
                            if ui.small_button(t("#app_menu_select_folder")).clicked() {
                                pick_web_folder(ctx.clone(), self.tx.clone());
                            }
                        });
                        ui.add_space(4.0);

                        ScrollArea::vertical().show(ui, |ui| {
                            render_web_dir_node(ui, tree, &mut temp_web_folder);
                        });
                    } else {
                        ui.label(t("#app_no_folder_loaded"));
                        if ui.button(t("#app_select_demos_folder")).clicked() {
                            pick_web_folder(ctx.clone(), self.tx.clone());
                        }
                    }
                }
            });

        // Demos List Top Panel
        TopBottomPanel::top("demos_list_panel")
            .resizable(true)
            .default_height(220.)
            .min_height(100.)
            .max_height(400.)
            .frame(Frame::side_top_panel(&ctx.style()).inner_margin(6.))
            .show(ctx, |ui| {
                if modal_open {
                    ui.disable();
                }
                ui.heading(t("#app_panel_demos"));
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label(t("#app_filter_search"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter_query)
                            .hint_text("Search name/map...")
                            .desired_width(140.0),
                    );

                    ui.add_space(6.0);
                    ui.label(t("#app_filter_type"));
                    egui::ComboBox::from_id_salt("filter_type_combo")
                        .selected_text(&self.filter_type)
                        .width(60.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.filter_type, "All".to_string(), "All");
                            ui.selectable_value(&mut self.filter_type, "POV".to_string(), "POV");
                            ui.selectable_value(&mut self.filter_type, "HLTV".to_string(), "HLTV");
                        });

                    ui.add_space(6.0);
                    ui.label(t("#app_filter_map"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter_map)
                            .hint_text("de_dust2")
                            .desired_width(90.0),
                    );

                    ui.add_space(6.0);
                    ui.label(t("#app_filter_date_start"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter_date_start)
                            .hint_text("Min Date")
                            .desired_width(80.0),
                    );
                    ui.label("-");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter_date_end)
                            .hint_text("Max Date")
                            .desired_width(80.0),
                    );

                    ui.add_space(10.0);
                    if ui.button(t("#app_filter_reset")).clicked() {
                        self.filter_query.clear();
                        self.filter_type = "All".to_string();
                        self.filter_map.clear();
                        self.filter_date_start.clear();
                        self.filter_date_end.clear();
                    }
                });
                ui.separator();

                #[cfg(not(target_arch = "wasm32"))]
                {
                    if self.current_dir.is_none() {
                        ui.weak(t("#app_please_choose_dir"));
                    } else {
                        if self.scanning_dir && self.desktop_files.is_empty() {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.weak(t("#app_scanning_folder"));
                            });
                        } else {
                            if self.scanning_dir {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.weak(t("#app_updating_folder"));
                                });
                                ui.add_space(4.0);
                            }

                            let selected_path = self.selected_analysis_path.clone();

                            let mut display_files: Vec<DemoListItem> = self.desktop_files.iter()
                                .filter(|item| {
                                    let path_str = item.path.to_string_lossy();
                                    self.filter_demo(&item.name, &item.map_name, &item.date, &path_str)
                                })
                                .cloned()
                                .collect();

                            if let Some(col) = self.sort_column {
                                display_files.sort_by(|a, b| {
                                    let path_a = a.path.to_string_lossy();
                                    let path_b = b.path.to_string_lossy();
                                    let type_a = if let Some((_, analysis)) = self.analyses.get(path_a.as_ref()) {
                                        analysis.demo_info.demo_type.as_str()
                                    } else if let Some(cached) = self.cache.demos.get(path_a.as_ref()) {
                                        cached.demo_type.as_str()
                                    } else if a.name.to_lowercase().contains("hltv") {
                                        "HLTV"
                                    } else {
                                        "POV"
                                    };
                                    let type_b = if let Some((_, analysis)) = self.analyses.get(path_b.as_ref()) {
                                        analysis.demo_info.demo_type.as_str()
                                    } else if let Some(cached) = self.cache.demos.get(path_b.as_ref()) {
                                        cached.demo_type.as_str()
                                    } else if b.name.to_lowercase().contains("hltv") {
                                        "HLTV"
                                    } else {
                                        "POV"
                                    };

                                    let cmp = match col {
                                        SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                                        SortColumn::Type => type_a.cmp(type_b),
                                        SortColumn::Map => a.map_name.to_lowercase().cmp(&b.map_name.to_lowercase()),
                                        SortColumn::Date => a.date.cmp(&b.date),
                                    };

                                    if self.sort_ascending {
                                        cmp
                                    } else {
                                        cmp.reverse()
                                    }
                                });
                            }

                            TableBuilder::new(ui)
                                .striped(true)
                                .cell_layout(Layout::left_to_right(Align::Center))
                                .column(Column::initial(300.0).resizable(true).clip(true)) // Name
                                .column(Column::initial(80.0).resizable(true)) // Type
                                .column(Column::initial(150.0).resizable(true)) // Map
                                .column(Column::initial(150.0)) // Date
                                .header(20.0, |mut header| {
                                    header.col(|ui| {
                                        let label = match (self.sort_column, self.sort_ascending) {
                                            (Some(SortColumn::Name), true) => format!("{} ⏶", t("#app_col_name")),
                                            (Some(SortColumn::Name), false) => format!("{} ⏷", t("#app_col_name")),
                                            _ => t("#app_col_name"),
                                        };
                                        if ui.add(egui::Button::new(label).frame(false)).clicked() {
                                            self.toggle_sort(SortColumn::Name);
                                        }
                                    });
                                    header.col(|ui| {
                                        let label = match (self.sort_column, self.sort_ascending) {
                                            (Some(SortColumn::Type), true) => format!("{} ⏶", t("#app_col_type")),
                                            (Some(SortColumn::Type), false) => format!("{} ⏷", t("#app_col_type")),
                                            _ => t("#app_col_type"),
                                        };
                                        if ui.add(egui::Button::new(label).frame(false)).clicked() {
                                            self.toggle_sort(SortColumn::Type);
                                        }
                                    });
                                    header.col(|ui| {
                                        let label = match (self.sort_column, self.sort_ascending) {
                                            (Some(SortColumn::Map), true) => format!("{} ⏶", t("#app_col_map")),
                                            (Some(SortColumn::Map), false) => format!("{} ⏷", t("#app_col_map")),
                                            _ => t("#app_col_map"),
                                        };
                                        if ui.add(egui::Button::new(label).frame(false)).clicked() {
                                            self.toggle_sort(SortColumn::Map);
                                        }
                                    });
                                    header.col(|ui| {
                                        let label = match (self.sort_column, self.sort_ascending) {
                                            (Some(SortColumn::Date), true) => format!("{} ⏶", t("#app_col_date")),
                                            (Some(SortColumn::Date), false) => format!("{} ⏷", t("#app_col_date")),
                                            _ => t("#app_col_date"),
                                        };
                                        if ui.add(egui::Button::new(label).frame(false)).clicked() {
                                            self.toggle_sort(SortColumn::Date);
                                        }
                                    });
                                })
                                .body(|mut body| {
                                    if self.desktop_files.is_empty() {
                                        body.row(18.0, |mut row| {
                                            row.col(|ui| {
                                                ui.weak(t("#app_no_demos_found"));
                                            });
                                            row.col(|_| {});
                                            row.col(|_| {});
                                            row.col(|_| {});
                                        });
                                    } else if display_files.is_empty() {
                                        body.row(18.0, |mut row| {
                                            row.col(|ui| {
                                                ui.weak(t("#app_no_matching_demos"));
                                            });
                                            row.col(|_| {});
                                            row.col(|_| {});
                                            row.col(|_| {});
                                        });
                                    } else {
                                        for item in &display_files {
                                            let path_str = item.path.to_string_lossy().into_owned();

                                            let is_selected = selected_path.as_ref() == Some(&path_str);
                                            let is_loading =
                                                self.loading_path.as_deref() == Some(path_str.as_str());

                                            body.row(18.0, |mut row| {
                                                row.set_selected(is_selected);
                                                row.col(|ui| {
                                                    ui.horizontal(|ui| {
                                                        if is_loading {
                                                            ui.spinner();
                                                        }
                                                        if ui
                                                            .selectable_label(
                                                                is_selected,
                                                                format!("📄 {}", item.name),
                                                            )
                                                            .clicked()
                                                        {
                                                            if !is_selected {
                                                                analyze_target_file =
                                                                    Some(item.path.clone());
                                                            }
                                                        }
                                                    });
                                                });
                                                row.col(|ui| {
                                                    let demo_type = if let Some((_, analysis)) =
                                                        self.analyses.get(&path_str)
                                                    {
                                                        analysis.demo_info.demo_type.as_str()
                                                    } else if let Some(cached) = self.cache.demos.get(&path_str) {
                                                        cached.demo_type.as_str()
                                                    } else if item.name.to_lowercase().contains("hltv") {
                                                        "HLTV"
                                                    } else {
                                                        "POV"
                                                    };
                                                    ui.label(demo_type);
                                                });
                                                row.col(|ui| {
                                                    ui.label(&item.map_name);
                                                });
                                                row.col(|ui| {
                                                    ui.label(&item.date);
                                                });
                                            });
                                        }
                                    }
                                });
                        }
                    }
                }

                #[cfg(target_arch = "wasm32")]
                {
                    if self.web_tree.is_none() {
                        ui.weak(t("#app_please_select_folder"));
                    } else {
                        let selected_path = self.selected_analysis_path.clone();
                        let loading_path = self.loading_path.clone();

                        let mut display_files: Vec<&WebFile> = filtered_web_files.iter()
                            .filter(|file| {
                                let relative_path = &file.path;
                                let name = &file.name;
                                let analysis_opt = self.analyses.get(relative_path);
                                let map = if let Some((_, analysis)) = analysis_opt {
                                    analysis.demo_info.map_name.as_str()
                                } else if let Some(cached) = self.cache.demos.get(relative_path) {
                                    cached.map_name.as_str()
                                } else {
                                    "-"
                                };
                                let date_str = if let Some((file_info, _)) = analysis_opt {
                                    let duration = file_info.created_at
                                        .duration_since(web_time::SystemTime::UNIX_EPOCH)
                                        .unwrap_or_default();
                                    let secs = duration.as_secs() as i64;
                                    let nsecs = duration.subsec_nanos();
                                    let dt = chrono::DateTime::from_timestamp(secs, nsecs).unwrap_or_default();
                                    dt.format("%Y-%m-%d").to_string()
                                } else if let Some(cached) = self.cache.demos.get(relative_path) {
                                    cached.date.clone()
                                } else {
                                    "-".to_string()
                                };
                                self.filter_demo(name, map, &date_str, relative_path)
                            })
                            .collect();

                        if let Some(col) = self.sort_column {
                            display_files.sort_by(|a, b| {
                                let relative_path_a = &a.path;
                                let relative_path_b = &b.path;
                                let name_a = &a.name;
                                let name_b = &b.name;

                                let analysis_opt_a = self.analyses.get(relative_path_a);
                                let analysis_opt_b = self.analyses.get(relative_path_b);

                                let map_a = if let Some((_, analysis)) = analysis_opt_a {
                                    analysis.demo_info.map_name.as_str()
                                } else if let Some(cached) = self.cache.demos.get(relative_path_a) {
                                    cached.map_name.as_str()
                                } else {
                                    "-"
                                };
                                let map_b = if let Some((_, analysis)) = analysis_opt_b {
                                    analysis.demo_info.map_name.as_str()
                                } else if let Some(cached) = self.cache.demos.get(relative_path_b) {
                                    cached.map_name.as_str()
                                } else {
                                    "-"
                                };

                                let type_a = if let Some((_, analysis)) = analysis_opt_a {
                                    analysis.demo_info.demo_type.as_str()
                                } else if let Some(cached) = self.cache.demos.get(relative_path_a) {
                                    cached.demo_type.as_str()
                                } else if name_a.to_lowercase().contains("hltv") {
                                    "HLTV"
                                } else {
                                    "POV"
                                };
                                let type_b = if let Some((_, analysis)) = analysis_opt_b {
                                    analysis.demo_info.demo_type.as_str()
                                } else if let Some(cached) = self.cache.demos.get(relative_path_b) {
                                    cached.demo_type.as_str()
                                } else if name_b.to_lowercase().contains("hltv") {
                                    "HLTV"
                                } else {
                                    "POV"
                                };

                                let cmp = match col {
                                    SortColumn::Name => name_a.to_lowercase().cmp(&name_b.to_lowercase()),
                                    SortColumn::Type => type_a.cmp(type_b),
                                    SortColumn::Map => map_a.to_lowercase().cmp(&map_b.to_lowercase()),
                                    SortColumn::Date => {
                                        let status_a = if loading_path.as_ref() == Some(relative_path_a) { 0 } else if self.analyses.contains_key(relative_path_a) { 1 } else { 2 };
                                        let status_b = if loading_path.as_ref() == Some(relative_path_b) { 0 } else if self.analyses.contains_key(relative_path_b) { 1 } else { 2 };
                                        status_a.cmp(&status_b)
                                    }
                                };

                                if self.sort_ascending {
                                    cmp
                                } else {
                                    cmp.reverse()
                                }
                            });
                        }

                        TableBuilder::new(ui)
                            .striped(true)
                            .cell_layout(Layout::left_to_right(Align::Center))
                            .column(Column::initial(300.0).resizable(true).clip(true)) // Name
                            .column(Column::initial(80.0).resizable(true)) // Type
                            .column(Column::initial(150.0).resizable(true)) // Map
                            .column(Column::initial(100.0)) // Status
                            .header(20.0, |mut header| {
                                header.col(|ui| {
                                    let label = match (self.sort_column, self.sort_ascending) {
                                        (Some(SortColumn::Name), true) => format!("{} ⏶", t("#app_col_name")),
                                        (Some(SortColumn::Name), false) => format!("{} ⏷", t("#app_col_name")),
                                        _ => t("#app_col_name"),
                                    };
                                    if ui.add(egui::Button::new(label).frame(false)).clicked() {
                                        self.toggle_sort(SortColumn::Name);
                                    }
                                });
                                header.col(|ui| {
                                    let label = match (self.sort_column, self.sort_ascending) {
                                        (Some(SortColumn::Type), true) => format!("{} ⏶", t("#app_col_type")),
                                        (Some(SortColumn::Type), false) => format!("{} ⏷", t("#app_col_type")),
                                        _ => t("#app_col_type"),
                                    };
                                    if ui.add(egui::Button::new(label).frame(false)).clicked() {
                                        self.toggle_sort(SortColumn::Type);
                                    }
                                });
                                header.col(|ui| {
                                    let label = match (self.sort_column, self.sort_ascending) {
                                        (Some(SortColumn::Map), true) => format!("{} ⏶", t("#app_col_map")),
                                        (Some(SortColumn::Map), false) => format!("{} ⏷", t("#app_col_map")),
                                        _ => t("#app_col_map"),
                                    };
                                    if ui.add(egui::Button::new(label).frame(false)).clicked() {
                                        self.toggle_sort(SortColumn::Map);
                                    }
                                });
                                header.col(|ui| {
                                    let label = match (self.sort_column, self.sort_ascending) {
                                        (Some(SortColumn::Date), true) => format!("{} ⏶", t("#app_col_status")),
                                        (Some(SortColumn::Date), false) => format!("{} ⏷", t("#app_col_status")),
                                        _ => t("#app_col_status"),
                                    };
                                    if ui.add(egui::Button::new(label).frame(false)).clicked() {
                                        self.toggle_sort(SortColumn::Date);
                                    }
                                });
                            })
                            .body(|mut body| {
                                if filtered_web_files.is_empty() {
                                    body.row(18.0, |mut row| {
                                        row.col(|ui| {
                                            ui.weak(t("#app_no_demos_found"));
                                        });
                                        row.col(|_| {});
                                        row.col(|_| {});
                                        row.col(|_| {});
                                    });
                                } else if display_files.is_empty() {
                                    body.row(18.0, |mut row| {
                                        row.col(|ui| {
                                            ui.weak(t("#app_no_matching_demos"));
                                        });
                                        row.col(|_| {});
                                        row.col(|_| {});
                                        row.col(|_| {});
                                    });
                                } else {
                                    for file in &display_files {
                                        let relative_path = &file.path;
                                        let name = &file.name;

                                        let analysis_opt = self.analyses.get(relative_path);

                                        let is_selected = selected_path.as_ref() == Some(relative_path);
                                        let is_loaded = analysis_opt.is_some();
                                        let is_loading = loading_path.as_ref() == Some(relative_path);

                                        body.row(18.0, |mut row| {
                                            row.set_selected(is_selected);
                                            row.col(|ui| {
                                                if ui
                                                    .selectable_label(
                                                        is_selected,
                                                        format!("📄 {}", name),
                                                    )
                                                    .clicked()
                                                {
                                                    if !is_selected && !is_loading {
                                                        parse_file_target = Some((*file).clone());
                                                    }
                                                }
                                            });
                                            row.col(|ui| {
                                                let demo_type =
                                                    if let Some((_, analysis)) = analysis_opt {
                                                        analysis.demo_info.demo_type.as_str()
                                                    } else if let Some(cached) = self.cache.demos.get(relative_path) {
                                                        cached.demo_type.as_str()
                                                    } else if name.to_lowercase().contains("hltv") {
                                                        "HLTV"
                                                    } else {
                                                        "POV"
                                                    };
                                                ui.label(demo_type);
                                            });
                                            row.col(|ui| {
                                                let map = if let Some((_, analysis)) = analysis_opt {
                                                    analysis.demo_info.map_name.as_str()
                                                } else if let Some(cached) = self.cache.demos.get(relative_path) {
                                                    cached.map_name.as_str()
                                                } else {
                                                    "-"
                                                };
                                                ui.label(map);
                                            });
                                            row.col(|ui| {
                                                let status = if is_loading {
                                                    t("#app_status_loading")
                                                } else if is_loaded {
                                                    t("#app_status_loaded")
                                                } else {
                                                    t("#app_status_not_loaded")
                                                };
                                                ui.label(status);
                                            });
                                        });
                                    }
                                }
                            });
                    }
                }
            });

        CentralPanel::default().show(ctx, |ui| {
            if modal_open {
                ui.disable();
            }
            if let Some(loading_path) = &self.loading_path {
                let progress = self.loading_progress.unwrap_or(0.0);
                let pct = progress * 100.0;

                let time_str = match (self.loading_elapsed, self.loading_eta) {
                    (Some(elapsed), Some(eta)) => {
                        format!(" (Elapsed: {:.1}s, ETA: {:.1}s)", elapsed, eta)
                    }
                    (Some(elapsed), None) => {
                        format!(" (Elapsed: {:.1}s)", elapsed)
                    }
                    _ => String::new(),
                };

                let filename = std::path::Path::new(loading_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(loading_path);

                let text = if progress == 0.0 {
                    t("#app_progress_reading")
                        .replace("%s1", filename)
                        .replace("%s2", &time_str)
                } else {
                    t("#app_progress_parsing")
                        .replace("%s1", filename)
                        .replace("%s2", &format!("{:.1}%", pct))
                        .replace("%s3", &time_str)
                };

                ui.add(egui::ProgressBar::new(progress).animate(true).text(text));
                ui.add_space(10.0);
            }

            if let Some(error) = &self.error_message {
                ui.centered_and_justified(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(t("#app_error_heading"))
                                .heading()
                                .color(egui::Color32::from_rgb(239, 68, 68)),
                        );
                        ui.add_space(8.0);
                        ui.label(error);
                    });
                });
            } else {
                self.check_scoreboard_cache();
                let show_blank = if let Some(path) = &self.selected_analysis_path {
                    !self.analyses.contains_key(path)
                } else {
                    self.loading_path.is_none()
                };

                if show_blank {
                    ScrollArea::vertical()
                        .id_salt("report_scroll_area")
                        .show(ui, |ui| {
                            report_ui(
                                None,
                                None,
                                &mut self.player_highlight,
                                &mut self.scoreboard_cache,
                                &mut self.chat_cache,
                                &mut self.player_details_cache,
                                ui,
                            );
                        });
                } else if let Some(path) = &self.selected_analysis_path {
                    if let Some((file_info, analysis)) = self.analyses.get(path) {
                        ScrollArea::vertical()
                            .id_salt("report_scroll_area")
                            .show(ui, |ui| {
                                report_ui(
                                    Some(file_info),
                                    Some(analysis),
                                    &mut self.player_highlight,
                                    &mut self.scoreboard_cache,
                                    &mut self.chat_cache,
                                    &mut self.player_details_cache,
                                    ui,
                                );
                            });
                    }
                }
            }
        });

        // Keyboard navigation for the Demos List
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut move_selection = 0;
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                move_selection = 1;
            } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                move_selection = -1;
            }

            if move_selection != 0 && !self.desktop_files.is_empty() {
                let mut current_idx = None;
                let selected_path_str = self.selected_analysis_path.as_deref();
                for (i, f) in self.desktop_files.iter().enumerate() {
                    if Some(f.path.to_string_lossy().as_ref()) == selected_path_str {
                        current_idx = Some(i);
                        break;
                    }
                }

                let new_idx = if let Some(idx) = current_idx {
                    (idx as isize + move_selection)
                        .clamp(0, (self.desktop_files.len() - 1) as isize)
                        as usize
                } else {
                    0
                };

                if current_idx != Some(new_idx) {
                    analyze_target_file = Some(self.desktop_files[new_idx].path.clone());
                }
            }
        }

        // Apply state updates at the end of the frame
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(new_dir) = next_dir {
                self.current_dir = Some(new_dir);
                self.selected_analysis_path = None;
                self.error_message = None;
            }
            if let Some(f) = analyze_target_file {
                let path_str = f.to_string_lossy().into_owned();
                if self.analyses.contains_key(&path_str) {
                    self.selected_analysis_path = Some(path_str);
                    self.error_message = None;
                } else {
                    self.selected_analysis_path = None;
                    self.error_message = None;
                    self.loading_path = Some(path_str);
                    self.loading_progress = Some(0.0);
                    self.loading_elapsed = Some(0.0);
                    self.loading_eta = None;
                    analyze_files_async(ctx.clone(), self.tx.clone(), vec![f]);
                }
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            if temp_web_folder != self.selected_web_folder {
                self.selected_web_folder = temp_web_folder;
                self.selected_analysis_path = None;
                self.error_message = None;
            }
            if let Some(file) = parse_file_target {
                if self.analyses.contains_key(&file.path) {
                    self.selected_analysis_path = Some(file.path);
                    self.error_message = None;
                } else {
                    self.selected_analysis_path = None;
                    self.error_message = None;
                    self.loading_path = Some(file.path.clone());
                    self.loading_progress = Some(0.0);
                    self.loading_elapsed = Some(0.0);
                    self.loading_eta = None;
                    self.parse_web_file(ctx, file);
                }
            }
        }

        if self.show_settings_window {
            let mut open = true;
            let mut close_clicked = false;
            egui::Window::new(t("#app_prefs_title"))
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.heading(t("#app_prefs_general"));
                        ui.add_space(8.0);

                        ui.horizontal(|ui| {
                            ui.label(t("#app_prefs_language"));
                            let mut current_lang = self.settings.language.clone();
                            egui::ComboBox::from_id_salt("language_select")
                                .selected_text(match current_lang.as_str() {
                                    "auto" => t("#app_prefs_lang_auto"),
                                    other => {
                                        let mut chars = other.chars();
                                        match chars.next() {
                                            None => String::new(),
                                            Some(f) => {
                                                f.to_uppercase().collect::<String>()
                                                    + chars.as_str()
                                            }
                                        }
                                    }
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut current_lang,
                                        "auto".to_string(),
                                        t("#app_prefs_lang_auto"),
                                    );
                                    ui.separator();
                                    ui.selectable_value(
                                        &mut current_lang,
                                        "english".to_string(),
                                        "English",
                                    );
                                    ui.selectable_value(
                                        &mut current_lang,
                                        "french".to_string(),
                                        "French",
                                    );
                                    ui.selectable_value(
                                        &mut current_lang,
                                        "german".to_string(),
                                        "German",
                                    );
                                    ui.selectable_value(
                                        &mut current_lang,
                                        "spanish".to_string(),
                                        "Spanish",
                                    );
                                    ui.selectable_value(
                                        &mut current_lang,
                                        "russian".to_string(),
                                        "Russian",
                                    );
                                    ui.selectable_value(
                                        &mut current_lang,
                                        "serbian".to_string(),
                                        "Serbian",
                                    );
                                    ui.selectable_value(
                                        &mut current_lang,
                                        "polish".to_string(),
                                        "Polish",
                                    );
                                    ui.selectable_value(
                                        &mut current_lang,
                                        "turkish".to_string(),
                                        "Turkish",
                                    );
                                });

                            if current_lang != self.settings.language {
                                self.settings.language = current_lang;
                                apply_language_setting(&self.settings.language);
                                save_settings(&self.settings);
                                ctx.request_repaint();
                            }
                        });

                        ui.add_space(8.0);
                        let mut scan_val = self.settings.scan_folders_for_demos;
                        if ui.checkbox(&mut scan_val, t("#app_prefs_scan_folders")).changed() {
                            self.settings.scan_folders_for_demos = scan_val;
                            save_settings(&self.settings);
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                self.subdir_cache.clear();
                                self.explorer_demo_cache.clear();
                            }
                            ctx.request_repaint();
                        }

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.heading("Highlight Capture Settings");
                        ui.add_space(4.0);

                        egui::Grid::new("capture_settings_grid")
                            .num_columns(2)
                            .spacing([8.0, 8.0])
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("Init Commands (startup):");
                                let mut val = self.settings.capture_init_commands.clone();
                                if ui.text_edit_multiline(&mut val).changed() {
                                    self.settings.capture_init_commands = val;
                                    save_settings(&self.settings);
                                }
                                ui.end_row();

                                ui.label("Start Record Commands:");
                                let mut val = self.settings.capture_start_commands.clone();
                                if ui.text_edit_multiline(&mut val).changed() {
                                    self.settings.capture_start_commands = val;
                                    save_settings(&self.settings);
                                }
                                ui.end_row();

                                ui.label("Stop Record Commands:");
                                let mut val = self.settings.capture_stop_commands.clone();
                                if ui.text_edit_multiline(&mut val).changed() {
                                    self.settings.capture_stop_commands = val;
                                    save_settings(&self.settings);
                                }
                                ui.end_row();

                                ui.label("Initial Load Delay (s):");
                                let mut val = self.settings.capture_initial_delay;
                                if ui.add(egui::Slider::new(&mut val, 0.0..=30.0).step_by(0.5)).changed() {
                                    self.settings.capture_initial_delay = val;
                                    save_settings(&self.settings);
                                }
                                ui.end_row();

                                ui.label("Fast-Forward Speed:");
                                let mut val = self.settings.capture_fast_forward_speed;
                                if ui.add(egui::Slider::new(&mut val, 0.01..=5.0).step_by(0.05)).changed() {
                                    self.settings.capture_fast_forward_speed = val;
                                    save_settings(&self.settings);
                                }
                                ui.end_row();

                                ui.label("Pre-Record Buffer (s):");
                                let mut val = self.settings.capture_pre_record_buffer;
                                if ui.add(egui::Slider::new(&mut val, 0.0..=30.0).step_by(0.5)).changed() {
                                    self.settings.capture_pre_record_buffer = val;
                                    save_settings(&self.settings);
                                }
                                ui.end_row();

                                ui.label("Record Start Lead (s):");
                                let mut val = self.settings.capture_record_start_lead;
                                if ui.add(egui::Slider::new(&mut val, 0.0..=10.0).step_by(0.5)).changed() {
                                    self.settings.capture_record_start_lead = val;
                                    save_settings(&self.settings);
                                }
                                ui.end_row();

                                ui.label("Record Stop Trail (s):");
                                let mut val = self.settings.capture_record_stop_trail;
                                if ui.add(egui::Slider::new(&mut val, 0.0..=10.0).step_by(0.5)).changed() {
                                    self.settings.capture_record_stop_trail = val;
                                    save_settings(&self.settings);
                                }
                                ui.end_row();

                                ui.label("Post-Record Buffer (s):");
                                let mut val = self.settings.capture_post_record_buffer;
                                if ui.add(egui::Slider::new(&mut val, 0.0..=30.0).step_by(0.5)).changed() {
                                    self.settings.capture_post_record_buffer = val;
                                    save_settings(&self.settings);
                                }
                                ui.end_row();
                            });

                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button(t("#app_prefs_close")).clicked() {
                                close_clicked = true;
                            }
                        });
                    });
                });
            self.show_settings_window = open && !close_clicked;
        }

        if self.show_about_window {
            let mut open = true;
            let mut close_clicked = false;
            egui::Window::new(t("#app_about_title"))
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.heading("dod-tools");
                        ui.add_space(4.0);
                        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                        ui.add_space(8.0);
                        ui.label(t("#app_about_desc"));
                        ui.add_space(8.0);

                        ui.horizontal(|ui| {
                            ui.label(format!("{}:", t("#app_about_github")));
                            ui.hyperlink_to(
                                "github.com/ccoventry/dod-tools",
                                "https://github.com/ccoventry/dod-tools",
                            );
                        });

                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(8.0);

                        if ui.button(t("#app_about_close")).clicked() {
                            close_clicked = true;
                        }
                    });
                });
            self.show_about_window = open && !close_clicked;
        }
        // Handle export request from Player Details view
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(req) = self.player_details_cache.export_request.take() {
                if let Some(ref active_path_str) = self.selected_analysis_path {
                    if let Some(player_id) = self.player_details_cache.player_id.clone() {
                        let input_path = std::path::PathBuf::from(active_path_str);
                        let stem = input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("patched");
                        let default_name = format!("{}_streak_{:.0}.dem", stem, req.start_time);
                        
                        self.pending_streak_export = Some(PendingStreakExport {
                            input_path,
                            player_id,
                            start_time: req.start_time,
                            stop_time: req.stop_time,
                        });
                        
                        self.capture_export_picker = std::mem::take(&mut self.capture_export_picker)
                            .default_file_name(&default_name);
                        self.capture_export_picker.save_file();
                    }
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn analyze_files_async(ctx: Context, tx: mpsc::Sender<GuiMessage>, paths: Vec<PathBuf>) {
    tokio::task::spawn_blocking(move || {
        tx.send(GuiMessage::AnalyzerStart {
            _files: paths.len(),
        })
        .unwrap();

        for (index, demo_path) in paths.iter().enumerate() {
            let tx_clone = tx.clone();
            let ctx_clone = ctx.clone();
            let path_str = demo_path.to_string_lossy().into_owned();
            let start_time = std::time::SystemTime::now();

            let progress_cb = move |processed: usize, total: usize| {
                if total > 0 {
                    let progress = processed as f32 / total as f32;
                    let elapsed_sec = start_time.elapsed().map(|d| d.as_secs_f32()).unwrap_or(0.0);
                    let eta_sec = if progress > 0.01 {
                        let total_estimated_sec = elapsed_sec / progress;
                        Some(total_estimated_sec - elapsed_sec)
                    } else {
                        None
                    };

                    let _ = tx_clone.send(GuiMessage::DemoParsingProgress {
                        path: path_str.clone(),
                        progress,
                        elapsed_sec,
                        eta_sec,
                    });
                    ctx_clone.request_repaint();
                }
            };

            match run_analyzer_with_progress(demo_path, progress_cb) {
                Ok((file_info, analysis)) => {
                    tx.send(GuiMessage::AnalyzerProgress {
                        file_info,
                        _progress: (index + 1, paths.len()),
                        analysis: Box::new(analysis),
                    })
                    .unwrap();
                }
                Err(e) => {
                    tx.send(GuiMessage::AnalyzerError {
                        path: demo_path.to_string_lossy().into_owned(),
                        error: e,
                    })
                    .unwrap();
                }
            }

            ctx.request_repaint();
        }

        tx.send(GuiMessage::Idle).unwrap();
    });
}
