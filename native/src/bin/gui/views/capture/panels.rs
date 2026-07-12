use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use egui_extras::{TableBuilder, Column};
use native::patch::{PatcherConfig, CommandRelation};
use crate::settings::{AppSettings, save_settings, apply_language_setting};
use super::widgets;
use super::acquire_lock;

fn create_pinned_file_dialog() -> egui_file_dialog::FileDialog {
    let mut fd = egui_file_dialog::FileDialog::new();
    let global = crate::settings::load_settings();
    if !global.pinned_folders.is_empty() {
        fd = fd.add_quick_access("Bookmarks", |s| {
            for path in &global.pinned_folders {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                s.add_path(&name, path.clone());
            }
        });
    }
    fd
}

pub fn render_engine_config_panel(
    ui: &mut egui::Ui,
    config: &mut PatcherConfig,
    error_message: &mut Option<String>,
) {
    // HLAE Path configuration
    widgets::render_path_row(
        ui,
        "HLAE Path (hlae.exe):",
        &mut config.hlae_path,
        "Executables",
        &["exe"],
        Some("hlae.exe"),
        error_message,
    );

    ui.add_space(8.0);

    // DoD Game Path configuration
    widgets::render_path_row(
        ui,
        "DoD Game Path (hl.exe):",
        &mut config.game_path,
        "Executables",
        &["exe"],
        Some("hl.exe"),
        error_message,
    );

    ui.add_space(8.0);

    // Custom FFmpeg Path configuration
    let mut ffmpeg_str = config.ffmpeg_override_path.clone().unwrap_or_default();
    widgets::render_path_row(
        ui,
        "Custom FFmpeg Path (Optional):",
        &mut ffmpeg_str,
        "All Files",
        &[],
        None,
        error_message,
    );
    if ffmpeg_str.trim().is_empty() {
        if config.ffmpeg_override_path.is_some() {
            config.ffmpeg_override_path = None;
            crate::settings::save_patcher_config(config);
        }
    } else if config.ffmpeg_override_path.as_deref() != Some(&ffmpeg_str) {
        config.ffmpeg_override_path = Some(ffmpeg_str.trim().to_string());
        crate::settings::save_patcher_config(config);
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
}

pub fn render_highlight_settings_panel(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    config: &mut PatcherConfig,
    settings: &mut AppSettings,
    draft_settings: &mut AppSettings,
    subdir_cache: &mut HashMap<PathBuf, Vec<PathBuf>>,
    tree_demo_cache: &mut HashMap<PathBuf, usize>,
) {
    ui.label("Init Commands (startup):");
    ui.add_space(4.0);
    widgets::render_command_list(
        ui,
        "init_commands_scroll_select",
        &mut config.init_commands,
        120.0,
    );
    
    ui.add_space(8.0);
    ui.label("Default Custom Commands:");
    ui.add_space(4.0);
    widgets::render_custom_command_list(
        ui,
        "default_commands_scroll_select",
        &mut config.custom_commands,
        120.0,
    );

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    ui.strong("Timeline Buffers");
    ui.add_space(4.0);

    ui.label("Initial Load Delay:");
    let mut val = config.initial_delay;
    if ui.add(egui::Slider::new(&mut val, 0.0..=30.0).step_by(0.5).suffix("s")).changed() {
        config.initial_delay = val;
    }

    ui.label("Fast-Forward Speed:");
    ui.add_enabled_ui(false, |ui| {
        let mut val = config.fast_forward_speed;
        if ui.add(egui::Slider::new(&mut val, 0.01..=5.0).step_by(0.05)).changed() {
            config.fast_forward_speed = val;
        }
    });

    ui.label("Pre-Record Buffer:");
    let mut val = config.pre_roll_seconds;
    if ui.add(egui::Slider::new(&mut val, 0.0..=30.0).step_by(0.5).suffix("s")).changed() {
        config.pre_roll_seconds = val;
    }

    ui.label("Record Start Lead:");
    let mut val = config.record_start_lead;
    if ui.add(egui::Slider::new(&mut val, 0.0..=10.0).step_by(0.5).suffix("s")).changed() {
        config.record_start_lead = val;
    }

    ui.label("Record Stop Trail:");
    let mut val = config.record_stop_trail;
    if ui.add(egui::Slider::new(&mut val, 0.0..=10.0).step_by(0.5).suffix("s")).changed() {
        config.record_stop_trail = val;
    }

    ui.label("Post-Record Buffer:");
    let mut val = config.post_roll_seconds;
    if ui.add(egui::Slider::new(&mut val, 0.0..=30.0).step_by(0.5).suffix("s")).changed() {
        config.post_roll_seconds = val;
    }

    // ── Mock Execution Timeline Visualizer ─────────────────────
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(egui::RichText::new("Execution Timeline (Mock)").strong());
    ui.add_space(4.0);

    struct TimelineEvent {
        time: f32,
        name: String,
        is_custom: bool,
    }

    let mut events = Vec::new();
    events.push(TimelineEvent {
        time: 0.0,
        name: "First Kill (Anchor)".to_string(),
        is_custom: false,
    });
    events.push(TimelineEvent {
        time: -config.record_start_lead,
        name: "Record Start".to_string(),
        is_custom: false,
    });
    events.push(TimelineEvent {
        time: -config.record_start_lead - config.pre_roll_seconds,
        name: "Pre-Roll (Speed Normal & Audio Flush)".to_string(),
        is_custom: false,
    });
    events.push(TimelineEvent {
        time: 10.0,
        name: "Last Kill (Anchor)".to_string(),
        is_custom: false,
    });
    events.push(TimelineEvent {
        time: 10.0 + config.record_stop_trail,
        name: "Record Stop".to_string(),
        is_custom: false,
    });
    events.push(TimelineEvent {
        time: 10.0 + config.record_stop_trail + config.post_roll_seconds,
        name: "Post-Roll End (Fast Forward)".to_string(),
        is_custom: false,
    });

    for custom in &config.custom_commands {
        let t = match custom.relation {
            CommandRelation::Before => -custom.offset,
            CommandRelation::After => 10.0 + custom.offset,
        };
        events.push(TimelineEvent {
            time: t,
            name: format!("Custom Cmd: {}", custom.command),
            is_custom: true,
        });
    }

    events.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));

    TableBuilder::new(ui)
        .striped(true)
        .column(Column::exact(100.0))
        .column(Column::remainder())
        .header(18.0, |mut header| {
            header.col(|ui| { ui.strong("Relative Time"); });
            header.col(|ui| { ui.strong("Action/Event"); });
        })
        .body(|body| {
            body.rows(18.0, events.len(), |mut row| {
                let event = &events[row.index()];
                row.col(|ui| {
                    let time_str = if event.time >= 0.0 {
                        format!("+{:.1} sec", event.time)
                    } else {
                        format!("{:.1} sec", event.time)
                    };
                    let color = if event.is_custom {
                        egui::Color32::LIGHT_BLUE
                    } else {
                        egui::Color32::GRAY
                    };
                    ui.colored_label(color, time_str);
                });
                row.col(|ui| {
                    let color = if event.is_custom {
                        egui::Color32::LIGHT_BLUE
                    } else {
                        egui::Color32::GRAY
                    };
                    ui.colored_label(color, &event.name);
                });
            });
        });

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("💾 Save Settings").clicked() {
            let old_scan = settings.scan_folders_for_demos;
            *settings = draft_settings.clone();
            apply_language_setting(&settings.language);
            save_settings(settings);
            
            if old_scan != settings.scan_folders_for_demos {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    subdir_cache.clear();
                    tree_demo_cache.clear();
                }
            }
            ctx.request_repaint();
        }
        if ui.button("🔄 Revert Settings").clicked() {
            *draft_settings = settings.clone();
            ctx.request_repaint();
        }
    });
}

pub fn render_capture_config_panel(
    ui: &mut egui::Ui,
    _ctx: &egui::Context,
    config: &mut PatcherConfig,
) {
    ui.add_space(8.0);

    // Row 4: Resolution & Capture FPS
    ui.horizontal(|ui| {
        ui.label("Width:");
        ui.add(egui::DragValue::new(&mut config.resolution_width)
            .range(640..=7680).speed(1));
        ui.add_space(10.0);
        ui.label("Height:");
        ui.add(egui::DragValue::new(&mut config.resolution_height)
            .range(480..=4320).speed(1));
        ui.add_space(10.0);
        ui.label("Capture FPS:");
        ui.add(egui::DragValue::new(&mut config.capture_fps)
            .range(30..=1000).speed(1));
    });

    // Row 5: Separate HUD
    ui.horizontal(|ui| {
        ui.checkbox(&mut config.separate_hud, "Separate HUD (Alpha & Color)")
            .on_hover_text("This toggle acts as the absolute source of truth and will override any separate_hud settings in your movie.cfg.");
    });

    // Row 5.5: Exit on Finish
    ui.horizontal(|ui| {
        ui.checkbox(&mut config.exit_on_finish, "Auto-Quit Game on Completion")
            .on_hover_text("If enabled, the game will automatically inject the 'quit' command after the final clip to close the game.");
    });

    ui.add_space(8.0);

    // Row 5.75: Movie Config
    ui.horizontal(|ui| {
        ui.label("Movie Config (Optional):");
        if ui.add(egui::TextEdit::singleline(&mut config.movie_config)
            .hint_text("e.g., movie.cfg")).changed() {
            
            // Aggressive sanitization
            config.movie_config.retain(|c| !c.is_whitespace());
            let sanitized = config.movie_config.trim_start_matches(|c| c == '-' || c == '+').to_string();
            config.movie_config = sanitized;
        }
    });
}

pub fn render_debug_panel(
    ui: &mut egui::Ui,
    config: &mut PatcherConfig,
) {
    ui.add_space(4.0);
    
    ui.horizontal(|ui| {
        ui.checkbox(&mut config.add_condebug, "Add Condebug to Launch Commands")
            .on_hover_text("If enabled, '-condebug' will be added to the launch arguments to generate a qconsole.log file.");
    });
    
    ui.horizontal(|ui| {
        ui.checkbox(&mut config.save_local_patched_copy, "Save a copy of patched demo to ./demos/")
            .on_hover_text("If enabled, a copy of the patched .dem file will be saved to the workspace's demos/ folder for debugging.");
    });

    ui.horizontal(|ui| {
        ui.checkbox(&mut config.auto_clear_logs, "Auto-Clear Logs & CFGs")
            .on_hover_text("If enabled, helper config files and log files are deleted automatically on exit.");
    });

    ui.horizontal(|ui| {
        ui.checkbox(&mut config.auto_clear_previews, "Auto-Clear Previews")
            .on_hover_text("If enabled, generated preview demos are deleted automatically on exit.");
    });

    ui.horizontal(|ui| {
        ui.checkbox(&mut config.auto_clear_temp_demos, "Auto-Clear Temporary Demos")
            .on_hover_text("If enabled, transient copy and chained demos from the game folder are deleted automatically on exit.");
    });
}

pub fn render_export_config_panel(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    render_config: &mut native::hlcr::config::RenderConfig,
) {
    ui.horizontal(|ui| {
        ui.label("Render Codec:");
        egui::ComboBox::from_id_salt("render_codec_combo")
            .selected_text(format!("{:?}", render_config.target_codec))
            .show_ui(ui, |ui| {
                let mut changed = false;
                changed |= ui.selectable_value(&mut render_config.target_codec, native::hlcr::config::RenderCodec::ProRes, "ProRes").changed();
                changed |= ui.selectable_value(&mut render_config.target_codec, native::hlcr::config::RenderCodec::NvencH264, "NvencH264").changed();
                changed |= ui.selectable_value(&mut render_config.target_codec, native::hlcr::config::RenderCodec::DnxHr, "DnxHr").changed();
                if changed {
                    let _ = native::hlcr::config::save_config(render_config);
                }
            });
    });

    #[cfg(not(target_arch = "wasm32"))]
    {
        static PRIMARY_PICKER: std::sync::OnceLock<Mutex<egui_file_dialog::FileDialog>> = std::sync::OnceLock::new();
        static BACKUP_PICKER: std::sync::OnceLock<Mutex<egui_file_dialog::FileDialog>> = std::sync::OnceLock::new();
        
        let mut primary_picker = acquire_lock!(PRIMARY_PICKER.get_or_init(|| Mutex::new(create_pinned_file_dialog())));
        if widgets::render_dir_picker_row(
            ui,
            ctx,
            "Primary Export Directory (Final .mov):",
            &mut primary_picker,
            &mut render_config.primary_export_dir,
        ) {
            let _ = native::hlcr::config::save_config(render_config);
        }

        let mut backup_picker = acquire_lock!(BACKUP_PICKER.get_or_init(|| Mutex::new(create_pinned_file_dialog())));
        if widgets::render_dir_picker_row(
            ui,
            ctx,
            "Backup Export Directory:",
            &mut backup_picker,
            &mut render_config.backup_export_dir,
        ) {
            let _ = native::hlcr::config::save_config(render_config);
        }
    }
}
