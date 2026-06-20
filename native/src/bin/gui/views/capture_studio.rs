use egui::Context;
use crate::Gui;
use crate::types::CaptureStudioState;
#[cfg(not(target_arch = "wasm32"))]
use crate::types::{CapturePhase, QueuedStreakExport};

impl Gui {
    pub fn capture_studio_ui(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        ui.vertical(|ui| {
            ui.add_space(8.0);
            
            // Stepper UI
            ui.horizontal(|ui| {
                ui.heading("🎬 Capture Studio");
                ui.separator();

                let phase = self.capture_studio_state;
                let is_wasm = cfg!(target_arch = "wasm32");

                // Step 1: Queue Review
                let step1_active = phase == CaptureStudioState::ReviewingQueue;
                let step1_btn = ui.selectable_label(step1_active, "1. Queue Review");
                if step1_btn.clicked() {
                    self.capture_studio_state = CaptureStudioState::ReviewingQueue;
                }

                if !is_wasm {
                    ui.label(" ➔ ");
                    let step2_active = phase == CaptureStudioState::Capturing;
                    let step2_btn = ui.selectable_label(step2_active, "2. HLAE Capture");
                    if step2_btn.clicked() {
                        self.capture_studio_state = CaptureStudioState::Capturing;
                    }

                    ui.label(" ➔ ");
                    let step3_active = phase == CaptureStudioState::Rendering;
                    let step3_btn = ui.selectable_label(step3_active, "3. HLCR Render");
                    if step3_btn.clicked() {
                        self.capture_studio_state = CaptureStudioState::Rendering;
                    }

                    ui.label(" ➔ ");
                    let step4_active = phase == CaptureStudioState::Complete;
                    let step4_btn = ui.selectable_label(step4_active, "4. Complete");
                    if step4_btn.clicked() {
                        self.capture_studio_state = CaptureStudioState::Complete;
                    }
                }
            });

            ui.separator();
            ui.add_space(8.0);

            // Sub-views based on CaptureStudioState
            match self.capture_studio_state {
                CaptureStudioState::ReviewingQueue => {
                    crate::views::batch_queue_ui(
                        &mut self.export_queue,
                        &mut self.settings,
                        &mut self.player_details_cache,
                        &self.analyses,
                        ui,
                    );
                }
                CaptureStudioState::Capturing => {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        ui.vertical(|ui| {
                            ui.heading("🎬 HLAE Capture Queue Dashboard");
                            ui.add_space(8.0);

                            let enabled_items: Vec<&QueuedStreakExport> = self.export_queue.iter()
                                .filter(|item| item.enabled)
                                .collect();
                            let total_count = enabled_items.len();
                            let completed_count = enabled_items.iter()
                                .filter(|item| matches!(item.status, CapturePhase::Complete | CapturePhase::Failed))
                                .count();

                            // 1. Overall Progress
                            let progress_fraction = if total_count > 0 {
                                completed_count as f32 / total_count as f32
                            } else {
                                0.0
                            };
                            ui.add(
                                egui::ProgressBar::new(progress_fraction)
                                    .text(format!("{} / {} completed", completed_count, total_count))
                            );
                            ui.add_space(12.0);

                            // 2. Active Item Banner
                            if let Some(active_item) = enabled_items.iter().find(|item| {
                                matches!(item.status, CapturePhase::Patching | CapturePhase::HlaeCapture)
                            }) {
                                egui::Frame::group(ui.style())
                                    .fill(ui.visuals().widgets.noninteractive.bg_fill)
                                    .stroke(egui::Stroke::new(1.0, ui.visuals().widgets.active.bg_stroke.color))
                                    .inner_margin(12.0)
                                    .corner_radius(6.0)
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.spinner();
                                            ui.vertical(|ui| {
                                                ui.strong(format!(
                                                    "Currently Processing: {} (Streak {})",
                                                    active_item.player_name, active_item.streak_idx
                                                ));
                                                let sub_status = match active_item.status {
                                                    CapturePhase::Patching => {
                                                        if let Some(ref sub) = active_item.sub_status {
                                                            format!("Writing patched demo to disk... ({})", sub)
                                                        } else {
                                                            "Writing patched demo to disk...".to_string()
                                                        }
                                                    }
                                                    CapturePhase::HlaeCapture => {
                                                        let mut msg = if let Some(started_at) = active_item.started_at {
                                                            let elapsed = started_at.elapsed().as_secs();
                                                            format!("HLAE Running... (Time elapsed: {} seconds)", elapsed)
                                                        } else {
                                                            "HLAE Running... (Starting...)".to_string()
                                                        };
                                                        if let Some(ref sub) = active_item.sub_status {
                                                            msg = format!("{} [{}]", msg, sub);
                                                        }
                                                        msg
                                                    }
                                                    _ => "Preparing...".to_string(),
                                                };
                                                ui.weak(sub_status);
                                            });
                                        });
                                    });
                            } else if completed_count == total_count && total_count > 0 {
                                egui::Frame::group(ui.style())
                                    .fill(egui::Color32::from_rgba_unmultiplied(34, 197, 94, 30))
                                    .stroke(egui::Stroke::new(1.0, egui::Color32::GREEN))
                                    .inner_margin(12.0)
                                    .corner_radius(6.0)
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label("✅");
                                            ui.vertical(|ui| {
                                                ui.strong("HLAE Capture Sequence Finished!");
                                                ui.weak("Transitioning to rendering phase...");
                                            });
                                        });
                                    });
                            } else {
                                egui::Frame::group(ui.style())
                                    .fill(ui.visuals().widgets.noninteractive.bg_fill)
                                    .stroke(egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color))
                                    .inner_margin(12.0)
                                    .corner_radius(6.0)
                                    .show(ui, |ui| {
                                        ui.weak("Waiting to begin capture sequence...");
                                    });
                            }
                            ui.add_space(12.0);

                            ui.horizontal(|ui| {
                                if ui.button(egui::RichText::new("🛑 Abort Capture Queue").color(egui::Color32::RED)).clicked() {
                                    self.cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                                    self.capture_studio_state = CaptureStudioState::ReviewingQueue;
                                }
                            });
                            ui.add_space(12.0);

                            // 3. Queue History & Error Reporting
                            ui.strong("Queue Status List");
                            ui.add_space(4.0);

                            if self.export_queue.is_empty() {
                                ui.label("The queue is empty.");
                            } else {
                                egui::ScrollArea::vertical()
                                    .id_salt("hlae_capture_dashboard_scroll")
                                    .show(ui, |ui| {
                                        for item in &self.export_queue {
                                            if !item.enabled {
                                                continue;
                                            }
                                            ui.group(|ui| {
                                                ui.horizontal(|ui| {
                                                    let (icon, color) = match item.status {
                                                        CapturePhase::Complete => ("✅", egui::Color32::GREEN),
                                                        CapturePhase::Failed => ("❌", egui::Color32::RED),
                                                        CapturePhase::Patching | CapturePhase::HlaeCapture => ("⏳", egui::Color32::LIGHT_BLUE),
                                                        _ => ("🕒", egui::Color32::GRAY),
                                                    };
                                                    ui.colored_label(color, icon);
                                                    
                                                    ui.strong(&item.player_name);
                                                    ui.weak(format!("(Streak {}, Kills {})", item.streak_idx, item.kills_count));
                                                    
                                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                        ui.colored_label(color, format!("{:?}", item.status));
                                                    });
                                                });
                                                if let Some(ref sub) = item.sub_status {
                                                    ui.add_space(2.0);
                                                    ui.weak(format!("Step: {}", sub));
                                                }
                                                if let Some(ref err) = item.error_message {
                                                    ui.add_space(4.0);
                                                    ui.horizontal(|ui| {
                                                        ui.colored_label(egui::Color32::RED, "⚠ Error:");
                                                        ui.add(egui::Label::new(egui::RichText::new(err).color(egui::Color32::RED)).wrap());
                                                    });
                                                }
                                                if item.debug_command.is_some() {
                                                    ui.add_space(4.0);
                                                    let collapsing_id = ui.make_persistent_id(format!("debug_log_{}", item.id));
                                                    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), collapsing_id, false)
                                                        .show_header(ui, |ui| {
                                                            ui.label("🔧 Show Debug Logs");
                                                        })
                                                        .body(|ui| {
                                                            if let Some(ref cmd_str) = item.debug_command {
                                                                ui.horizontal(|ui| {
                                                                    ui.strong("Launch Command:");
                                                                    ui.text_edit_multiline(&mut cmd_str.clone());
                                                                });
                                                            }
                                                        });
                                                }
                                            });
                                            ui.add_space(4.0);
                                        }
                                    });
                            }
                        });
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        ui.label("HLAE Capture is not supported in the WASM target.");
                    }
                }
                CaptureStudioState::Rendering => {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        self.hlcr_state.draw_ui(ui, ctx);
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        ui.label("HLCR rendering is not supported in the WASM target.");
                    }
                }
                CaptureStudioState::Complete => {
                    ui.vertical_centered(|ui| {
                        ui.heading("Capture Studio Complete");
                        ui.add_space(10.0);
                        ui.label("All recording and rendering processes have finished.");
                        if ui.button("Return to Queue").clicked() {
                            self.capture_studio_state = CaptureStudioState::ReviewingQueue;
                        }
                    });
                }
            }
        });
    }
}
