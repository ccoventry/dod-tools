use crate::capture_manager::CustomCommandPayload;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub hlae_path: String,
    pub hl_path: String,
    pub ffmpeg_path: Option<String>,
    pub pinned_folders: Vec<String>,
    /// Analyzer sidebar's "Recent" quick-links tier — most-recent-first,
    /// capped at 10, pushed whenever a folder selection yields a non-empty
    /// demo listing. Mirrors dev's `settings.demo_folder_history`.
    #[serde(default)]
    pub demo_folder_history: Vec<String>,
    /// Gates the Demo Analyzer Explorer tree's per-subfolder "(N)" demo-count
    /// badge (Quick Links counts are always shown, matching dev — see
    /// docs/tauri_parity_audit.md Area 1). Defaults to `false`, matching
    /// dev's own `AppSettings::default()`.
    #[serde(default)]
    pub scan_folders_for_demos: bool,
    pub language: String,
    pub capture_fps: i32,
    pub pre_roll_seconds: f32,
    pub post_roll_seconds: f32,
    #[serde(default = "default_resolution_width")]
    pub resolution_width: i32,
    #[serde(default = "default_resolution_height")]
    pub resolution_height: i32,
    #[serde(default)]
    pub separate_hud: bool,
    #[serde(default = "default_add_condebug")]
    pub add_condebug: bool,
    #[serde(default)]
    pub auto_clear_logs: bool,
    #[serde(default)]
    pub auto_clear_previews: bool,
    #[serde(default)]
    pub auto_clear_temp_demos: bool,
    #[serde(default)]
    pub record_start_lead: f32,
    #[serde(default)]
    pub record_stop_trail: f32,
    #[serde(default = "default_initial_delay")]
    pub initial_delay: f32,
    #[serde(default = "default_fast_forward_speed")]
    pub fast_forward_speed: f32,
    /// Capture Output pool — required, no fallback (see capture_pane.js's
    /// refreshLaunchGuard/buildCapturePayload).
    #[serde(default)]
    pub target_drives: Vec<String>,
    #[serde(default)]
    pub init_commands: Vec<String>,
    #[serde(default)]
    pub custom_commands: Vec<CustomCommandPayload>,
    #[serde(default)]
    pub save_local_patched_copy: bool,
    /// Matches native `DriveAllocationStrategy`: "MaximizeSpace" | "Chronological".
    #[serde(default = "default_allocation_strategy")]
    pub allocation_strategy: String,
    #[serde(default)]
    pub render_folders: Vec<String>,
    #[serde(default = "default_render_codec")]
    pub render_codec: String,
    #[serde(default = "default_render_fps")]
    pub render_fps: i32,
    #[serde(default = "default_render_max_concurrent")]
    pub render_max_concurrent: i32,
    /// JIT multi-drive export pool for Render Studio.
    #[serde(default)]
    pub render_export_dirs: Vec<String>,
}

fn default_resolution_width() -> i32 { 1280 }
fn default_resolution_height() -> i32 { 720 }
fn default_add_condebug() -> bool { true }
fn default_initial_delay() -> f32 { 3.0 }
fn default_fast_forward_speed() -> f32 { 0.05 }
fn default_allocation_strategy() -> String { "MaximizeSpace".to_string() }
fn default_render_codec() -> String { "prores".to_string() }
fn default_render_fps() -> i32 { 300 }
fn default_render_max_concurrent() -> i32 { 2 }

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hlae_path: String::new(),
            hl_path: String::new(),
            ffmpeg_path: None,
            pinned_folders: Vec::new(),
            demo_folder_history: Vec::new(),
            scan_folders_for_demos: false,
            language: "en".to_string(),
            capture_fps: 300,
            pre_roll_seconds: 2.0,
            post_roll_seconds: 0.6,
            resolution_width: default_resolution_width(),
            resolution_height: default_resolution_height(),
            separate_hud: false,
            add_condebug: default_add_condebug(),
            auto_clear_logs: false,
            auto_clear_previews: false,
            auto_clear_temp_demos: false,
            record_start_lead: 0.0,
            record_stop_trail: 0.0,
            initial_delay: default_initial_delay(),
            fast_forward_speed: default_fast_forward_speed(),
            target_drives: Vec::new(),
            init_commands: Vec::new(),
            custom_commands: Vec::new(),
            save_local_patched_copy: false,
            allocation_strategy: default_allocation_strategy(),
            render_folders: Vec::new(),
            render_codec: default_render_codec(),
            render_fps: default_render_fps(),
            render_max_concurrent: default_render_max_concurrent(),
            render_export_dirs: Vec::new(),
        }
    }
}

pub fn settings_path() -> PathBuf {
    let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = config_dir.join("dod-tools");
    if let Err(e) = fs::create_dir_all(&dir) {
        log::warn!("Failed to create settings directory {:?}: {}", dir, e);
    }
    dir.join("settings.json")
}

impl AppSettings {
    pub fn load_or_default() -> Self {
        let path = settings_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(settings) = serde_json::from_str::<AppSettings>(&content) {
                    return settings;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        fs::write(&path, json)
            .map_err(|e| format!("Failed to write settings file {:?}: {}", path, e))?;
        Ok(())
    }
}

pub struct SettingsManager {
    pub inner: Arc<Mutex<AppSettings>>,
}

impl SettingsManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AppSettings::load_or_default())),
        }
    }
}
