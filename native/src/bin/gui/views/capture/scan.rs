// ============================================================
// views/capture/scan.rs
// Renders CaptureStudioState::Scan — Step 1 of the Capture Studio wizard.
//
// Responsibilities:
//   - Min Kills and Target Player Filter rule configuration fields
//   - "Add Demo Files" / "Add Folder" buttons (RFD file picker, background thread)
//   - Queued demo list with per-entry remove buttons
//   - "Proceed to Selection" button with transition logging
//
// Intentionally excluded:
//   - Max Gap (sec) field: backend scanner uses life-bounded segmentation only.
//     The field was confirmed dead in the Phase 9a audit.
// ============================================================

use std::sync::{Arc, Mutex};
use native::patch::HighlightRules;
use crate::types::{DemoData, CaptureStudioState};
use super::{CaptureState, IngestionInput, spawn_ingestion_thread, acquire_lock};
use super::log_markdown;

pub fn render(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state_ptr: &mut CaptureStudioState,
    tx: std::sync::mpsc::Sender<crate::types::GuiMessage>,
    loading_ptr: &mut bool,
    rules_mutex: &'static Mutex<HighlightRules>,
    min_kills_str_mutex: &'static Mutex<String>,
    capture_state_mutex: &'static Mutex<CaptureState>,
    queued_demos_arc: Arc<Mutex<Arc<Vec<DemoData>>>>,
) {
    let mut rules = acquire_lock!(rules_mutex);
    let mut min_kills_str = acquire_lock!(min_kills_str_mutex);

    ui.group(|ui| {
        ui.vertical(|ui| {
            ui.heading("📂 Step 1: Scan & Discover Highlights");
            ui.add_space(4.0);
            ui.label("Configure highlight rules and scan files/folders to discover streaks dynamically.");
            ui.add_space(8.0);

            // ── Rule Configuration ───────────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.label("Min Kills:");
                if ui.text_edit_singleline(&mut *min_kills_str).changed() {
                    let trimmed = min_kills_str.trim().to_string();
                    if trimmed.is_empty() {
                        rules.min_kills = None;
                    } else if let Ok(val) = trimmed.parse::<usize>() {
                        rules.min_kills = Some(val);
                    }
                }
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Target Player Filter (comma-separated):");
                let mut filter_text = rules.target_players.join(", ");
                if ui.text_edit_singleline(&mut filter_text).changed() {
                    rules.target_players = filter_text
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
            });

            ui.add_space(8.0);

            // ── Import Buttons (RFD, spawned on background thread) ───────────────
            ui.horizontal(|ui| {
                let ingesting = matches!(
                    *acquire_lock!(capture_state_mutex),
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

            // ── Queued Demo List ─────────────────────────────────────────────────
            let mut pending_demo_to_remove: Option<usize> = None;
            {
                let queued_guard = acquire_lock!(queued_demos_arc);
                if !queued_guard.is_empty() {
                    ui.add_space(16.0);
                    ui.strong(format!("Queued Demos ({})", queued_guard.len()));
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                        for (index, demo) in queued_guard.iter().enumerate() {
                            ui.horizontal(|ui| {
                                if ui.button("🗑").clicked() {
                                    pending_demo_to_remove = Some(index);
                                }
                                ui.label(&demo.demo_name);
                            });
                        }
                    });
                }
            }

            // Apply deferred removal outside the read-lock scope.
            if let Some(idx) = pending_demo_to_remove {
                let mut queued_guard = acquire_lock!(queued_demos_arc);
                let queued = Arc::make_mut(&mut *queued_guard);
                if idx < queued.len() {
                    queued.remove(idx);
                }
            }

            // ── Proceed Button ───────────────────────────────────────────────────
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if ui.button("Proceed to Selection ->").clicked() {
                    log_markdown("UI Interaction: Clicked Proceed to Selection");
                    let queued_guard = acquire_lock!(queued_demos_arc);
                    let msg = format!("Transitioning with {} items", queued_guard.len());
                    log::info!("{}", msg);
                    log_markdown(&msg);
                    *state_ptr = CaptureStudioState::Select;
                }
            });
        });
    });
}
