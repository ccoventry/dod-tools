use egui::{Align, Color32, Context, Layout, ScrollArea, Ui};
use egui_extras::{Column, TableBuilder};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;

use crate::{AuditorState, Gui};

pub fn render(gui: &mut Gui, ui: &mut Ui, ctx: &Context) {
    egui::TopBottomPanel::bottom("demo_auditor_footer").show_inside(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Duplicates Found: 0 | Wasted Space: 0.00 GB");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Delete Selected Demos").clicked() {
                    // TODO: Wire deletion dispatch
                }
            });
        });
    });

    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.heading("📋 Demo Auditor");
            ui.add_space(10.0);
            ui.weak("(Read-Only duplicate demo scanning tool)");
        });
        ui.separator();

        // Directory selection controls
        ui.horizontal(|ui| {
            ui.label("Target Folder:");
            
            // Text edit showing the current explorer/auditor directory path, fully editable
            ui.add(
                egui::TextEdit::singleline(&mut gui.target_folder)
                    .desired_width(350.0),
            );

            if ui.button("📁 Select Folder").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    gui.target_folder = path.display().to_string();
                }
            }

            // Start auditing button logic
            let is_scanning = matches!(gui.auditor_state, AuditorState::Scanning { .. });
            if ui.add_enabled(!is_scanning && !gui.target_folder.is_empty(), egui::Button::new("🔍 Start Audit")).clicked() {
                let folder_path = PathBuf::from(&gui.target_folder);
                let (tx, rx) = mpsc::channel();
                let cancel = Arc::new(AtomicBool::new(false));
                
                gui.auditor_state = AuditorState::Scanning {
                    rx,
                    cancel: cancel.clone(),
                    progress_text: "Initializing...".to_string(),
                    files_found: 0,
                    last_update: Instant::now(),
                };

                let cancel_clone = cancel;
                let tx_clone = tx;
                let ctx_clone = ctx.clone();
                let folder_path_clone = folder_path.clone();

                std::thread::spawn(move || {
                    if !folder_path_clone.exists() {
                        let _ = tx_clone.send(hl_demo_auditor::AuditProgress::Failed("Selected path does not exist".to_string()));
                        ctx_clone.request_repaint();
                        return;
                    }

                    let (lib_tx, lib_rx) = mpsc::channel();
                    let cancel_for_worker = cancel_clone.clone();
                    let folder_path_for_worker = folder_path_clone.clone();

                    // Spawn worker thread for actual library execution
                    let worker_handle = std::thread::spawn(move || {
                        let mut files = vec![];
                        let lib_tx_opt = Some(lib_tx.clone());
                        hl_demo_auditor::scan_dir(&folder_path_for_worker, &mut files, &cancel_for_worker, &lib_tx_opt);
                        if cancel_for_worker.load(Ordering::Relaxed) {
                            return;
                        }
                        let total_files = files.len();
                        let _ = lib_tx.send(hl_demo_auditor::AuditProgress::Found(total_files));
                        
                        if total_files == 0 {
                            let _ = lib_tx.send(hl_demo_auditor::AuditProgress::Done(vec![], 0, 0));
                            return;
                        }

                        let (_, groups, dup_count, space) = hl_demo_auditor::find_duplicates(files, &cancel_for_worker, &lib_tx_opt);
                        if cancel_for_worker.load(Ordering::Relaxed) {
                            return;
                        }
                        let _ = lib_tx.send(hl_demo_auditor::AuditProgress::Done(groups, dup_count, space));
                    });

                    // Forwarding loop with explicit 50ms repainting throttling math
                    let mut last_repaint = Instant::now();
                    while let Ok(progress_msg) = lib_rx.recv() {
                        let is_done_or_failed = matches!(progress_msg, hl_demo_auditor::AuditProgress::Done(..) | hl_demo_auditor::AuditProgress::Failed(..));
                        let _ = tx_clone.send(progress_msg);

                        if is_done_or_failed || last_repaint.elapsed() > std::time::Duration::from_millis(50) {
                            ctx_clone.request_repaint();
                            last_repaint = Instant::now();
                        }
                    }
                    let _ = worker_handle.join();
                });
            }
        });

        ui.add_space(8.0);

        // Render current auditor state
        match &mut gui.auditor_state {
            AuditorState::Idle => {
                ui.centered_and_justified(|ui| {
                    ui.weak("Choose a folder and click Start Audit to scan for duplicate .dem files.");
                });
            }
            AuditorState::Scanning { progress_text, files_found, cancel, .. } => {
                let mut cancel_clicked = false;
                ui.vertical_centered(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.add_space(8.0);
                    ui.strong(format!("Found {} demo files so far...", files_found));
                    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                        ui.add(egui::Label::new(progress_text.as_str()).truncate());
                    });
                    ui.add_space(8.0);
                    if ui.button("🛑 Cancel Scan").clicked() {
                        cancel.store(true, Ordering::Relaxed);
                        cancel_clicked = true;
                    }
                });
                if cancel_clicked {
                    gui.auditor_state = AuditorState::Idle;
                }
            }
            AuditorState::Failed(err_msg) => {
                ui.colored_label(Color32::from_rgb(239, 68, 68), format!("❌ Audit Failed: {}", err_msg));
            }
            AuditorState::Complete { groups, total, wasted, expanded } => {
                ui.horizontal(|ui| {
                    let size_gb = *wasted as f64 / (1024.0 * 1024.0 * 1024.0);
                    let size_mb = *wasted as f64 / (1024.0 * 1024.0);
                    ui.strong(format!("Duplicates: {}", total));
                    ui.separator();
                    if size_gb >= 1.0 {
                        ui.strong(format!("Wasted Space: {:.2} GB", size_gb));
                    } else {
                        ui.strong(format!("Wasted Space: {:.2} MB", size_mb));
                    }
                    ui.separator();
                    ui.weak(format!("Groups: {}", groups.len()));
                });
                ui.separator();
                // Build a flattened view of the results table
                enum FlatRow {
                    GroupHeader {
                        group_idx: usize,
                        file_count: usize,
                        hash: u64,
                    },
                    FileItem {
                        path: PathBuf,
                        size: u64,
                    },
                }
                
                let mut flat_rows = vec![];
                for (g_idx, group) in groups.iter().enumerate() {
                    flat_rows.push(FlatRow::GroupHeader {
                        group_idx: g_idx,
                        file_count: group.files.len(),
                        hash: group.key.header_hash,
                    });
                    if expanded.contains(&g_idx) {
                        for file in &group.files {
                            flat_rows.push(FlatRow::FileItem {
                                path: file.clone(),
                                size: group.key.size,
                            });
                        }
                    }
                }

                let total_rows = flat_rows.len();

                ScrollArea::horizontal().show(ui, |ui| {
                    TableBuilder::new(ui)
                        .striped(true)
                        .cell_layout(Layout::left_to_right(Align::Center))
                        .column(Column::initial(150.0).resizable(true)) // Status (Group / File)
                        .column(Column::initial(80.0).resizable(true)) // Size
                        .column(Column::initial(450.0).resizable(true).clip(true)) // File Path
                        .column(Column::initial(200.0)) // Action
                        .header(22.0, |mut header| {
                            header.col(|ui| { ui.strong("Status"); });
                            header.col(|ui| { ui.strong("Size"); });
                            header.col(|ui| { ui.strong("File Path"); });
                            header.col(|ui| { ui.strong("Action"); });
                        })
                        .body(|body| {
                            body.rows(20.0, total_rows, |mut row| {
                                let item = &flat_rows[row.index()];

                                match item {
                                    FlatRow::GroupHeader { group_idx, file_count, hash } => {
                                        row.col(|ui| {
                                            let is_expanded = expanded.contains(group_idx);
                                            let icon = if is_expanded { "▼" } else { "▶" };
                                            if ui.button(format!("{} Group ({} files)", icon, file_count)).clicked() {
                                                if is_expanded {
                                                    expanded.remove(group_idx);
                                                } else {
                                                    expanded.insert(*group_idx);
                                                }
                                            }
                                        });

                                        row.col(|ui| {
                                            ui.label("-");
                                        });

                                        row.col(|ui| {
                                            ui.weak(format!("Identical Hash: {:x}", hash));
                                        });

                                        row.col(|ui| {
                                            ui.label("");
                                        });
                                    }
                                    FlatRow::FileItem { path, size } => {
                                        let mut display_path = path.to_string_lossy().into_owned();
                                        if display_path.starts_with(r"\\?\") {
                                            display_path = display_path[4..].to_string();
                                        }

                                        row.col(|ui| {
                                            ui.colored_label(ui.visuals().text_color(), "   ↳ 📄 File");
                                        });

                                        row.col(|ui| {
                                            let size_mb = *size as f64 / (1024.0 * 1024.0);
                                            ui.label(format!("{:.2} MB", size_mb));
                                        });

                                        row.col(|ui| {
                                            ui.label(&display_path);
                                        });

                                        row.col(|ui| {
                                            ui.horizontal(|ui| {
                                                if ui.button("📋 Copy Path").clicked() {
                                                    ctx.copy_text(display_path.clone());
                                                }
                                                if ui.button("📁 Open Folder").clicked() {
                                                    #[cfg(target_os = "windows")]
                                                    {
                                                        let _ = std::process::Command::new("explorer")
                                                            .arg("/select,")
                                                            .arg(path)
                                                            .spawn();
                                                    }
                                                    
                                                    #[cfg(target_os = "macos")]
                                                    {
                                                        let _ = std::process::Command::new("open")
                                                            .arg("-R")
                                                            .arg(path)
                                                            .spawn();
                                                    }
                                                    
                                                    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                                                    {
                                                        // Linux fallback: just open the parent directory
                                                        if let Some(parent) = path.parent() {
                                                            let _ = open::that(parent);
                                                        }
                                                    }
                                                }
                                            });
                                        });
                                    }
                                }
                            });
                        });
                });
            }
        }
    });
}
