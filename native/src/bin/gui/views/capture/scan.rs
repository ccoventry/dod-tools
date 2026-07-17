// ============================================================
// views/capture/scan.rs
// Renders the scan sub-view within CaptureStudioState::Workspace.
//
// Responsibilities:
//   - "Add Demo Files" / "Add Folder" buttons (RFD file picker, background thread)
//   - Queued demo list with per-entry remove buttons
//
// Intentionally excluded:
//   - Min Kills and Target Player filter fields: removed in Phase 1 refactor.
//   - Max Gap (sec) field: backend scanner uses life-bounded segmentation only.
//     The field was confirmed dead in the Phase 9a audit.
//   - "Proceed to Selection" button: states merged into unified Workspace.
// ============================================================

use std::sync::{Arc, Mutex};
use native::patch::HighlightRules;
use crate::types::{DemoData, CaptureStudioState};
use super::{CaptureState, IngestionInput, spawn_ingestion_thread};

pub fn render(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    _state_ptr: &mut CaptureStudioState,
    tx: std::sync::mpsc::Sender<crate::types::GuiMessage>,
    loading_ptr: &mut bool,
    rules_mutex: &'static Mutex<HighlightRules>,
    capture_state_mutex: &'static Mutex<CaptureState>,
    queued_demos_arc: Arc<Mutex<Arc<Vec<DemoData>>>>,
) {
    let rules = match rules_mutex.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };

    ui.group(|ui| {
        ui.vertical(|ui| {
            ui.heading("📂 Step 1: Scan & Discover Highlights");
            ui.add_space(4.0);
            ui.label("Configure highlight rules and scan files/folders to discover streaks dynamically.");
            ui.add_space(8.0);

            ui.add_space(8.0);

            // ── Import Buttons (RFD, spawned on background thread) ───────────────
            ui.horizontal(|ui| {
                let ingesting = matches!(
                    *match capture_state_mutex.lock() {
                        Ok(g) => g,
                        Err(p) => p.into_inner(),
                    },
                    CaptureState::Scanning(_)
                );

                if ui.add_enabled(!ingesting, egui::Button::new("➕ Add Demo Files")).clicked() {
                    *loading_ptr = true;
                    let ctx_clone = ctx.clone();
                    let rules_clone = rules.clone();
                    let tx_clone = tx.clone();
                    std::thread::Builder::new()
                        .name("rfd_dialog".into())
                        .stack_size(8 * 1024 * 1024)
                        .spawn(move || {
                            if let Some(files) = rfd::FileDialog::new()
                                .add_filter("Demo files", &["dem"])
                                .pick_files()
                            {
                                spawn_ingestion_thread(
                                    IngestionInput::Batch(files),
                                    rules_clone,
                                    ctx_clone,
                                    tx_clone,
                                );
                            } else {
                                let _ = tx_clone.send(crate::types::GuiMessage::IngestionFinished);
                            }
                        })
                        .unwrap();
                }

                if ui.add_enabled(!ingesting, egui::Button::new("📂 Add Folder")).clicked() {
                    *loading_ptr = true;
                    let ctx_clone = ctx.clone();
                    let rules_clone = rules.clone();
                    let tx_clone = tx.clone();
                    std::thread::Builder::new()
                        .name("rfd_dialog".into())
                        .stack_size(8 * 1024 * 1024)
                        .spawn(move || {
                            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                spawn_ingestion_thread(
                                    IngestionInput::Batch(vec![folder]),
                                    rules_clone,
                                    ctx_clone,
                                    tx_clone,
                                );
                            } else {
                                let _ = tx_clone.send(crate::types::GuiMessage::IngestionFinished);
                            }
                        })
                        .unwrap();
                }


                if ingesting {
                    ui.spinner();
                    ui.weak("Scanning files... (App is responsive)");
                }
            });

            // ── Master List ──────────────────────────────────────────────────────
            let mut pending_demo_to_remove: Option<usize> = None;
            {
                let queued_guard = match queued_demos_arc.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                if !queued_guard.is_empty() {
                    ui.add_space(16.0);
                    ui.strong(format!("Queued Demos ({})", queued_guard.len()));
                    ui.add_space(4.0);
                    
                    let active_idx = *super::get_active_demo_selection().lock().unwrap();
                    let mut clicked_idx: Option<usize> = None;

                    egui_extras::TableBuilder::new(ui)
                        .striped(true)
                        .resizable(true)
                        .sense(egui::Sense::click())
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .column(egui_extras::Column::remainder()) // Demo File
                        .column(egui_extras::Column::initial(80.0)) // Yield
                        .column(egui_extras::Column::initial(100.0)) // Status
                        .header(20.0, |mut header| {
                            header.col(|ui| { ui.strong("Demo File"); });
                            header.col(|ui| { ui.strong("Yield"); });
                            header.col(|ui| { ui.strong("Status"); });
                        })
                        .body(|mut body| {
                            for (index, demo) in queued_guard.iter().enumerate() {
                                let is_selected = active_idx == Some(index);
                                
                                body.row(24.0, |mut row| {
                                    if is_selected {
                                        row.set_selected(true);
                                    }
                                    
                                    row.col(|ui| {
                                        ui.label(&demo.demo_name);
                                    });
                                    row.col(|ui| {
                                        ui.label(format!("{} streaks", demo.streaks.len()));
                                    });
                                    row.col(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label("Scanned");
                                            if ui.button("🗑").clicked() {
                                                pending_demo_to_remove = Some(index);
                                            }
                                        });
                                    });
                                    
                                    if row.response().clicked() {
                                        clicked_idx = Some(index);
                                    }
                                });
                            }
                        });

                    if let Some(idx) = clicked_idx {
                        if let Ok(mut lock) = super::get_active_demo_selection().lock() {
                            *lock = Some(idx);
                        }
                    }
                }
            }

            // Apply deferred removal outside the read-lock scope.
            if let Some(idx) = pending_demo_to_remove {
                let mut queued_guard = match queued_demos_arc.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                let queued = Arc::make_mut(&mut *queued_guard);
                if idx < queued.len() {
                    queued.remove(idx);
                }
            }

        });
    });
}
