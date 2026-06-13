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

#[cfg(not(target_arch = "wasm32"))]
use egui_file_dialog::FileDialog;
#[cfg(not(target_arch = "wasm32"))]
use native::run_analyzer_with_progress;

#[cfg(not(target_arch = "wasm32"))]
use explorer::{DemoListItem, get_native_roots, render_native_dir_node, scan_dir_async};
#[cfg(target_arch = "wasm32")]
use explorer::{DirNode, SendWrapper, WebFile, build_web_tree, render_web_dir_node};

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
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "auto".to_string(),
            scan_folders_for_demos: false,
        }
    }
}

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
                return AppSettings {
                    language,
                    scan_folders_for_demos,
                };
            }
        }
    }
    AppSettings::default()
}

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
    let val = serde_json::Value::Object(map);
    if let Ok(content) = serde_json::to_string_pretty(&val) {
        let _ = std::fs::write("settings.json", content);
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

struct Gui {
    analyses: HashMap<String, (FileInfo, Analysis)>,
    selected_analysis_path: Option<String>,
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

    #[cfg(target_arch = "wasm32")]
    web_files: Vec<WebFile>,
    loading_path: Option<String>,
    loading_progress: Option<f32>,
    loading_elapsed: Option<f32>,
    loading_eta: Option<f32>,
    #[cfg(target_arch = "wasm32")]
    selected_web_folder: String,
    #[cfg(target_arch = "wasm32")]
    web_tree: Option<DirNode>,
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
}

impl Default for Gui {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let settings = load_settings();
        apply_language_setting(&settings.language);

        Self {
            analyses: HashMap::default(),
            selected_analysis_path: None,
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

            #[cfg(target_arch = "wasm32")]
            web_files: Vec::default(),
            loading_path: None,
            loading_progress: None,
            loading_elapsed: None,
            loading_eta: None,
            #[cfg(target_arch = "wasm32")]
            selected_web_folder: ".".to_string(),
            #[cfg(target_arch = "wasm32")]
            web_tree: None,
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
fn parse_web_file(ctx: Context, tx: mpsc::Sender<GuiMessage>, web_file: WebFile) {
    wasm_bindgen_futures::spawn_local(async move {
        let file = &web_file.js_file.0;
        let promise = file.array_buffer();
        if let Ok(array_buffer_val) = wasm_bindgen_futures::JsFuture::from(promise).await {
            let array_buffer = js_sys::ArrayBuffer::from(array_buffer_val);
            let uint8_array = js_sys::Uint8Array::new(&array_buffer);
            let bytes = uint8_array.to_vec();

            let tx_clone = tx.clone();
            let ctx_clone = ctx.clone();
            let path_str = web_file.path.clone();
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

            match Analysis::try_from_bytes_with_progress(bytes.as_slice(), progress_cb) {
                Ok(analysis) => {
                    let last_modified_ms = js_sys::Reflect::get(
                        file.as_ref(),
                        &wasm_bindgen::JsValue::from_str("lastModified"),
                    )
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                    let created_at = std::time::SystemTime::UNIX_EPOCH
                        + std::time::Duration::from_millis(last_modified_ms as u64);

                    let size_bytes = file.size() as u64;
                    let file_info = FileInfo {
                        created_at,
                        name: web_file.name.clone(),
                        path: web_file.path.clone(),
                        size_bytes,
                    };

                    tx.send(GuiMessage::WebFileParsed {
                        path: web_file.path.clone(),
                        file_info,
                        analysis: Box::new(analysis),
                    })
                    .ok();
                }
                Err(e) => {
                    tx.send(GuiMessage::AnalyzerError {
                        path: web_file.path.clone(),
                        error: e,
                    })
                    .ok();
                }
            }
            ctx.request_repaint();
        }
    });
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
            }
        }

        // Trigger directory scan if current_dir changed
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.current_dir != self.last_scanned_dir {
                self.last_scanned_dir = self.current_dir.clone();
                self.desktop_files.clear();
                self.trigger_dir_scan(ctx);
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
                    }
                }
                #[cfg(target_arch = "wasm32")]
                GuiMessage::WebFolderLoaded(files) => {
                    self.web_files = files;
                    self.selected_web_folder = ".".to_string();
                    self.web_tree = Some(build_web_tree(&self.web_files));
                    self.selected_analysis_path = None;
                    self.error_message = None;
                }
                #[cfg(target_arch = "wasm32")]
                GuiMessage::WebFileParsed {
                    path,
                    file_info,
                    analysis,
                } => {
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
                        self.error_message = None;
                        let analysis = Analysis::from(&bytes[..]);
                        let file_info = FileInfo {
                            created_at: std::time::SystemTime::UNIX_EPOCH,
                            name: name.clone(),
                            path: name.clone(),
                            size_bytes: bytes.len() as u64,
                        };
                        self.selected_analysis_path = Some(name.clone());
                        self.analyses.insert(name, (file_info, analysis));
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

                // Native Directory Browser
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ui.horizontal(|ui| {
                        if ui.small_button(t("#app_panel_refresh")).clicked() {
                            self.subdir_cache.clear();
                            self.explorer_demo_cache.clear();
                            self.trigger_dir_scan(ctx);
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
                    } else if self.scanning_dir && self.desktop_files.is_empty() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.weak(t("#app_scanning_folder"));
                        });
                    } else if self.desktop_files.is_empty() {
                        ui.weak(t("#app_no_demos_found"));
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
                                } else if a.name.to_lowercase().contains("hltv") {
                                    "HLTV"
                                } else {
                                    "POV"
                                };
                                let type_b = if let Some((_, analysis)) = self.analyses.get(path_b.as_ref()) {
                                    analysis.demo_info.demo_type.as_str()
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
                                    if ui.selectable_label(self.sort_column == Some(SortColumn::Name), label).clicked() {
                                        self.toggle_sort(SortColumn::Name);
                                    }
                                });
                                header.col(|ui| {
                                    let label = match (self.sort_column, self.sort_ascending) {
                                        (Some(SortColumn::Type), true) => format!("{} ⏶", t("#app_col_type")),
                                        (Some(SortColumn::Type), false) => format!("{} ⏷", t("#app_col_type")),
                                        _ => t("#app_col_type"),
                                    };
                                    if ui.selectable_label(self.sort_column == Some(SortColumn::Type), label).clicked() {
                                        self.toggle_sort(SortColumn::Type);
                                    }
                                });
                                header.col(|ui| {
                                    let label = match (self.sort_column, self.sort_ascending) {
                                        (Some(SortColumn::Map), true) => format!("{} ⏶", t("#app_col_map")),
                                        (Some(SortColumn::Map), false) => format!("{} ⏷", t("#app_col_map")),
                                        _ => t("#app_col_map"),
                                    };
                                    if ui.selectable_label(self.sort_column == Some(SortColumn::Map), label).clicked() {
                                        self.toggle_sort(SortColumn::Map);
                                    }
                                });
                                header.col(|ui| {
                                    let label = match (self.sort_column, self.sort_ascending) {
                                        (Some(SortColumn::Date), true) => format!("{} ⏶", t("#app_col_date")),
                                        (Some(SortColumn::Date), false) => format!("{} ⏷", t("#app_col_date")),
                                        _ => t("#app_col_date"),
                                    };
                                    if ui.selectable_label(self.sort_column == Some(SortColumn::Date), label).clicked() {
                                        self.toggle_sort(SortColumn::Date);
                                    }
                                });
                            })
                            .body(|mut body| {
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
                            });
                    }
                }

                #[cfg(target_arch = "wasm32")]
                {
                    if self.web_tree.is_none() {
                        ui.weak(t("#app_please_select_folder"));
                    } else if filtered_web_files.is_empty() {
                        ui.weak(t("#app_no_demos_found"));
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
                                } else {
                                    "-"
                                };
                                let date_str = if let Some((file_info, _)) = analysis_opt {
                                    chrono::DateTime::<chrono::Utc>::from(file_info.created_at)
                                        .format("%Y-%m-%d")
                                        .to_string()
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
                                } else {
                                    "-"
                                };
                                let map_b = if let Some((_, analysis)) = analysis_opt_b {
                                    analysis.demo_info.map_name.as_str()
                                } else {
                                    "-"
                                };

                                let type_a = if let Some((_, analysis)) = analysis_opt_a {
                                    analysis.demo_info.demo_type.as_str()
                                } else if name_a.to_lowercase().contains("hltv") {
                                    "HLTV"
                                } else {
                                    "POV"
                                };
                                let type_b = if let Some((_, analysis)) = analysis_opt_b {
                                    analysis.demo_info.demo_type.as_str()
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
                                    if ui.selectable_label(self.sort_column == Some(SortColumn::Name), label).clicked() {
                                        self.toggle_sort(SortColumn::Name);
                                    }
                                });
                                header.col(|ui| {
                                    let label = match (self.sort_column, self.sort_ascending) {
                                        (Some(SortColumn::Type), true) => format!("{} ⏶", t("#app_col_type")),
                                        (Some(SortColumn::Type), false) => format!("{} ⏷", t("#app_col_type")),
                                        _ => t("#app_col_type"),
                                    };
                                    if ui.selectable_label(self.sort_column == Some(SortColumn::Type), label).clicked() {
                                        self.toggle_sort(SortColumn::Type);
                                    }
                                });
                                header.col(|ui| {
                                    let label = match (self.sort_column, self.sort_ascending) {
                                        (Some(SortColumn::Map), true) => format!("{} ⏶", t("#app_col_map")),
                                        (Some(SortColumn::Map), false) => format!("{} ⏷", t("#app_col_map")),
                                        _ => t("#app_col_map"),
                                    };
                                    if ui.selectable_label(self.sort_column == Some(SortColumn::Map), label).clicked() {
                                        self.toggle_sort(SortColumn::Map);
                                    }
                                });
                                header.col(|ui| {
                                    let label = match (self.sort_column, self.sort_ascending) {
                                        (Some(SortColumn::Date), true) => format!("{} ⏶", t("#app_col_status")),
                                        (Some(SortColumn::Date), false) => format!("{} ⏷", t("#app_col_status")),
                                        _ => t("#app_col_status"),
                                    };
                                    if ui.selectable_label(self.sort_column == Some(SortColumn::Date), label).clicked() {
                                        self.toggle_sort(SortColumn::Date);
                                    }
                                });
                            })
                            .body(|mut body| {
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
                                                    parse_file_target = Some(file.clone());
                                                }
                                            }
                                        });
                                        row.col(|ui| {
                                            let demo_type =
                                                if let Some((_, analysis)) = analysis_opt {
                                                    analysis.demo_info.demo_type.as_str()
                                                } else if name.to_lowercase().contains("hltv") {
                                                    "HLTV"
                                                } else {
                                                    "POV"
                                                };
                                            ui.label(demo_type);
                                        });
                                        row.col(|ui| {
                                            let map = if let Some((_, analysis)) = analysis_opt {
                                                &analysis.demo_info.map_name
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
                let show_blank = if let Some(path) = &self.selected_analysis_path {
                    !self.analyses.contains_key(path)
                } else {
                    self.loading_path.is_none()
                };

                if show_blank {
                    ScrollArea::vertical()
                        .id_salt("report_scroll_area")
                        .show(ui, |ui| {
                            report_ui(None, None, &mut self.player_highlight, ui);
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
                    parse_web_file(ctx.clone(), self.tx.clone(), file);
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
