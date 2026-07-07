use std::path::PathBuf;

pub type Settings = AppSettings;

#[derive(Debug, Clone)]
pub struct AppSettings {
    pub language: String,
    pub scan_folders_for_demos: bool,
    pub demo_folder_history: Vec<PathBuf>,
    pub pinned_folders: Vec<PathBuf>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "auto".to_string(),
            scan_folders_for_demos: false,
            demo_folder_history: Vec::new(),
            pinned_folders: Vec::new(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_settings() -> AppSettings {
    let path = PathBuf::from("settings.json");
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                let language = val
                    .get("language")
                    .and_then(|v| v.as_str())
                    .unwrap_or("auto")
                    .to_string();
                let scan_folders_for_demos = val
                    .get("scan_folders_for_demos")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let demo_folder_history = val
                    .get("demo_folder_history")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(PathBuf::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let pinned_folders = val
                    .get("pinned_folders")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(PathBuf::from))
                            .collect()
                    })
                    .unwrap_or_default();
                return AppSettings {
                    language,
                    scan_folders_for_demos,
                    demo_folder_history,
                    pinned_folders,
                };
            }
        }
    }
    AppSettings::default()
}

#[cfg(target_arch = "wasm32")]
pub fn load_settings() -> AppSettings {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            if let Ok(Some(content)) = storage.get_item("settings") {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    let language = val
                        .get("language")
                        .and_then(|v| v.as_str())
                        .unwrap_or("auto")
                        .to_string();
                    let scan_folders_for_demos = val
                        .get("scan_folders_for_demos")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let demo_folder_history = val
                        .get("demo_folder_history")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(PathBuf::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let pinned_folders = val
                        .get("pinned_folders")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(PathBuf::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    return AppSettings {
                        language,
                        scan_folders_for_demos,
                        demo_folder_history,
                        pinned_folders,
                    };
                }
            }
        }
    }
    AppSettings::default()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_settings(settings: &AppSettings) {
    let mut map = serde_json::Map::new();
    map.insert(
        "language".to_string(),
        serde_json::Value::String(settings.language.clone()),
    );
    map.insert(
        "scan_folders_for_demos".to_string(),
        serde_json::Value::Bool(settings.scan_folders_for_demos),
    );
    map.insert(
        "demo_folder_history".to_string(),
        serde_json::Value::Array(
            settings
                .demo_folder_history
                .iter()
                .map(|p| serde_json::Value::String(p.to_string_lossy().into_owned()))
                .collect(),
        ),
    );
    map.insert(
        "pinned_folders".to_string(),
        serde_json::Value::Array(
            settings
                .pinned_folders
                .iter()
                .map(|p| serde_json::Value::String(p.to_string_lossy().into_owned()))
                .collect(),
        ),
    );
    let val = serde_json::Value::Object(map);
    if let Ok(content) = serde_json::to_string_pretty(&val) {
        let _ = std::fs::write("settings.json", content);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn save_settings(settings: &AppSettings) {
    let mut map = serde_json::Map::new();
    map.insert(
        "language".to_string(),
        serde_json::Value::String(settings.language.clone()),
    );
    map.insert(
        "scan_folders_for_demos".to_string(),
        serde_json::Value::Bool(settings.scan_folders_for_demos),
    );
    map.insert(
        "demo_folder_history".to_string(),
        serde_json::Value::Array(
            settings
                .demo_folder_history
                .iter()
                .map(|p| serde_json::Value::String(p.to_string_lossy().into_owned()))
                .collect(),
        ),
    );
    map.insert(
        "pinned_folders".to_string(),
        serde_json::Value::Array(
            settings
                .pinned_folders
                .iter()
                .map(|p| serde_json::Value::String(p.to_string_lossy().into_owned()))
                .collect(),
        ),
    );
    let val = serde_json::Value::Object(map);
    if let Ok(content) = serde_json::to_string_pretty(&val) {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let _ = storage.set_item("settings", &content);
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub fn detect_os_language() -> String {
    for var in &["LANG", "LC_ALL", "LC_MESSAGES"] {
        if let Ok(val) = std::env::var(var) {
            let val_lower = val.to_lowercase();
            if val_lower.starts_with("de") {
                return "german".to_string();
            }
            if val_lower.starts_with("fr") {
                return "french".to_string();
            }
            if val_lower.starts_with("es") {
                return "spanish".to_string();
            }
            if val_lower.starts_with("ru") {
                return "russian".to_string();
            }
            if val_lower.starts_with("en") {
                return "english".to_string();
            }
        }
    }

    if let Ok(output) = std::process::Command::new("reg")
        .args(&[
            "query",
            "HKCU\\Control Panel\\International",
            "/v",
            "LocaleName",
        ])
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).to_lowercase();
            for line in s.lines() {
                if line.contains("localename") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if let Some(locale) = parts.last() {
                        let loc = locale.to_lowercase();
                        if loc.starts_with("de") {
                            return "german".to_string();
                        }
                        if loc.starts_with("fr") {
                            return "french".to_string();
                        }
                        if loc.starts_with("es") {
                            return "spanish".to_string();
                        }
                        if loc.starts_with("ru") {
                            return "russian".to_string();
                        }
                        if loc.starts_with("sr") {
                            return "serbian".to_string();
                        }
                        if loc.starts_with("tr") {
                            return "turkish".to_string();
                        }
                        if loc.starts_with("pl") {
                            return "polish".to_string();
                        }
                    }
                }
            }
        }
    }

    if let Ok(output) = std::process::Command::new("powershell")
        .args(&[
            "-NoProfile",
            "-Command",
            "[System.Globalization.CultureInfo]::CurrentUICulture.Name",
        ])
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_lowercase();
            if s.starts_with("de") {
                return "german".to_string();
            }
            if s.starts_with("fr") {
                return "french".to_string();
            }
            if s.starts_with("es") {
                return "spanish".to_string();
            }
            if s.starts_with("ru") {
                return "russian".to_string();
            }
            if s.starts_with("sr") {
                return "serbian".to_string();
            }
            if s.starts_with("tr") {
                return "turkish".to_string();
            }
            if s.starts_with("pl") {
                return "polish".to_string();
            }
        }
    }

    "english".to_string()
}

#[cfg(not(target_os = "windows"))]
pub fn detect_os_language() -> String {
    for var in &["LANG", "LC_ALL", "LC_MESSAGES"] {
        if let Ok(val) = std::env::var(var) {
            let val_lower = val.to_lowercase();
            if val_lower.starts_with("de") {
                return "german".to_string();
            }
            if val_lower.starts_with("fr") {
                return "french".to_string();
            }
            if val_lower.starts_with("es") {
                return "spanish".to_string();
            }
            if val_lower.starts_with("ru") {
                return "russian".to_string();
            }
            if val_lower.starts_with("en") {
                return "english".to_string();
            }
        }
    }
    "english".to_string()
}

pub fn apply_language_setting(settings_lang: &str) {
    let target_lang = if settings_lang == "auto" {
        detect_os_language()
    } else {
        settings_lang.to_string()
    };
    let static_lang = match target_lang.as_str() {
        "german" => "german",
        "french" => "french",
        "spanish" => "spanish",
        "russian" => "russian",
        "serbian" => "serbian",
        "polish" => "polish",
        "turkish" => "turkish",
        _ => "english",
    };
    analysis::set_active_language(static_lang);
}

pub fn resolve_ffmpeg_path(ffmpeg_override_path: Option<&String>) -> Result<std::path::PathBuf, String> {
    // 1. User Override (Settings)
    if let Some(path_str) = ffmpeg_override_path {
        if !path_str.trim().is_empty() {
            let path = std::path::PathBuf::from(path_str);
            if path.exists() {
                return Ok(path);
            }
        }
    }

    // 2. Bundled Local Executable
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                let local_path = parent.join("local/tools/ffmpeg.exe");
                if local_path.exists() {
                    return Ok(local_path);
                }
            }
        }
    }

    // 3. System PATH (return "ffmpeg")
    Ok(std::path::PathBuf::from("ffmpeg"))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_patcher_config() -> native::patch::PatcherConfig {
    let path = PathBuf::from("patcher_config.json");
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str::<native::patch::PatcherConfig>(&content) {
                return config;
            }
        }
    }
    native::patch::PatcherConfig::default()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_patcher_config(config: &native::patch::PatcherConfig) {
    let path = PathBuf::from("patcher_config.json");
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(&path, json);
    }
}
