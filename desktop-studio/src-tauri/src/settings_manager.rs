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
    pub language: String,
    pub capture_fps: i32,
    pub pre_roll_seconds: f32,
    pub post_roll_seconds: f32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hlae_path: String::new(),
            hl_path: String::new(),
            ffmpeg_path: None,
            pinned_folders: Vec::new(),
            language: "en".to_string(),
            capture_fps: 300,
            pre_roll_seconds: 2.0,
            post_roll_seconds: 0.6,
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
