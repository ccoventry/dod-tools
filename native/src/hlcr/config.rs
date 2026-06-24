#![cfg(not(target_arch = "wasm32"))]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum RenderCodec {
    NvencH264,
    ProRes,
    DnxHr,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RenderConfig {
    pub ffmpeg_path: String,
    pub source_folder: String,
    pub primary_export_dir: Option<PathBuf>,
    pub backup_export_dir: Option<PathBuf>,
    pub fps: u32,
    pub target_codec: RenderCodec,
    pub max_concurrent_renders: usize,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            ffmpeg_path: "ffmpeg".to_string(),
            source_folder: "".to_string(),
            primary_export_dir: None,
            backup_export_dir: None,
            fps: 300,
            target_codec: RenderCodec::ProRes,
            max_concurrent_renders: 2,
        }
    }
}

pub struct CodecPreset {
    pub standard: Vec<&'static str>,
    pub alpha: Vec<&'static str>,
    pub ext_standard: &'static str,
    pub ext_alpha: &'static str,
}

pub fn get_codec_preset(codec: &str) -> CodecPreset {
    match codec {
        "h264" => CodecPreset {
            standard: vec!["-c:v", "libx264", "-preset", "fast", "-crf", "16", "-pix_fmt", "yuv420p"],
            alpha: vec!["-c:v", "prores_ks", "-profile:v", "4444", "-pix_fmt", "yuva444p10le"],
            ext_standard: ".mp4",
            ext_alpha: ".mov",
        },
        "dnxhr" => CodecPreset {
            standard: vec!["-c:v", "dnxhd", "-profile:v", "dnxhr_hq", "-pix_fmt", "yuv422p"],
            alpha: vec!["-c:v", "dnxhd", "-profile:v", "dnxhr_444", "-pix_fmt", "yuv444p10le"],
            ext_standard: ".mov",
            ext_alpha: ".mov",
        },
        _ => CodecPreset { // Default is ProRes
            standard: vec!["-c:v", "prores", "-profile:v", "3", "-pix_fmt", "yuv422p10le"],
            alpha: vec!["-c:v", "prores_ks", "-profile:v", "4444", "-pix_fmt", "yuva444p10le"],
            ext_standard: ".mov",
            ext_alpha: ".mov",
        },
    }
}

pub fn get_config_path() -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            return parent.join("hlcr_config.json");
        }
    }
    PathBuf::from("hlcr_config.json")
}

pub fn load_config() -> RenderConfig {
    let path = get_config_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str::<RenderConfig>(&content) {
                return config;
            }
        }
    }
    let default_config = RenderConfig::default();
    let _ = save_config(&default_config);
    default_config
}

pub fn save_config(config: &RenderConfig) -> Result<(), String> {
    let path = get_config_path();
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    std::fs::write(&path, content)
        .map_err(|e| format!("Failed to write config file to {}: {}", path.display(), e))?;
    Ok(())
}
