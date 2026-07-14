// ============================================================
// views/capture/mod.rs
// Public surface of the Capture Studio UI sub-module.
//
// Exposes:
//   - Shared live statics and their accessors
//   - Public helper functions consumed by capture_studio.rs and main.rs
//   - render_patch_ui dispatcher that delegates to scan/select sub-modules
//
// Dead code removed in Phase 9a audit:
//   - WORKER_STATE, PROGRESS_MSG, PROGRESS_PCT, SUCCESS_STATE, ERROR_STATE (stale pre-async statics)
//   - CaptureWorker, PatchEvent, spawn_patch_batch (superseded by inline patch_worker + mpsc pattern)
//   - MAX_TIME_GAP_STR, get_max_time_gap_str(), max_time_gap UI widget (field exists on HighlightRules
//     but scan_demo_for_highlights uses only life-bounded segmentation — field never read by backend)
// ============================================================

pub mod scan;
pub mod select;
pub mod capture;
pub mod widgets;
pub mod payload;
pub mod panels;

use std::sync::{Arc, Mutex, OnceLock, atomic::{AtomicBool, Ordering}};
use std::path::PathBuf;

pub use native::log_markdown;

fn get_default_projects_dir() -> Option<std::path::PathBuf> {
    dirs::document_dir().map(|mut p| {
        p.push("dod-tools");
        p.push("projects");
        // Ensure the directory exists before the dialog tries to open it
        let _ = std::fs::create_dir_all(&p);
        p
    })
}

macro_rules! acquire_lock {
    ($mutex:expr) => {
        match $mutex.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                log::error!("Mutex poisoned, attempting recovery...");
                poisoned.into_inner()
            }
        }
    };
}
// Export the macro so sub-modules can use it without re-declaring it.
pub(super) use acquire_lock;

use native::patch::{
    PatcherConfig, HighlightRules, scan_demo_for_highlights,
};
use crate::types::{
    QueuedStreakExport, DemoData, HighlightStreak, CaptureStudioState,
};

// ── Live atomic: gates the "Proceed to Capture" button and drives the spinner ──

pub(crate) static IS_PATCHING: AtomicBool = AtomicBool::new(false);

pub(crate) fn is_patching() -> bool {
    IS_PATCHING.load(Ordering::SeqCst)
}

pub(crate) fn set_is_patching(val: bool) {
    IS_PATCHING.store(val, Ordering::SeqCst);
}

pub static ACTIVE_PROJECT_PATH: std::sync::OnceLock<std::sync::Mutex<Option<std::path::PathBuf>>> = std::sync::OnceLock::new();

pub fn get_active_project_path() -> Option<std::path::PathBuf> {
    let mutex = ACTIVE_PROJECT_PATH.get_or_init(|| std::sync::Mutex::new(None));
    let guard = acquire_lock!(mutex);
    guard.clone()
}

pub fn set_active_project_path(path: Option<std::path::PathBuf>) {
    let mutex = ACTIVE_PROJECT_PATH.get_or_init(|| std::sync::Mutex::new(None));
    let mut guard = acquire_lock!(mutex);
    *guard = path;
}

// ── Wizard field string state — only min_kills remains (max_time_gap removed) ──

static MIN_KILLS_STR: OnceLock<Mutex<String>> = OnceLock::new();

fn get_min_kills_str() -> &'static Mutex<String> {
    MIN_KILLS_STR.get_or_init(|| Mutex::new(String::new()))
}

// ── Core ingestion and discovery state ──────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureState {
    Idle,
    Scanning(String),
    Ready,
}

static HIGHLIGHT_RULES: OnceLock<Mutex<HighlightRules>> = OnceLock::new();
static QUEUED_DEMOS: OnceLock<Arc<Mutex<Arc<Vec<DemoData>>>>> = OnceLock::new();
static CAPTURE_STATE: OnceLock<Mutex<CaptureState>> = OnceLock::new();
static PATCHER_CONFIG: OnceLock<Mutex<PatcherConfig>> = OnceLock::new();

fn get_highlight_rules() -> &'static Mutex<HighlightRules> {
    HIGHLIGHT_RULES.get_or_init(|| Mutex::new(HighlightRules {
        min_kills: None,
        // max_time_gap is kept on the struct for future use but not exposed in the UI.
        // The backend scanner uses strictly life-bounded segmentation (DeathMsg / ServerReset).
        max_time_gap: None,
        target_players: Vec::new(),
    }))
}

/// Returns a snapshot clone of the current highlight rules.
/// Used by main.rs drag-drop routing to pass rules to the ingestion thread.
pub(crate) fn get_highlight_rules_clone() -> HighlightRules {
    acquire_lock!(get_highlight_rules()).clone()
}

pub(crate) fn get_queued_demos() -> Arc<Mutex<Arc<Vec<DemoData>>>> {
    QUEUED_DEMOS.get_or_init(|| Arc::new(Mutex::new(Arc::new(Vec::new())))).clone()
}

fn get_capture_state() -> &'static Mutex<CaptureState> {
    CAPTURE_STATE.get_or_init(|| Mutex::new(CaptureState::Idle))
}

pub(crate) fn get_patcher_config() -> &'static Mutex<PatcherConfig> {
    PATCHER_CONFIG.get_or_init(|| {
        Mutex::new(crate::settings::load_patcher_config())
    })
}

static RENDER_CONFIG: OnceLock<Mutex<native::hlcr::config::RenderConfig>> = OnceLock::new();
pub(crate) fn get_render_config() -> &'static Mutex<native::hlcr::config::RenderConfig> {
    RENDER_CONFIG.get_or_init(|| Mutex::new(native::hlcr::config::load_config()))
}

// ── Public dispatcher ────────────────────────────────────────────────────────────
//
// Called by capture_studio.rs. Delegates rendering to the appropriate sub-module
// based on `current_state`. Passing the state value separately from the mutable
// pointer is intentional: the match reads the current state without holding a
// mutable borrow while sub-renderers may write to state_ptr.

pub fn render_patch_ui(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    _export_queue: &mut Vec<QueuedStreakExport>,
    current_state: CaptureStudioState,
    state_ptr: &mut CaptureStudioState,
    tx: std::sync::mpsc::Sender<crate::types::GuiMessage>,
    loading_ptr: &mut bool,
    // Fields forwarded exclusively to the Capture step renderer:
    settings: &mut crate::settings::AppSettings,
    draft_settings: &mut crate::settings::AppSettings,
    error_message: &mut Option<String>,
    subdir_cache: &mut std::collections::HashMap<std::path::PathBuf, Vec<std::path::PathBuf>>,
    tree_demo_cache: &mut std::collections::HashMap<std::path::PathBuf, usize>,
    capture_engine_running: &mut bool,
    engine_msg: &str,
    engine_progress: f32,
    engine_jobs_done: usize,
    engine_jobs_total: usize,
    cancel_token: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    // If the ingestion worker is actively scanning, show a blocking spinner and
    // return early — no other UI should be interactive during file I/O.
    {
        let state = acquire_lock!(get_capture_state()).clone();
        if let CaptureState::Scanning(msg) = state {
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.heading("📂 Step 1: Scan & Discover Highlights");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("{}...", msg));
                    });
                });
            });
            return;
        }
    }

    // Project controls header
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("📂 Load Project").clicked() {
                *loading_ptr = true;
                let ctx_clone = ctx.clone();
                let rules_clone = get_highlight_rules_clone();
                let tx_clone = tx.clone();
                let queued_demos_clone = get_queued_demos();
                std::thread::Builder::new()
                    .name("rfd_dialog_load_project".into())
                    .stack_size(8 * 1024 * 1024)
                    .spawn(move || {
                        let mut dialog = rfd::FileDialog::new().add_filter("JSON", &["json"]);
                        if let Some(dir) = get_default_projects_dir() {
                            dialog = dialog.set_directory(&dir);
                        }
                        if let Some(json_path) = dialog.pick_file() {
                            set_active_project_path(Some(json_path.clone()));
                            if let Ok(json) = std::fs::read_to_string(&json_path) {
                                if let Ok(session_data) = serde_json::from_str::<crate::session::SessionData>(&json) {
                                    if let Some(base_dir) = rfd::FileDialog::new().pick_folder() {
                                        let rt = tokio::runtime::Runtime::new().unwrap();
                                        let resolved = rt.block_on(crate::session::import_session_async(base_dir, session_data.entries));
                                        if !resolved.is_empty() {
                                            let mut paths_to_ingest = Vec::new();
                                            {
                                                let mut guard = acquire_lock!(queued_demos_clone);
                                                let queued = Arc::make_mut(&mut *guard);
                                                for (path, metas) in resolved {
                                                    if let Some(demo) = queued.iter_mut().find(|d| d.path == path) {
                                                        for (streak, meta) in demo.streaks.iter_mut().zip(metas) {
                                                            streak.is_selected = meta.is_selected;
                                                            streak.start_index = meta.start_kill as usize;
                                                            streak.end_index = meta.end_kill as usize;
                                                            streak.status = meta.status;
                                                            streak.update_visuals();
                                                        }
                                                    } else {
                                                        paths_to_ingest.push(path);
                                                    }
                                                }
                                            }
                                            if !paths_to_ingest.is_empty() {
                                                spawn_ingestion_thread(
                                                    IngestionInput::Batch(paths_to_ingest),
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

            let queued_demos_shared = {
                let arc = get_queued_demos();
                let guard = acquire_lock!(arc);
                guard.clone()
            };
            let data = &*queued_demos_shared;

            if ui.button("💾 Save").clicked() {
                if let Some(path) = get_active_project_path() {
                    let _ = serialize_and_save_project(&path, data);
                } else {
                    let mut dialog = rfd::FileDialog::new().add_filter("JSON", &["json"]);
                    if let Some(dir) = get_default_projects_dir() {
                        dialog = dialog.set_directory(&dir);
                    }
                    if let Some(path) = dialog.save_file() {
                        if let Ok(()) = serialize_and_save_project(&path, data) {
                            set_active_project_path(Some(path));
                        }
                    }
                }
            }

            if ui.button("💾 Save As...").clicked() {
                let mut dialog = rfd::FileDialog::new().add_filter("JSON", &["json"]);
                if let Some(dir) = get_default_projects_dir() {
                    dialog = dialog.set_directory(&dir);
                }
                if let Some(path) = dialog.save_file() {
                    if let Ok(()) = serialize_and_save_project(&path, data) {
                        set_active_project_path(Some(path));
                    }
                }
            }

            if let Some(path) = get_active_project_path() {
                ui.weak(format!("Active: {}", path.display()));
            }
        });
    });
    ui.add_space(4.0);

    match current_state {
        CaptureStudioState::Scan => {
            scan::render(
                ui, ctx, state_ptr, tx, loading_ptr,
                get_highlight_rules(),
                get_min_kills_str(),
                get_capture_state(),
                get_queued_demos(),
            );
        }
        CaptureStudioState::Select => {
            select::render(
                ui, ctx, state_ptr, tx, loading_ptr,
                get_highlight_rules(),
                get_queued_demos(),
                get_patcher_config(),
                get_render_config(),
                settings,
                draft_settings,
                error_message,
                subdir_cache,
                tree_demo_cache,
            );
        }
        CaptureStudioState::Capture => {
            capture::render(
                ui,
                ctx,
                capture_engine_running,
                engine_msg,
                engine_progress,
                engine_jobs_done,
                engine_jobs_total,
                tx,
                state_ptr,
                cancel_token,
            );
        }
        _ => {}
    }
}

// ── Ingestion thread ─────────────────────────────────────────────────────────────

pub enum IngestionInput {
    Batch(Vec<PathBuf>),
}

pub(crate) fn spawn_ingestion_thread(
    input: IngestionInput,
    rules: HighlightRules,
    ctx: egui::Context,
    tx: std::sync::mpsc::Sender<crate::types::GuiMessage>,
) {
    {
        let mut state = acquire_lock!(get_capture_state());
        *state = CaptureState::Scanning("Scanning files".to_string());
    }

    log_markdown("STARTING SCAN");

    std::thread::Builder::new()
        .name("ingestion_worker".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut last_repaint = std::time::Instant::now();
            let mut batch_count = 0;
            let files = match input {
                IngestionInput::Batch(paths) => {
                    let mut list = Vec::new();
                    let mut dir_stack = Vec::new();

                    for path in paths {
                        if path.is_dir() {
                            dir_stack.push(path);
                        } else if path.is_file() && path.extension().map(|ext| ext == "dem").unwrap_or(false) {
                            let insert_idx = list.binary_search_by(|p: &PathBuf| p.file_name().unwrap_or_default().cmp(&path.file_name().unwrap_or_default()))
                                .unwrap_or_else(|pos| pos);
                            list.insert(insert_idx, path);
                        }
                    }

                    // Iterative walk — explicit stack avoids OS stack pressure on deep trees.
                    while let Some(dir) = dir_stack.pop() {
                        if let Ok(entries) = std::fs::read_dir(&dir) {
                            for entry in entries.flatten() {
                                let path = entry.path();
                                if path.is_dir() {
                                    dir_stack.push(path);
                                } else if path.is_file()
                                    && path.extension().map(|ext| ext == "dem").unwrap_or(false)
                                {
                                    let insert_idx = list.binary_search_by(|p: &PathBuf| p.file_name().unwrap_or_default().cmp(&path.file_name().unwrap_or_default()))
                                        .unwrap_or_else(|pos| pos);
                                    list.insert(insert_idx, path);
                                }
                            }
                        }
                    }
                    list
                }
            };

            let total_files = files.len();
            for (index, file) in files.into_iter().enumerate() {
                {
                    let mut state = acquire_lock!(get_capture_state());
                    *state = CaptureState::Scanning(format!("demo {} of {}", index + 1, total_files));
                }
                ctx.request_repaint();

                log_markdown(&format!("Attempting Demo {}: {:?}", index + 1, file.file_name().unwrap_or_default()));

                match scan_demo_for_highlights(&file, &rules) {
                    Ok((tickrate, streaks, is_pov, local_player_index, playback_frames)) => {
                        let selectable: Vec<HighlightStreak> = streaks
                            .into_iter()
                            .map(|s| {
                                let target_player = s.target_player.unwrap_or_default();
                                let display_text = format!(
                                    "Player: {} | Kills: {} | Ticks: {} to {}",
                                    target_player, s.kill_count, s.start_tick, s.end_tick
                                );
                                HighlightStreak {
                                    start_tick: s.start_tick,
                                    end_tick: s.end_tick,
                                    kill_count: s.kill_count,
                                    target_player,
                                    is_selected: false,
                                    display_text,
                                    timeline_string: s.timeline_string,
                                    duration_string: s.duration_string,
                                    player_index: s.player_index,
                                    kills: s.kills,
                                    start_index: s.start_index,
                                    end_index: s.end_index,
                                    viewdemo_times: s.viewdemo_times,
                                    frame_times: s.frame_times,
                                    status: crate::session::HighlightStatus::None,
                                }
                            })
                            .collect();

                        if !selectable.is_empty() {
                            let demo_name = file.file_name().unwrap_or_default().to_string_lossy().into_owned();
                            let mut item = DemoData {
                                demo_name,
                                path: file.to_path_buf(),
                                streaks: selectable,
                                tickrate,
                                is_pov,
                                local_player_index,
                                playback_frames,
                            };

                            if item.is_pov {
                                for streak in &mut item.streaks {
                                    if Some(streak.player_index) != item.local_player_index {
                                        streak.is_selected = false;
                                    }
                                }
                            }

                            // Lock is dropped immediately after modifying the collection.
                            let _added = {
                                let queued_arc = get_queued_demos();
                                let mut queued_guard = acquire_lock!(queued_arc);
                                log::info!("Ingestion thread acquired lock to push: {:?}", item.path);
                                if !queued_guard.iter().any(|d| d.path == item.path) {
                                    let queued = Arc::make_mut(&mut *queued_guard);
                                    let insert_idx = queued.binary_search_by(|d| d.demo_name.cmp(&item.demo_name))
                                        .unwrap_or_else(|pos| pos);
                                    queued.insert(insert_idx, item);
                                    true
                                } else {
                                    false
                                }
                            };
                        }
                    }
                    Err(err) => {
                        if err == "Unsupported HLTV proxy demo format" {
                            log_markdown(&format!("- **[WARNING]** Skipped HLTV proxy demo: {:?}", file.file_name().unwrap_or_default()));
                        } else {
                            log_markdown(&format!("- **[WARNING]** Skipped corrupted demo {:?}: {}", file.file_name().unwrap_or_default(), err));
                        }
                    }
                }

                // Batch repaints gate
                batch_count += 1;
                if last_repaint.elapsed() > std::time::Duration::from_millis(16) || batch_count >= 5 {
                    ctx.request_repaint();
                    last_repaint = std::time::Instant::now();
                    batch_count = 0;
                }
            }

            {
                let mut state = acquire_lock!(get_capture_state());
                *state = CaptureState::Ready;
            }
            // Signal the main event loop to clear the loading flag.
            let _ = tx.send(crate::types::GuiMessage::IngestionFinished);
            ctx.request_repaint();
        })
        .unwrap();
}

pub fn serialize_and_save_project(path: &std::path::Path, data: &[crate::types::DemoData]) -> Result<(), String> {
    let entries = data.iter().map(|d| {
        let highlights = d.streaks.iter().map(|s| {
            crate::session::HighlightMetadata {
                is_selected: s.is_selected,
                start_kill: s.start_index as i32,
                end_kill: s.end_index as i32,
                status: s.status,
            }
        }).collect();
        crate::session::DemoEntry {
            path: d.path.clone(),
            key: native::utils::demo_hasher::calculate_demo_key(&d.path),
            highlights,
        }
    }).collect();
    let session_data = crate::session::SessionData { entries };
    let json = serde_json::to_string_pretty(&session_data).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())?;
    Ok(())
}
