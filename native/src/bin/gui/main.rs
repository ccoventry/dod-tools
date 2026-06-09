//! Demo analyzer entry point with an interactive directory browser.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod views;
mod explorer;

use analysis::Analysis;
use clap::Parser;
use egui::{Align, CentralPanel, Context, Frame, Layout, ScrollArea, SidePanel, TopBottomPanel};
use egui_extras::{Column, TableBuilder};
use native::FileInfo;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use views::{report_ui, PlayerHighlighting};

#[cfg(not(target_arch = "wasm32"))]
use native::run_analyzer_with_progress;
#[cfg(not(target_arch = "wasm32"))]
use egui_file_dialog::FileDialog;

#[cfg(target_arch = "wasm32")]
use explorer::{SendWrapper, WebFile, DirNode, build_web_tree, render_web_dir_node};
#[cfg(not(target_arch = "wasm32"))]
use explorer::{DemoListItem, scan_dir_async, get_native_roots, render_native_dir_node};

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

    eframe::run_native(
        "dod-tools",
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
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

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

struct Gui {
    analyses: HashMap<String, (FileInfo, Analysis)>,
    selected_analysis_path: Option<String>,
    player_highlight: PlayerHighlighting,
    error_message: Option<String>,

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

        Self {
            analyses: HashMap::default(),
            selected_analysis_path: None,
            player_highlight: PlayerHighlighting::default(),
            error_message: None,
            rx,
            tx,

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
                    let last_modified_ms = js_sys::Reflect::get(file.as_ref(), &wasm_bindgen::JsValue::from_str("lastModified"))
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let created_at = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(last_modified_ms as u64);

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
                ui.horizontal(|ui| {
                    ui.menu_button("File ⏷", |ui| {
                        #[cfg(not(target_arch = "wasm32"))]
                        if ui.button("Choose Directory...").clicked() {
                            self.file_picker.pick_directory();
                            ui.close();
                        }

                        #[cfg(target_arch = "wasm32")]
                        if ui.button("Select Demos Folder...").clicked() {
                            pick_web_folder(ctx.clone(), self.tx.clone());
                            ui.close();
                        }

                        ui.separator();

                        if ui.button("Clear loaded cache").clicked() {
                            self.analyses.clear();
                            self.selected_analysis_path = None;
                            #[cfg(not(target_arch = "wasm32"))]
                            self.subdir_cache.clear();
                            ui.close();
                        }

                        #[cfg(not(target_arch = "wasm32"))]
                        if ui.button("Quit").clicked() {
                            std::process::exit(0);
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
                ui.heading("Explorer");
                ui.separator();

                // Native Directory Browser
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ui.horizontal(|ui| {
                        if ui.small_button("🔄 Refresh").clicked() {
                            self.subdir_cache.clear();
                            self.trigger_dir_scan(ctx);
                        }
                    });
                    ui.add_space(4.0);

                    ScrollArea::vertical().show(ui, |ui| {
                        let mut cache = std::mem::take(&mut self.subdir_cache);

                        let collapsing_id = ui.make_persistent_id("this_pc");
                        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
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
                            ui.label("💻 This PC");
                        });

                        if is_open {
                            ui.indent(ui.make_persistent_id("this_pc_indent"), |ui| {
                                let roots = get_native_roots();
                                for root in roots {
                                    render_native_dir_node(ui, &root, self.current_dir.as_deref(), &mut next_dir, &mut cache);
                                    ui.add_space(2.0);
                                }
                            });
                        }

                        self.subdir_cache = cache;
                    });
                }

                // Web Assembly Folder Listing
                #[cfg(target_arch = "wasm32")]
                {
                    if let Some(tree) = &self.web_tree {
                        ui.horizontal(|ui| {
                            if ui.small_button("Select Folder...").clicked() {
                                pick_web_folder(ctx.clone(), self.tx.clone());
                            }
                        });
                        ui.add_space(4.0);

                        ScrollArea::vertical().show(ui, |ui| {
                            render_web_dir_node(ui, tree, &mut temp_web_folder);
                        });
                    } else {
                        ui.label("No folder loaded.");
                        if ui.button("Select Demos Folder").clicked() {
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
                ui.heading("Demos");
                ui.separator();

                #[cfg(not(target_arch = "wasm32"))]
                {
                    if self.current_dir.is_none() {
                        ui.weak("Please choose a directory to begin.");
                    } else if self.scanning_dir && self.desktop_files.is_empty() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.weak("Scanning folder...");
                        });
                    } else if self.desktop_files.is_empty() {
                        ui.weak("No demos found in this folder.");
                    } else {
                        if self.scanning_dir {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.weak("Updating folder contents...");
                            });
                            ui.add_space(4.0);
                        }

                        let selected_path = &self.selected_analysis_path;

                        TableBuilder::new(ui)
                            .striped(true)
                            .cell_layout(Layout::left_to_right(Align::Center))
                            .column(Column::initial(300.0).resizable(true).clip(true)) // Name
                            .column(Column::initial(150.0).resizable(true))            // Map
                            .column(Column::initial(150.0))                           // Date
                            .header(20.0, |mut header| {
                                header.col(|ui| { ui.strong("Name"); });
                                header.col(|ui| { ui.strong("Map"); });
                                header.col(|ui| { ui.strong("Date"); });
                            })
                            .body(|mut body| {
                                for item in &self.desktop_files {
                                    let path_str = item.path.to_string_lossy().into_owned();

                                    let is_selected = selected_path.as_ref() == Some(&path_str);
                                    let is_loading = self.loading_path.as_deref() == Some(path_str.as_str());

                                    body.row(18.0, |mut row| {
                                        row.set_selected(is_selected);
                                        row.col(|ui| {
                                            ui.horizontal(|ui| {
                                                if is_loading {
                                                    ui.spinner();
                                                }
                                                if ui.selectable_label(is_selected, format!("📄 {}", item.name)).clicked() {
                                                    if !is_selected {
                                                        analyze_target_file = Some(item.path.clone());
                                                    }
                                                }
                                            });
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
                        ui.weak("Please select a demos folder to begin.");
                    } else if filtered_web_files.is_empty() {
                        ui.weak("No demos found in this folder.");
                    } else {
                        let selected_path = &self.selected_analysis_path;
                        let analyses = &self.analyses;
                        let loading_path = &self.loading_path;

                        TableBuilder::new(ui)
                            .striped(true)
                            .cell_layout(Layout::left_to_right(Align::Center))
                            .column(Column::initial(300.0).resizable(true).clip(true)) // Name
                            .column(Column::initial(150.0).resizable(true))            // Map
                            .column(Column::initial(100.0))                           // Status
                            .header(20.0, |mut header| {
                                header.col(|ui| { ui.strong("Name"); });
                                header.col(|ui| { ui.strong("Map"); });
                                header.col(|ui| { ui.strong("Status"); });
                            })
                            .body(|mut body| {
                                for file in &filtered_web_files {
                                    let relative_path = &file.path;
                                    let name = &file.name;

                                    let is_selected = selected_path.as_ref() == Some(relative_path);
                                    let analysis_opt = analyses.get(relative_path);
                                    let is_loaded = analysis_opt.is_some();
                                    let is_loading = loading_path.as_ref() == Some(relative_path);

                                    body.row(18.0, |mut row| {
                                        row.set_selected(is_selected);
                                        row.col(|ui| {
                                            if ui.selectable_label(is_selected, format!("📄 {}", name)).clicked() {
                                                if !is_selected && !is_loading {
                                                    parse_file_target = Some(file.clone());
                                                }
                                            }
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
                                                "Loading..."
                                            } else if is_loaded {
                                                "Loaded"
                                            } else {
                                                "Not Loaded"
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
                    format!("Reading and preparing {}...{}", filename, time_str)
                } else {
                    format!("Parsing events for {}: {:.1}%{}", filename, pct, time_str)
                };

                ui.add(
                    egui::ProgressBar::new(progress)
                        .animate(true)
                        .text(text),
                );
                ui.add_space(10.0);
            }

            if let Some(error) = &self.error_message {
                ui.centered_and_justified(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("⚠️ Analysis Error").heading().color(egui::Color32::from_rgb(239, 68, 68)));
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
                                report_ui(Some(file_info), Some(analysis), &mut self.player_highlight, ui);
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
                    (idx as isize + move_selection).clamp(0, (self.desktop_files.len() - 1) as isize) as usize
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
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn analyze_files_async(ctx: Context, tx: mpsc::Sender<GuiMessage>, paths: Vec<PathBuf>) {
    tokio::task::spawn_blocking(move || {
        tx.send(GuiMessage::AnalyzerStart { _files: paths.len() })
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

