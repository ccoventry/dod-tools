//! Demo analyzer entry point with an interactive directory browser.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod tree;
mod views;
pub mod types;
pub mod settings;
#[cfg(target_arch = "wasm32")]
pub mod worker;
#[cfg(not(target_arch = "wasm32"))]
pub mod pipeline;
pub mod capture_engine;
pub mod session;

use analysis::Analysis;
use clap::Parser;
use egui::{Align, CentralPanel, Context, Frame, Layout, ScrollArea, SidePanel, TopBottomPanel};
#[cfg(target_arch = "wasm32")]
use egui_extras::{Column, TableBuilder};

use native::FileInfo;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use views::{PlayerHighlighting, report_ui, t};


use types::{
    SortColumn, ScoreboardCache, ChatFilterState, ChatCache,
    CapturePhase, CaptureStudioState, QueuedStreakExport, PlayerDetailsCache, SidebarTab,
    GuiMessage, StartupState,
};
#[cfg(not(target_arch = "wasm32"))]
use types::{AuditorState, PendingStreakExport, BrowserView};

use settings::{AppSettings, load_settings, apply_language_setting};
#[cfg(not(target_arch = "wasm32"))]
use settings::save_settings;

#[cfg(not(target_arch = "wasm32"))]
use pipeline::{analyze_files_async, generate_python_queue_sequencer, start_capture_pipeline};
#[cfg(target_arch = "wasm32")]
use worker::pick_web_folder;

#[cfg(not(target_arch = "wasm32"))]
use egui_file_dialog::FileDialog;
#[cfg(not(target_arch = "wasm32"))]


#[cfg(not(target_arch = "wasm32"))]
use tree::{DemoListItem, get_native_roots, render_native_dir_node, scan_dir_async, scan_demo_folders_async, count_demo_files};
#[cfg(target_arch = "wasm32")]
use tree::{DirNode, WebFile, build_web_tree, render_web_dir_node};
use tree::{DemoCache, CachedDemo};

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
            let args: Vec<PathBuf> = std::env::args()
                .skip(1)
                .map(PathBuf::from)
                .filter(|path| path.extension().unwrap_or_default() == "dem")
                .collect();
            Ok(Box::new(
                Gui::default().with_initial_files(args),
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















#[cfg(not(target_arch = "wasm32"))]
pub use views::browser::VisibleNode;

pub struct Notification {
    pub message: String,
    pub expiration: std::time::Instant,
}

pub(crate) struct Gui {
    pub(crate) analyses: HashMap<String, (FileInfo, Analysis)>,
    pub(crate) selected_analysis_path: Option<String>,
    pub(crate) cache: DemoCache,
    pub notification: Option<Notification>,
    pub(crate) player_highlight: PlayerHighlighting,
    pub(crate) error_message: Option<String>,
    pub(crate) settings: AppSettings,
    pub(crate) draft_settings: AppSettings,
    pub(crate) show_about_window: bool,
    pub(crate) active_sidebar_tab: SidebarTab,

    pub(crate) filter_query: String,
    pub(crate) filter_type: String,
    pub(crate) filter_map: String,
    pub(crate) filter_date_start: String,
    pub(crate) filter_date_end: String,

    pub(crate) sort_column: Option<SortColumn>,
    pub(crate) sort_ascending: bool,

    pub(crate) rx: mpsc::Receiver<GuiMessage>,
    pub(crate) tx: mpsc::Sender<GuiMessage>,

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) file_picker: FileDialog,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) capture_export_picker: FileDialog,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) pending_streak_export: Option<PendingStreakExport>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) pending_loaded_project: Option<Vec<(PathBuf, Vec<crate::session::HighlightMetadata>)>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) root_dir: Option<PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) current_dir: Option<PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) initial_files: Vec<PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) subdir_cache: HashMap<PathBuf, Vec<PathBuf>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) tree_demo_cache: HashMap<PathBuf, usize>,

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) desktop_files: Vec<DemoListItem>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) last_scanned_dir: Option<PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) scanning_dir: bool,

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) demo_folders: Vec<(PathBuf, usize)>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) scanning_demo_folders: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) current_scan_id: usize,

    #[cfg(target_arch = "wasm32")]
    pub(crate) web_files: Vec<WebFile>,
    #[cfg(target_arch = "wasm32")]
    pub(crate) demo_folders: Vec<(String, usize)>,

    pub(crate) loading_path: Option<String>,
    pub(crate) loading_progress: Option<f32>,
    pub(crate) loading_elapsed: Option<f32>,
    pub(crate) loading_eta: Option<f32>,
    #[cfg(target_arch = "wasm32")]
    pub(crate) selected_web_folder: String,
    #[cfg(target_arch = "wasm32")]
    pub(crate) web_tree: Option<DirNode>,
    #[cfg(target_arch = "wasm32")]
    pub(crate) parser_worker: Option<web_sys::Worker>,

    pub(crate) scoreboard_cache: ScoreboardCache,
    pub(crate) chat_cache: ChatCache,
    pub(crate) player_details_cache: PlayerDetailsCache,
    pub(crate) export_queue: Vec<QueuedStreakExport>,
    pub(crate) capture_studio_state: CaptureStudioState,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) batch_export_picker: FileDialog,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) hlcr_state: native::hlcr::HlcrState,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) auditor_state: AuditorState,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) target_folder: String,
    pub(crate) cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) browser_view: BrowserView,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) selection_changed_via_keyboard: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) selected_folder_id: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) visible_nodes: Vec<VisibleNode>,
    pub(crate) capture_engine_running: bool,
    pub(crate) capture_engine_msg: String,
    pub(crate) capture_engine_progress: f32,
    pub(crate) capture_engine_jobs_total: usize,
    pub(crate) capture_engine_jobs_done: usize,
    /// Shared cancellation token threaded into the capture engine thread.
    /// Set to `true` by the Cancel button; reset to `false` on each new launch.
    pub(crate) capture_cancel_token: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(crate) capture_studio_loading: bool,
    pub(crate) last_capture_studio_state: Option<CaptureStudioState>,
    /// Autosave recovery state: set to PendingRecovery on startup when
    /// `.autosave.json` is present (indicates an unclean prior exit).
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) startup_state: StartupState,
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
                let cache_path = native::shared::paths::get_appdata_dir().join(".dod-tools-cache.json");
                let _ = std::fs::write(cache_path, content);
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
            server_ip: None,
            player_roster_hash: None,
            event_signature: None,
            recorder_id: None,
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
        let cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let settings = load_settings();
        apply_language_setting(&settings.language);

        #[cfg(not(target_arch = "wasm32"))]
        let cache = {
            let cache_path = native::shared::paths::get_appdata_dir().join(".dod-tools-cache.json");
            if cache_path.exists() {
                std::fs::read_to_string(&cache_path)
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
            draft_settings: settings.clone(),
            settings,
            show_about_window: false,
            active_sidebar_tab: SidebarTab::Analyzer,

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
            pending_loaded_project: None,
            #[cfg(not(target_arch = "wasm32"))]
            root_dir: std::env::current_dir().ok(),
            #[cfg(not(target_arch = "wasm32"))]
            current_dir: std::env::current_dir().ok(),
            #[cfg(not(target_arch = "wasm32"))]
            initial_files: Vec::default(),
            #[cfg(not(target_arch = "wasm32"))]
            subdir_cache: HashMap::default(),
            #[cfg(not(target_arch = "wasm32"))]
            tree_demo_cache: HashMap::default(),

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
            export_queue: Vec::new(),
            capture_studio_state: CaptureStudioState::Workspace,
            #[cfg(not(target_arch = "wasm32"))]
            batch_export_picker: FileDialog::default(),
            #[cfg(not(target_arch = "wasm32"))]
            hlcr_state: native::hlcr::HlcrState::new(cancel_flag.clone()),
            #[cfg(not(target_arch = "wasm32"))]
            auditor_state: AuditorState::Idle,
            #[cfg(not(target_arch = "wasm32"))]
            target_folder: std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            cancel_flag,
            #[cfg(not(target_arch = "wasm32"))]
            browser_view: BrowserView::Flat,
            #[cfg(not(target_arch = "wasm32"))]
            selection_changed_via_keyboard: false,
            #[cfg(not(target_arch = "wasm32"))]
            selected_folder_id: None,
            #[cfg(not(target_arch = "wasm32"))]
            visible_nodes: Vec::new(),
            capture_engine_running: false,
            capture_engine_msg: String::new(),
            capture_engine_progress: 0.0,
            capture_engine_jobs_total: 0,
            capture_engine_jobs_done: 0,
            capture_cancel_token: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            capture_studio_loading: false,
            last_capture_studio_state: None,
            #[cfg(not(target_arch = "wasm32"))]
            startup_state: {
                let capture_autosave = native::shared::paths::get_appdata_dir().join(".autosave.json");
                let render_autosave = native::shared::paths::get_appdata_dir().join(".render_autosave.json");
                if capture_autosave.exists() {
                    log::warn!("[Startup] .autosave.json detected — capture recovery pending");
                    StartupState::PendingRecovery
                } else if render_autosave.exists() {
                    log::warn!("[Startup] .render_autosave.json detected — render recovery pending");
                    StartupState::PendingRenderRecovery
                } else {
                    StartupState::Normal
                }
            },
            notification: None,
        }
    }
}




impl eframe::App for Gui {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let mut trigger_save = false;
        let mut trigger_save_as = false;
        let mut trigger_new = false;
        let mut trigger_close = false;
        let mut trigger_open = false;

        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::S) {
                if i.modifiers.shift {
                    trigger_save_as = true;
                } else {
                    trigger_save = true;
                }
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::O) {
                trigger_open = true;
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::N) {
                trigger_new = true;
            }
            if i.consume_key(egui::Modifiers::CTRL, egui::Key::W) {
                trigger_close = true;
            }
        });

        #[cfg(not(target_arch = "wasm32"))]
        {
            if trigger_open {
                self.capture_studio_loading = true;
                self.active_sidebar_tab = SidebarTab::CaptureStudio;
                self.capture_studio_state = CaptureStudioState::Workspace;
                let ctx_clone = ctx.clone();
                let rules_clone = crate::views::capture::get_highlight_rules_clone();
                let tx_clone = self.tx.clone();
                let queued_demos_clone = crate::views::capture::get_queued_demos();

                std::thread::Builder::new()
                    .name("rfd_dialog_load_project".into())
                    .stack_size(8 * 1024 * 1024)
                    .spawn(move || {
                        let mut dialog = rfd::FileDialog::new().add_filter("JSON", &["json"]);
                        if let Some(dir) = crate::views::capture::get_default_projects_dir() {
                            dialog = dialog.set_directory(&dir);
                        }
                        if let Some(json_path) = dialog.pick_file() {
                            crate::views::capture::set_active_project_path(Some(json_path.clone()));
                            if let Ok(json) = std::fs::read_to_string(&json_path) {
                                if let Ok(session_data) = serde_json::from_str::<crate::session::SessionData>(&json) {
                                    if let Some(base_dir) = rfd::FileDialog::new().pick_folder() {
                                        let rt = tokio::runtime::Runtime::new().unwrap();
                                        let project_root = json_path.parent().map(|p| p.to_path_buf());
                                        let settings = crate::settings::load_settings();
                                        let last_used = settings.last_demo_dir.clone();
                                        let resolved = rt.block_on(crate::session::import_session_async(
                                            base_dir,
                                            session_data.entries,
                                            project_root,
                                            last_used,
                                        ));
                                        if !resolved.is_empty() {
                                            let mut paths_to_ingest = Vec::new();
                                            {
                                                let guard = match queued_demos_clone.lock() {
                                                    Ok(g) => g,
                                                    Err(p) => p.into_inner(),
                                                };
                                                for (resolved_path, orig_path, _) in &resolved {
                                                    if !guard.iter().any(|d| &d.path == orig_path) {
                                                        paths_to_ingest.push((resolved_path.clone(), orig_path.clone()));
                                                    }
                                                }
                                            }
                                            let resolved_for_msg: Vec<(PathBuf, Vec<crate::session::HighlightMetadata>)> = resolved.iter()
                                                .map(|(_, orig_path, metas)| (orig_path.clone(), metas.clone()))
                                                .collect();
                                            let _ = tx_clone.send(crate::types::GuiMessage::ProjectLoaded(resolved_for_msg));
                                            if !paths_to_ingest.is_empty() {
                                                crate::views::capture::spawn_ingestion_thread(
                                                    crate::views::capture::IngestionInput::Batch(paths_to_ingest),
                                                    rules_clone,
                                                    ctx_clone,
                                                    tx_clone,
                                                );
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        let _ = tx_clone.send(crate::types::GuiMessage::IngestionFinished);
                    })
                    .unwrap();
            }

            if trigger_save || trigger_save_as {
                let arc = crate::views::capture::get_queued_demos();
                let data = match arc.lock() { Ok(g) => g, Err(p) => p.into_inner() }.clone();

                if trigger_save_as || crate::views::capture::get_active_project_path().is_none() {
                    let mut dialog = rfd::FileDialog::new().add_filter("JSON", &["json"]);
                    if let Some(dir) = crate::views::capture::get_default_projects_dir() {
                        dialog = dialog.set_directory(&dir);
                    }
                    if let Some(path) = dialog.save_file() {
                        if let Ok(()) = crate::views::capture::serialize_and_save_project(&path, &*data) {
                            crate::views::capture::set_active_project_path(Some(path));
                            self.notification = Some(Notification {
                                message: "Project saved successfully".to_string(),
                                expiration: std::time::Instant::now() + std::time::Duration::from_secs(3),
                            });
                        }
                    }
                } else if let Some(path) = crate::views::capture::get_active_project_path() {
                    if let Ok(()) = crate::views::capture::serialize_and_save_project(&path, &*data) {
                        self.notification = Some(Notification {
                            message: "Project saved successfully".to_string(),
                            expiration: std::time::Instant::now() + std::time::Duration::from_secs(3),
                        });
                    }
                }
            }

            if trigger_new || trigger_close {
                crate::views::capture::set_active_project_path(None);
                let arc = crate::views::capture::get_queued_demos();
                let mut guard = match arc.lock() { Ok(g) => g, Err(p) => p.into_inner() };
                let queued = std::sync::Arc::make_mut(&mut *guard);
                queued.clear();
                
                if trigger_close {
                    self.capture_studio_state = CaptureStudioState::Workspace;
                }
            }
        }

        let current_state = self.capture_studio_state;
        if self.last_capture_studio_state != Some(current_state) {
            let prev_str = match self.last_capture_studio_state {
                Some(s) => format!("{:?}", s),
                None => "None".to_string(),
            };
            let transition_msg = format!("State Transition: {} -> {:?}", prev_str, current_state);
            log::info!("{}", transition_msg);
            #[cfg(not(target_arch = "wasm32"))]
            crate::views::capture::log_markdown(&transition_msg);
            self.last_capture_studio_state = Some(current_state);
        }

        let modal_open = self.show_about_window;

        // ── Unclean-exit recovery modal ───────────────────────────────────────────
        // Rendered first so it can intercept the frame before any other panel
        // attempts to draw.  The modal is native-only; wasm has no lockfile I/O.
        #[cfg(not(target_arch = "wasm32"))]
        if self.startup_state == StartupState::PendingRecovery {
            let mut recover_clicked = false;
            let mut discard_clicked = false;

            egui::Window::new("⚠ Unclean Exit Detected")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.add_space(4.0);
                    ui.label("The previous session did not exit cleanly.");
                    ui.label("An autosave of the capture queue was found (.autosave.json).");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("🔄 Recover Session").clicked() {
                            recover_clicked = true;
                        }
                        ui.add_space(8.0);
                        if ui.button("🗑 Discard & Clean Up").clicked() {
                            discard_clicked = true;
                        }
                    });
                    ui.add_space(4.0);
                });

            if recover_clicked {
                // Deserialize the lockfile and feed it into the existing import pipeline.
                let autosave_path = native::shared::paths::get_appdata_dir().join(".autosave.json");
                if let Ok(json) = std::fs::read_to_string(&autosave_path) {
                    if let Ok(session_data) = serde_json::from_str::<crate::session::SessionData>(&json) {
                        self.capture_studio_loading = true;
                        self.active_sidebar_tab = SidebarTab::CaptureStudio;
                        self.capture_studio_state = CaptureStudioState::Workspace;
                        let rules = crate::views::capture::get_highlight_rules_clone();
                        let tx_clone = self.tx.clone();
                        let ctx_clone = ctx.clone();
                        let paths: Vec<(std::path::PathBuf, std::path::PathBuf)> = session_data.entries
                            .iter()
                            .map(|e| {
                                let resolved = crate::views::capture::resolve_demo_path(
                                    &e.path.to_string_lossy(),
                                    None,
                                    self.settings.last_demo_dir.as_ref(),
                                );
                                (resolved, e.path.clone())
                            })
                            .collect();
                        crate::views::capture::spawn_ingestion_thread(
                            crate::views::capture::IngestionInput::Batch(paths),
                            rules,
                            ctx_clone,
                            tx_clone,
                        );
                    }
                }
                let _ = std::fs::remove_file(&autosave_path);
                self.startup_state = StartupState::Normal;
            }

            if discard_clicked {
                // GC sweep: remove any stale signal dirs and junction from a crashed batch.
                // Use the same semantics as CaptureCleanupGuard::drop.
                let config = crate::views::capture::get_patcher_config()
                    .lock()
                    .unwrap_or_else(|p| p.into_inner());
                if !config.game_path.is_empty() {
                    if let Some(hl_parent) = std::path::Path::new(&config.game_path).parent() {
                        let exit_trigger = hl_parent.join("DOD_TOOLS_EXIT_TRIGGER");
                        let session_junction = hl_parent.join("dodtools_session");
                        if let Err(e) = std::fs::remove_dir_all(&exit_trigger) {
                            if e.kind() != std::io::ErrorKind::NotFound {
                                log::warn!("[GC::discard] remove exit_trigger {:?}: {}", exit_trigger, e);
                            }
                        }
                        if let Err(e) = std::fs::remove_dir(&session_junction) {
                            if e.kind() != std::io::ErrorKind::NotFound {
                                log::warn!("[GC::discard] remove session_junction {:?}: {}", session_junction, e);
                            }
                        }
                    }
                }
                let autosave_path = native::shared::paths::get_appdata_dir().join(".autosave.json");
                let _ = std::fs::remove_file(&autosave_path);
                self.startup_state = StartupState::Normal;
            }

            // Do not render the rest of the UI while the modal is active.
            return;
        }

        // ── Render batch recovery modal ───────────────────────────────────────
        #[cfg(not(target_arch = "wasm32"))]
        if self.startup_state == StartupState::PendingRenderRecovery {
            let render_autosave_path = native::shared::paths::get_appdata_dir()
                .join(".render_autosave.json");

            let mut recover_clicked = false;
            let mut discard_clicked = false;

            // Read metadata for display while we still hold the path.
            let (pending_count, completed_count, source_folder) = {
                std::fs::read_to_string(&render_autosave_path)
                    .ok()
                    .and_then(|json| {
                        serde_json::from_str::<native::hlcr::RenderSessionData>(&json).ok()
                    })
                    .map(|s| {
                        let pending = s.jobs.iter()
                            .filter(|j| j.status == native::hlcr::RenderJobStatus::Pending)
                            .count();
                        let completed = s.jobs.iter()
                            .filter(|j| j.status == native::hlcr::RenderJobStatus::Completed)
                            .count();
                        (pending, completed, s.source_folder.clone())
                    })
                    .unwrap_or((0, 0, String::new()))
            };

            egui::Window::new("🎬 Render Batch Interrupted")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.add_space(4.0);
                    ui.label("A render batch did not complete cleanly.");
                    ui.label(format!("Source: {}", source_folder));
                    ui.add_space(6.0);
                    ui.label(format!("✅ Completed: {}   ⏳ Pending: {}", completed_count, pending_count));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("🔄 Recover Render Batch").clicked() {
                            recover_clicked = true;
                        }
                        ui.add_space(8.0);
                        if ui.button("🗑 Discard").clicked() {
                            discard_clicked = true;
                        }
                    });
                    ui.add_space(4.0);
                });

            if recover_clicked {
                if let Ok(json) = std::fs::read_to_string(&render_autosave_path) {
                    if let Ok(session) = serde_json::from_str::<native::hlcr::RenderSessionData>(&json) {
                        use std::sync::Arc;
                        use std::sync::atomic::AtomicBool;

                        // Populate HlcrState from the saved session.
                        // Only Pending jobs are set to "Queued"; Completed ones
                        // display as "Finished" in the render table.
                        self.hlcr_state.config.source_folder = session.source_folder.clone();
                        self.hlcr_state.jobs.clear();
                        self.hlcr_state.clips.clear();

                        for (i, rj) in session.jobs.iter().enumerate() {
                            let (status_str, progress) = if rj.status == native::hlcr::RenderJobStatus::Completed {
                                ("Finished".to_string(), 100u32)
                            } else {
                                ("Queued".to_string(), 0u32)
                            };
                            self.hlcr_state.jobs.push(native::hlcr::ui::RenderJobState {
                                id: i.to_string(),
                                name: rj.name.clone(),
                                stream: String::new(),
                                frames: 0,
                                date: String::new(),
                                status: status_str,
                                speed: String::new(),
                                progress,
                                error_log: None,
                                cancel_flag: Arc::new(AtomicBool::new(false)),
                                resolved_output_path: if rj.output_path.is_empty() {
                                    None
                                } else {
                                    Some(rj.output_path.clone())
                                },
                            });
                            // Push a stub ClipData so index alignment is preserved.
                            self.hlcr_state.clips.push(native::hlcr::scanner::ClipData {
                                take_folder: rj.take_folder.clone(),
                                clip_type: "single".to_string(),
                                img_folder: String::new(),
                                wav_file: "sound.wav".to_string(),
                                base_name: rj.name.clone(),
                                frame_count: 0,
                                date: String::new(),
                            });
                        }
                        self.hlcr_state.render_session = Some(session);
                        self.hlcr_state.status_message =
                            format!("Recovered: {} pending, {} completed.",
                                pending_count, completed_count);
                        self.active_sidebar_tab = SidebarTab::CaptureStudio;
                        self.capture_studio_state = CaptureStudioState::Render;
                    }
                }
                let _ = std::fs::remove_file(&render_autosave_path);
                self.startup_state = StartupState::Normal;
            }

            if discard_clicked {
                let _ = std::fs::remove_file(&render_autosave_path);
                self.startup_state = StartupState::Normal;
            }

            return;
        }

        if self.loading_path.is_some() {
            ctx.request_repaint();
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

        #[cfg(not(target_arch = "wasm32"))]
        // Left-most narrow Navigation Sidebar
        SidePanel::left("navigation_sidebar")
            .resizable(false)
            .exact_width(48.0)
            .frame(Frame::side_top_panel(&ctx.style())
                .fill(ctx.style().visuals.extreme_bg_color)
                .inner_margin(egui::Margin::same(4_i8)))
            .show(ctx, |ui| {
                if modal_open {
                    ui.disable();
                }
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);

                    let analyzer_active = self.active_sidebar_tab == SidebarTab::Analyzer;
                    let analyzer_btn = egui::Button::new(egui::RichText::new("🔍").size(18.0))
                        .selected(analyzer_active);
                    if ui.add(analyzer_btn).on_hover_text("Demo Analyzer").clicked() {
                        self.active_sidebar_tab = SidebarTab::Analyzer;
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        ui.add_space(8.0);
                        let auditor_active = self.active_sidebar_tab == SidebarTab::Auditor;
                        let auditor_btn = egui::Button::new(egui::RichText::new("📋").size(18.0))
                            .selected(auditor_active);
                        if ui.add(auditor_btn).on_hover_text("Demo Auditor").clicked() {
                            self.active_sidebar_tab = SidebarTab::Auditor;
                        }
                    }

                    ui.add_space(8.0);

                    let capture_studio_active = self.active_sidebar_tab == SidebarTab::CaptureStudio;
                    let capture_studio_btn = egui::Button::new(egui::RichText::new("🎬").size(18.0))
                        .selected(capture_studio_active);
                    if ui.add(capture_studio_btn).on_hover_text("Capture Studio").clicked() {
                        self.active_sidebar_tab = SidebarTab::CaptureStudio;
                    }

                    ui.add_space(8.0);

                    let settings_active = self.active_sidebar_tab == SidebarTab::Settings;
                    let settings_btn = egui::Button::new(egui::RichText::new("⚙").size(18.0))
                        .selected(settings_active);
                    if ui.add(settings_btn).on_hover_text("Settings").clicked() {
                        self.active_sidebar_tab = SidebarTab::Settings;
                    }
                });
            });

        // Update native file picker
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.file_picker.update(ctx);
            if let Some(path) = self.file_picker.take_picked() {
                self.root_dir = Some(path.clone());
                self.current_dir = Some(path.clone());
                self.settings.last_demo_dir = Some(path);
                crate::settings::save_settings(&self.settings);
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

                            let player_deaths = if let Some(ref analysis) = self.analyses.get(&export_info.input_path.to_string_lossy().into_owned()).map(|(_, a)| a) {
                                if let Some(player) = analysis.state.players.iter().find(|p| p.id == export_info.player_id) {
                                    player.mortality.iter()
                                        .filter(|change| matches!(change.mortality(), analysis::Mortality::Dead))
                                        .map(|change| change.time().real_offset.as_secs_f32())
                                        .collect::<Vec<_>>()
                                } else {
                                    vec![]
                                }
                            } else {
                                vec![]
                            };

                            let config = crate::views::capture::get_patcher_config().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                            let options = native::patch::PatchOptions {
                                exit_on_finish: false,
                                init_commands: config.init_commands.clone(),
                                custom_commands: config.custom_commands.clone(),
                                fast_forward_speed: Some(config.fast_forward_speed),
                                hltv_spec_player,
                                initial_delay: Some(config.initial_delay),
                                pre_record_buffer: Some(config.pre_roll_seconds),
                                record_start_lead: Some(config.record_start_lead),
                                record_stop_trail: Some(config.record_stop_trail),
                                post_record_buffer: Some(config.post_roll_seconds),
                                player_deaths: Some(player_deaths),
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

        // Update batch export folder picker
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.batch_export_picker.update(ctx);
            if let Some(dest_dir) = self.batch_export_picker.take_picked() {
                let enabled_items: Vec<QueuedStreakExport> = self.export_queue.iter()
                    .filter(|item| item.enabled)
                    .cloned()
                    .collect();

                if !enabled_items.is_empty() {
                    let mut queue_info = vec![];
                    let mut has_error = false;

                    let mut file_cache: HashMap<std::path::PathBuf, Result<Vec<u8>, String>> = HashMap::new();

                    for item in &enabled_items {
                        let demo_bytes = file_cache.entry(item.input_path.clone())
                            .or_insert_with(|| {
                                std::fs::read(&item.input_path)
                                    .map_err(|e| format!("Failed to read source demo {}: {}", item.input_path.display(), e))
                            });

                        match demo_bytes {
                            Ok(bytes) => {
                                let player_deaths = if let Some(ref analysis) = self.analyses.get(&item.input_path.to_string_lossy().into_owned()).map(|(_, a)| a) {
                                    if let Some(player) = analysis.state.players.iter().find(|p| p.id == item.player_id) {
                                        player.mortality.iter()
                                            .filter(|change| matches!(change.mortality(), analysis::Mortality::Dead))
                                            .map(|change| change.time().real_offset.as_secs_f32())
                                            .collect::<Vec<_>>()
                                    } else {
                                        vec![]
                                    }
                                } else {
                                    vec![]
                                };

                                let options = native::patch::PatchOptions {
                                    exit_on_finish: item.exit_on_finish,
                                    init_commands: item.init_commands.clone(),
                                    custom_commands: item.custom_commands.clone(),
                                    fast_forward_speed: Some(item.fast_forward_speed),
                                    hltv_spec_player: item.hltv_spec_player.clone(),
                                    initial_delay: Some(item.initial_delay),
                                    pre_record_buffer: Some(item.pre_record_buffer),
                                    record_start_lead: Some(item.record_start_lead),
                                    record_stop_trail: Some(item.record_stop_trail),
                                    post_record_buffer: Some(item.post_record_buffer),
                                    player_deaths: Some(player_deaths),
                                };

                                let intervals = &[(item.start_time, item.stop_time)];
                                match native::patch::patch_demo_highlights(bytes, intervals, &options) {
                                    Ok(patched_bytes) => {
                                        let out_path = dest_dir.join(&item.output_name);
                                        if let Err(e) = std::fs::write(&out_path, patched_bytes) {
                                            self.error_message = Some(format!("Failed to write patched demo: {}", e));
                                            has_error = true;
                                            break;
                                        }
                                        queue_info.push(serde_json::json!({
                                            "demo_path": out_path.to_string_lossy().into_owned(),
                                            "player": item.player_name.clone(),
                                            "streak_index": item.streak_idx,
                                            "kills": item.kills_count,
                                        }));
                                    }
                                    Err(e) => {
                                        self.error_message = Some(format!("Failed to patch demo: {}", e));
                                        has_error = true;
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                self.error_message = Some(e.clone());
                                has_error = true;
                                break;
                            }
                        }
                    }

                    if !has_error && !queue_info.is_empty() {
                        let queue_json_path = dest_dir.join("capture_queue.json");
                        let json_write = std::fs::write(
                            &queue_json_path,
                            serde_json::to_string_pretty(&queue_info).unwrap_or_default(),
                        );
                        if let Err(e) = json_write {
                            self.error_message = Some(format!("Failed to write capture_queue.json: {}", e));
                        }

                        let queue_py_path = dest_dir.join("capture_queue.py");
                        let config = crate::views::capture::get_patcher_config().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                        let py_script = generate_python_queue_sequencer(&config.hlae_path, &config.game_path);
                        if let Err(e) = std::fs::write(&queue_py_path, py_script) {
                            self.error_message = Some(format!("Failed to write capture_queue.py: {}", e));
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

        // Poll AuditorState background scanning if active
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let AuditorState::Scanning { ref rx, ref mut progress_text, ref mut files_found, .. } = self.auditor_state {
                while let Ok(progress_msg) = rx.try_recv() {
                    match progress_msg {
                        hl_demo_auditor::AuditProgress::Scanning(path) => {
                            *progress_text = format!("Scanning: {}", path);
                        }
                        hl_demo_auditor::AuditProgress::Hashing(path) => {
                            *progress_text = format!("Hashing: {}", path);
                        }
                        hl_demo_auditor::AuditProgress::Found(count) => {
                            *files_found = count;
                        }
                        hl_demo_auditor::AuditProgress::Failed(err_str) => {
                            self.auditor_state = AuditorState::Failed(err_str);
                            ctx.request_repaint();
                            break;
                        }
                        hl_demo_auditor::AuditProgress::Done(groups, total_duplicates, wasted_space) => {
                            self.auditor_state = AuditorState::Complete {
                                groups,
                                total: total_duplicates,
                                wasted: wasted_space,
                                expanded: std::collections::HashSet::new(),
                            };
                            ctx.request_repaint();
                            break;
                        }
                    }
                }
            }
        }

        // Handle incoming messages
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                GuiMessage::Idle => {}
                #[cfg(not(target_arch = "wasm32"))]
                GuiMessage::ProjectLoaded(resolved) => {
                    self.pending_loaded_project = Some(resolved);
                }
                GuiMessage::PatchingComplete => {
                    #[cfg(not(target_arch = "wasm32"))]
                    crate::views::capture::set_is_patching(false);
                    self.capture_studio_state = CaptureStudioState::Capture;
                }
                GuiMessage::PreviewPatchingComplete => {
                    #[cfg(not(target_arch = "wasm32"))]
                    crate::views::capture::set_is_patching(false);
                }
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
                #[cfg(not(target_arch = "wasm32"))]
                GuiMessage::CapturePipelineUpdate { item_id, phase, sub_status, debug_command, error } => {
                    if let Some(item) = self.export_queue.iter_mut().find(|i| i.id == item_id) {
                        item.status = phase.clone();
                        item.error_message = error;
                        item.sub_status = sub_status;
                        if debug_command.is_some() {
                            item.debug_command = debug_command;
                        }
                        if phase == CapturePhase::HlaeCapture && item.started_at.is_none() {
                            item.started_at = Some(std::time::Instant::now());
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                GuiMessage::CaptureStudioFinished => {
                    self.capture_studio_state = CaptureStudioState::Render;
                    let mut session_dir = std::path::PathBuf::new();
                    
                    let config_guard = crate::views::capture::get_patcher_config().lock();
                    let mut game_path = String::new();
                    if let Ok(config) = config_guard {
                        game_path = config.game_path.clone();
                        if let Some(primary) = &config.primary_media_dir {
                            session_dir = if config.session_id.is_empty() {
                                primary.clone()
                            } else {
                                primary.join(&config.session_id)
                            };
                        }
                    }

                    if session_dir.as_os_str().is_empty() {
                        if let Some(game_dir) = std::path::Path::new(&game_path).parent() {
                            session_dir = game_dir.join("dod").join("hlcr_captures");
                        }
                    }

                    self.hlcr_state.config.source_folder = session_dir.to_string_lossy().to_string();
                    let _ = native::hlcr::config::save_config(&self.hlcr_state.config);

                    self.hlcr_state.auto_render = true;
                    self.hlcr_state.start_scan();
                }
                GuiMessage::CaptureEngineEvent(event) => {
                    use types::EngineEvent;
                    match event {
                        EngineEvent::Starting(total) => {
                            self.capture_engine_running = true;
                            self.capture_engine_msg = format!("Starting capture sequence of {} jobs...", total);
                            self.capture_engine_progress = 0.0;
                            self.capture_engine_jobs_total = total;
                            self.capture_engine_jobs_done = 0;
                        }
                        EngineEvent::Launching(demo_name) => {
                            self.capture_engine_msg = format!("Launching game to record: {}", demo_name);
                        }
                        EngineEvent::Finished(demo_name) => {
                            self.capture_engine_jobs_done += 1;
                            if self.capture_engine_jobs_total > 0 {
                                self.capture_engine_progress = self.capture_engine_jobs_done as f32 / self.capture_engine_jobs_total as f32;
                            }
                            self.capture_engine_msg = format!("Finished recording: {}", demo_name);
                        }
                        EngineEvent::Verified(demo_name) => {
                            self.capture_engine_msg = format!("Verified recording output for: {}", demo_name);
                        }
                        EngineEvent::Error(err_msg) => {
                            self.capture_engine_msg = format!("Error: {}", err_msg);
                            #[cfg(not(target_arch = "wasm32"))]
                            crate::views::capture::set_is_patching(false);
                        }
                        EngineEvent::AllCompleted => {
                            self.capture_engine_running = false;
                            self.capture_engine_progress = 1.0;
                            self.capture_engine_msg = "All captures completed successfully!".to_string();
                        }
                        EngineEvent::Cancelled => {
                            self.capture_engine_running = false;
                            self.capture_engine_msg = "⛔ Capture cancelled by user.".to_string();
                        }
                    }
                }
                GuiMessage::IngestionFinished => {
                    self.capture_studio_loading = false;
                    #[cfg(not(target_arch = "wasm32"))]
                    let was_loaded = self.pending_loaded_project.is_some();
                    if let Some(resolved) = self.pending_loaded_project.take() {
                        let queued_demos_arc = crate::views::capture::get_queued_demos();
                        let mut guard = match queued_demos_arc.lock() {
                            Ok(g) => g,
                            Err(p) => p.into_inner(),
                        };
                        let queued = std::sync::Arc::make_mut(&mut *guard);
                        for (path, metas) in resolved {
                            if let Some(demo) = queued.iter_mut().find(|d| d.path == path) {
                                for (streak, meta) in demo.streaks.iter_mut().zip(metas) {
                                    streak.is_selected = meta.is_selected;
                                    streak.start_index = meta.start_kill as usize;
                                    streak.end_index = meta.end_kill as usize;
                                    streak.status = meta.status;
                                    streak.notes = meta.notes;
                                    streak.update_visuals();
                                }
                            }
                        }
                    }
                    if was_loaded {
                        self.notification = Some(Notification {
                            message: "Project loaded successfully".to_string(),
                            expiration: std::time::Instant::now() + std::time::Duration::from_secs(3),
                        });
                    }
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
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut capture_scan_paths = Vec::new();
            let mut analyzer_paths = Vec::new();

            ctx.input(|i| {
                for file in &i.raw.dropped_files {
                    if let Some(path) = &file.path {
                        let is_capture_scan = self.active_sidebar_tab == SidebarTab::CaptureStudio
                            && self.capture_studio_state == CaptureStudioState::Workspace;

                        if is_capture_scan {
                            capture_scan_paths.push(path.clone());
                        } else {
                            analyzer_paths.push(path.clone());
                        }
                    }
                }
            });

            if !capture_scan_paths.is_empty() {
                let rules = crate::views::capture::get_highlight_rules_clone();
                let ctx_clone = ctx.clone();
                let tx_clone = self.tx.clone();
                std::thread::Builder::new()
                    .name("drop_ingestion_batch".into())
                    .stack_size(16 * 1024 * 1024)
                    .spawn(move || {
                        crate::views::capture::spawn_ingestion_thread(
                            crate::views::capture::IngestionInput::Batch(capture_scan_paths),
                            rules,
                            ctx_clone,
                            tx_clone,
                        );
                    })
                    .ok();
            }

            for path in analyzer_paths {
                let path_str = path.to_string_lossy().into_owned();
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
        }

        #[cfg(target_arch = "wasm32")]
        ctx.input(|i| {
            for file in &i.raw.dropped_files {
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
                                self.tree_demo_cache.clear();
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
                            self.active_sidebar_tab = SidebarTab::Settings;
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

        #[cfg(not(target_arch = "wasm32"))]
        let is_auditor = self.active_sidebar_tab == SidebarTab::Auditor;
        #[cfg(target_arch = "wasm32")]
        let is_auditor = false;

        if is_auditor {
            #[cfg(not(target_arch = "wasm32"))]
            {
                CentralPanel::default().show(ctx, |ui| {
                    if modal_open {
                        ui.disable();
                    }
                    views::auditor::render(self, ui, ctx);
                });
            }
        } else {
            // Sidebar Explorer panel (Folder Tree only)
        if self.active_sidebar_tab == SidebarTab::Analyzer {
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
                    
                    ScrollArea::both()
                        .max_height(140.0)
                        .id_salt("quick_links_scroll")
                        .show(ui, |ui| {
                            let mut has_any_links = false;

                            // 1. Pinned Folders
                            if !self.settings.pinned_folders.is_empty() {
                                has_any_links = true;
                                ui.label(egui::RichText::new("📌 Pinned").small().weak());
                                for folder in self.settings.pinned_folders.clone() {
                                    let count = *self.tree_demo_cache.entry(folder.clone()).or_insert_with(|| {
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
                                    let count = *self.tree_demo_cache.entry(folder.clone()).or_insert_with(|| {
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
                    ScrollArea::both()
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
                            self.tree_demo_cache.clear();
                            self.trigger_dir_scan(ctx);
                            self.trigger_demo_folders_scan(ctx);
                        }
                    });
                    ui.add_space(4.0);

                    ScrollArea::both().show(ui, |ui| {
                        let mut cache = std::mem::take(&mut self.subdir_cache);
                        let mut demo_cache = std::mem::take(&mut self.tree_demo_cache);

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
                        self.tree_demo_cache = demo_cache;
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

                        ScrollArea::both().show(ui, |ui| {
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
        }

        // Demos List Top Panel
        if self.active_sidebar_tab == SidebarTab::Analyzer {
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
                            }


                            views::browser::browser_ui(ctx, ui, self, &mut analyze_target_file);
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

                        ScrollArea::horizontal().show(ui, |ui| {
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
                        });
                    }
                }
            });
        }

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

            // Error modal — floats above the UI so the user can dismiss it
            // without losing access to any tab or current content.
            if let Some(ref error_text) = self.error_message.clone() {
                {
                    let mut open = true;
                    let is_success = error_text.contains("copied to clipboard");
                    let title = if is_success { "Success" } else { &t("#app_error_heading") };
                    egui::Window::new(title)
                        .collapsible(false)
                        .resizable(false)
                        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                        .open(&mut open)
                        .show(ctx, |ui| {
                            let text_color = if is_success {
                                egui::Color32::from_rgb(34, 197, 94) // Green
                            } else {
                                egui::Color32::from_rgb(239, 68, 68) // Red
                            };
                            ui.label(
                                egui::RichText::new(title)
                                    .color(text_color)
                                    .heading(),
                            );
                            ui.add_space(8.0);
                            ui.label(error_text);
                            ui.add_space(12.0);
                            if ui.button("Dismiss").clicked() {
                                self.error_message = None;
                            }
                        });
                    if !open {
                        self.error_message = None;
                    }
                }
            }
                match self.active_sidebar_tab {
                    SidebarTab::Analyzer => {
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
                                        &mut self.export_queue,
                                        &mut self.settings,
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
                                            &mut self.export_queue,
                                            &mut self.settings,
                                            ui,
                                        );
                                    });
                            }
                        }
                    }
                    SidebarTab::CaptureStudio => {
                        self.capture_studio_ui(ui, ctx);
                    }
                    SidebarTab::Settings => {
                        self.render_settings_ui(ui, ctx);
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    SidebarTab::Auditor => {}
                }
        });
        }

        // Keyboard navigation for the Demos List


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

        // Settings are now drawn inline in the Settings sidebar tab!

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
        // Handle batch export request from Batch Queue view
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.player_details_cache.batch_export_request {
                self.player_details_cache.batch_export_request = false;
                
                let enabled_items: Vec<QueuedStreakExport> = self.export_queue.iter()
                    .filter(|item| item.enabled)
                    .cloned()
                    .collect();

                if !enabled_items.is_empty() {
                    self.capture_studio_state = CaptureStudioState::Capture;

                    let mut player_deaths_map = HashMap::new();
                    for item in &enabled_items {
                        let deaths = if let Some((_, analysis)) = self.analyses.get(&item.input_path.to_string_lossy().into_owned()) {
                            if let Some(player) = analysis.state.players.iter().find(|p| p.id == item.player_id) {
                                player.mortality.iter()
                                    .filter(|change| matches!(change.mortality(), analysis::Mortality::Dead))
                                    .map(|change| change.time().real_offset.as_secs_f32())
                                    .collect::<Vec<f32>>()
                            } else {
                                vec![]
                            }
                        } else {
                            vec![]
                        };
                        player_deaths_map.insert(item.id.clone(), deaths);
                    }

                    self.cancel_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                    let config = crate::views::capture::get_patcher_config().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    start_capture_pipeline(
                        ctx.clone(),
                        self.tx.clone(),
                        enabled_items,
                        player_deaths_map,
                        config.game_path.clone(),
                        config.hlae_path.clone(),
                        self.cancel_flag.clone(),
                    );
                }
            }
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

        // Handle add-to-queue request from Player Details view
        if let Some(req) = self.player_details_cache.add_to_queue_request.take() {
            if let Some(ref active_path_str) = self.selected_analysis_path {
                if let Some(player_id) = self.player_details_cache.player_id.clone() {
                    let input_path = std::path::PathBuf::from(active_path_str);
                    
                    let player_name = if let Some((_, analysis)) = self.analyses.get(active_path_str) {
                        analysis.state.players.iter()
                            .find(|p| p.id == player_id)
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| "player".to_string())
                    } else {
                        "player".to_string()
                    };

                    let clean_player = player_name.replace("-", "_").chars()
                        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect::<String>();
                    let clean_player = if clean_player.is_empty() { "player".to_string() } else { clean_player };

                    let stem = input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("patched");
                    let clean_stem = stem.replace("-", "_").chars()
                        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect::<String>();

                    let default_name = format!("{}_ks_{}_{}.dem", clean_stem, clean_player, req.streak_idx);

                    let hltv_spec_player = if let Some((_, analysis)) = self.analyses.get(active_path_str) {
                        if analysis.demo_info.demo_type == "HLTV" {
                            Some(player_name.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let new_id = format!("{}_{}_{}", active_path_str, player_id, req.streak_idx);

                    if !self.export_queue.iter().any(|item| item.id == new_id) {
                            let config = crate::views::capture::get_patcher_config().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                            self.export_queue.push(QueuedStreakExport {
                                id: new_id,
                                input_path,
                                player_id,
                                player_name,
                                start_time: req.start_time,
                                stop_time: req.stop_time,
                                streak_idx: req.streak_idx,
                                kills_count: req.kills_count,
                                output_name: default_name,
                                enabled: true,
                                exit_on_finish: true,
                                init_commands: config.init_commands.clone(),
                                custom_commands: config.custom_commands.clone(),
                                fast_forward_speed: config.fast_forward_speed,
                                hltv_spec_player,
                                initial_delay: config.initial_delay,
                                pre_record_buffer: config.pre_roll_seconds,
                                record_start_lead: config.record_start_lead,
                                record_stop_trail: config.record_stop_trail,
                                post_record_buffer: config.post_roll_seconds,
                            status: CapturePhase::ReviewQueue,
                            error_message: None,
                            sub_status: None,
                            debug_command: None,
                            started_at: None,
                        });
                    }
                }
            }
        }

        // Render toast notifications if active
        let expired = if let Some(n) = &self.notification {
            std::time::Instant::now() >= n.expiration
        } else {
            false
        };
        if expired {
            self.notification = None;
        }

        if let Some(n) = &self.notification {
            egui::Area::new(egui::Id::new("toast_notification"))
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::Vec2::new(-20.0, -20.0))
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(egui::Color32::from_black_alpha(150))
                        .rounding(4.0)
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(&n.message)
                                    .color(egui::Color32::WHITE)
                            );
                        });
                });
            ctx.request_repaint();
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            self.selection_changed_via_keyboard = false;
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

