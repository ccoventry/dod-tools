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
    /// Demo Analyzer Explorer sidebar's drag-to-resize width, in pixels.
    #[serde(default = "default_analyzer_explorer_width")]
    pub analyzer_explorer_width: i32,
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
    #[serde(default)]
    pub ffmpeg_capture: bool,
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
    /// Connected-workspace mode: "quick-clip" (nothing persists, blunt
    /// clearing) or "workspace" (project file + take index persist, clearing
    /// protects tracked demos). See docs/engineering_backlog.md Phase 4.
    #[serde(default = "default_studio_mode")]
    pub studio_mode: String,
}

fn default_resolution_width() -> i32 { 1280 }
fn default_resolution_height() -> i32 { 720 }
fn default_add_condebug() -> bool { true }
fn default_initial_delay() -> f32 { 3.0 }
fn default_fast_forward_speed() -> f32 { 0.05 }
fn default_render_codec() -> String { "prores".to_string() }
fn default_render_fps() -> i32 { 300 }
fn default_render_max_concurrent() -> i32 { 2 }
fn default_analyzer_explorer_width() -> i32 { 260 }
fn default_studio_mode() -> String { "quick-clip".to_string() }

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hlae_path: String::new(),
            hl_path: String::new(),
            ffmpeg_path: None,
            pinned_folders: Vec::new(),
            demo_folder_history: Vec::new(),
            scan_folders_for_demos: false,
            analyzer_explorer_width: default_analyzer_explorer_width(),
            language: "en".to_string(),
            capture_fps: 300,
            pre_roll_seconds: 2.0,
            post_roll_seconds: 0.6,
            resolution_width: default_resolution_width(),
            resolution_height: default_resolution_height(),
            separate_hud: false,
            ffmpeg_capture: false,
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
            render_folders: Vec::new(),
            render_codec: default_render_codec(),
            render_fps: default_render_fps(),
            render_max_concurrent: default_render_max_concurrent(),
            render_export_dirs: Vec::new(),
            studio_mode: default_studio_mode(),
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// A settings.json saved by a pre-2026-08-18 build still has an
    /// `allocation_strategy` key on disk (the field was removed from
    /// `AppSettings` when the Chronological drive-allocation strategy was
    /// deleted). Since `AppSettings` doesn't set `#[serde(deny_unknown_fields)]`,
    /// serde must silently ignore the stale key rather than fail deserialization
    /// — otherwise every existing user's settings would revert to defaults on
    /// their next launch after upgrading.
    #[test]
    fn test_deserialize_ignores_legacy_allocation_strategy_field() {
        let legacy_json = r#"{
            "hlae_path": "C:/hlae/hlae.exe",
            "hl_path": "C:/dod/hl.exe",
            "ffmpeg_path": null,
            "pinned_folders": [],
            "language": "en",
            "capture_fps": 300,
            "pre_roll_seconds": 2.0,
            "post_roll_seconds": 0.6,
            "allocation_strategy": "Chronological"
        }"#;

        let settings: AppSettings = serde_json::from_str(legacy_json)
            .expect("legacy allocation_strategy field must not break deserialization");

        assert_eq!(settings.hlae_path, "C:/hlae/hlae.exe");
        assert_eq!(settings.capture_fps, 300);
        // Pre-Phase-4 settings.json files have no studio_mode key at all —
        // must default to Quick-Clip rather than fail deserialization.
        assert_eq!(settings.studio_mode, "quick-clip");
    }

    #[test]
    fn test_default_settings_serialize_and_deserialize_roundtrip() {
        let original = AppSettings::default();
        let json = serde_json::to_string(&original).expect("default settings must serialize");
        let restored: AppSettings =
            serde_json::from_str(&json).expect("serialized default settings must deserialize");

        assert_eq!(restored.language, original.language);
        assert_eq!(restored.capture_fps, original.capture_fps);
        assert_eq!(restored.fast_forward_speed, original.fast_forward_speed);
        assert_eq!(restored.render_codec, original.render_codec);
        assert_eq!(restored.studio_mode, original.studio_mode);
    }
}
