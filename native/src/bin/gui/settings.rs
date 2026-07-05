use std::path::PathBuf;

pub type Settings = AppSettings;

#[derive(Debug, Clone)]
pub struct AppSettings {
    pub language: String,
    pub scan_folders_for_demos: bool,
    pub demo_folder_history: Vec<PathBuf>,
    pub pinned_folders: Vec<PathBuf>,
    pub capture_init_commands: Vec<String>,
    pub custom_commands: Vec<native::patch::CustomCommand>,
    pub capture_initial_delay: f32,
    pub capture_fast_forward_speed: f32,
    pub capture_pre_record_buffer: f32,
    pub capture_record_start_lead: f32,
    pub capture_record_stop_trail: f32,
    pub post_record_buffer: f32,
    pub hlae_path: String,
    pub game_path: String,
    pub primary_media_dir: Option<String>,
    pub backup_media_dir: Option<String>,
    pub ffmpeg_override_path: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "auto".to_string(),
            scan_folders_for_demos: false,
            demo_folder_history: Vec::new(),
            pinned_folders: Vec::new(),
            capture_init_commands: Vec::new(),
            custom_commands: vec![
                native::patch::CustomCommand {
                    command: "r_decals 5555".to_string(),
                    offset: 2.0,
                    relation: native::patch::CommandRelation::Before,
                },
                native::patch::CustomCommand {
                    command: "hud_deathnotice_time 5555".to_string(),
                    offset: 2.0,
                    relation: native::patch::CommandRelation::Before,
                },
                native::patch::CustomCommand {
                    command: "r_decals 0".to_string(),
                    offset: 2.0,
                    relation: native::patch::CommandRelation::After,
                },
                native::patch::CustomCommand {
                    command: "hud_deathnotice_time 5".to_string(),
                    offset: 2.0,
                    relation: native::patch::CommandRelation::After,
                }
            ],
            capture_initial_delay: 3.0,
            capture_fast_forward_speed: 0.2,
            capture_pre_record_buffer: 6.0,
            capture_record_start_lead: 2.0,
            capture_record_stop_trail: 2.0,
            post_record_buffer: 4.0,
            hlae_path: "".to_string(),
            game_path: "".to_string(),
            primary_media_dir: None,
            backup_media_dir: None,
            ffmpeg_override_path: None,
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
                let capture_init_commands = val
                    .get("capture_init_commands")
                    .map(|v| {
                        if let Some(arr) = v.as_array() {
                            arr.iter().filter_map(|x| x.as_str().map(String::from)).collect()
                        } else if let Some(s) = v.as_str() {
                            s.lines().map(String::from).collect()
                        } else {
                            Vec::new()
                        }
                    })
                    .unwrap_or_default();
                let custom_commands = val
                    .get("custom_commands")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| {
                                let cmd = v.get("command")?.as_str()?.to_string();
                                let offset = v.get("offset")?.as_f64()? as f32;
                                let rel_str = v.get("relation")?.as_str()?;
                                let relation = match rel_str {
                                    "After" => native::patch::CommandRelation::After,
                                    _ => native::patch::CommandRelation::Before,
                                };
                                Some(native::patch::CustomCommand {
                                    command: cmd,
                                    offset,
                                    relation,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_else(|| AppSettings::default().custom_commands);
                let capture_initial_delay = val.get("capture_initial_delay").and_then(|v| v.as_f64()).unwrap_or(3.0) as f32;
                let capture_fast_forward_speed = val.get("capture_fast_forward_speed").and_then(|v| v.as_f64()).unwrap_or(0.2) as f32;
                let capture_pre_record_buffer = val.get("capture_pre_record_buffer").and_then(|v| v.as_f64()).unwrap_or(6.0) as f32;
                let capture_record_start_lead = val.get("capture_record_start_lead").and_then(|v| v.as_f64()).unwrap_or(2.0) as f32;
                let capture_record_stop_trail = val.get("capture_record_stop_trail").and_then(|v| v.as_f64()).unwrap_or(2.0) as f32;
                let post_record_buffer = val.get("capture_post_record_buffer").and_then(|v| v.as_f64()).unwrap_or(4.0) as f32;
                let hlae_path = val.get("hlae_path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let game_path = val.get("game_path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let primary_media_dir = val.get("primary_media_dir").and_then(|v| v.as_str()).map(|s| s.to_string());
                let backup_media_dir = val.get("backup_media_dir").and_then(|v| v.as_str()).map(|s| s.to_string());
                let ffmpeg_override_path = val.get("ffmpeg_override_path").and_then(|v| v.as_str()).map(|s| s.to_string());
                return AppSettings {
                    language,
                    scan_folders_for_demos,
                    demo_folder_history,
                    pinned_folders,
                    capture_init_commands,
                    custom_commands,
                    capture_initial_delay,
                    capture_fast_forward_speed,
                    capture_pre_record_buffer,
                    capture_record_start_lead,
                    capture_record_stop_trail,
                    post_record_buffer,
                    hlae_path,
                    game_path,
                    primary_media_dir,
                    backup_media_dir,
                    ffmpeg_override_path,
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
                    let capture_init_commands = val
                        .get("capture_init_commands")
                        .map(|v| {
                            if let Some(arr) = v.as_array() {
                                arr.iter().filter_map(|x| x.as_str().map(String::from)).collect()
                            } else if let Some(s) = v.as_str() {
                                s.lines().map(String::from).collect()
                            } else {
                                Vec::new()
                            }
                        })
                        .unwrap_or_default();
                    let custom_commands = val
                        .get("custom_commands")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| {
                                    let cmd = v.get("command")?.as_str()?.to_string();
                                    let offset = v.get("offset")?.as_f64()? as f32;
                                    let rel_str = v.get("relation")?.as_str()?;
                                    let relation = match rel_str {
                                        "After" => native::patch::CommandRelation::After,
                                        _ => native::patch::CommandRelation::Before,
                                    };
                                    Some(native::patch::CustomCommand {
                                        command: cmd,
                                        offset,
                                        relation,
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_else(|| AppSettings::default().custom_commands);
                    let capture_initial_delay = val.get("capture_initial_delay").and_then(|v| v.as_f64()).unwrap_or(3.0) as f32;
                    let capture_fast_forward_speed = val.get("capture_fast_forward_speed").and_then(|v| v.as_f64()).unwrap_or(0.2) as f32;
                    let capture_pre_record_buffer = val.get("capture_pre_record_buffer").and_then(|v| v.as_f64()).unwrap_or(6.0) as f32;
                    let capture_record_start_lead = val.get("capture_record_start_lead").and_then(|v| v.as_f64()).unwrap_or(2.0) as f32;
                    let capture_record_stop_trail = val.get("capture_record_stop_trail").and_then(|v| v.as_f64()).unwrap_or(2.0) as f32;
                    let post_record_buffer = val.get("capture_post_record_buffer").and_then(|v| v.as_f64()).unwrap_or(4.0) as f32;
                    let hlae_path = val.get("hlae_path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let game_path = val.get("game_path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let primary_media_dir = val.get("primary_media_dir").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let backup_media_dir = val.get("backup_media_dir").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let ffmpeg_override_path = val.get("ffmpeg_override_path").and_then(|v| v.as_str()).map(|s| s.to_string());
                    return AppSettings {
                        language,
                        scan_folders_for_demos,
                        demo_folder_history,
                        pinned_folders,
                        capture_init_commands,
                        custom_commands,
                        capture_initial_delay,
                        capture_fast_forward_speed,
                        capture_pre_record_buffer,
                        capture_record_start_lead,
                        capture_record_stop_trail,
                        post_record_buffer,
                        hlae_path,
                        game_path,
                        primary_media_dir,
                        backup_media_dir,
                        ffmpeg_override_path,
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
    map.insert(
        "capture_init_commands".to_string(),
        serde_json::Value::Array(
            settings
                .capture_init_commands
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect(),
        ),
    );
    let mut custom_cmds_json = vec![];
    for cmd in &settings.custom_commands {
        let mut cmd_map = serde_json::Map::new();
        cmd_map.insert("command".to_string(), serde_json::Value::String(cmd.command.clone()));
        cmd_map.insert("offset".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(cmd.offset as f64).unwrap()));
        let rel_str = match cmd.relation {
            native::patch::CommandRelation::Before => "Before",
            native::patch::CommandRelation::After => "After",
        };
        cmd_map.insert("relation".to_string(), serde_json::Value::String(rel_str.to_string()));
        custom_cmds_json.push(serde_json::Value::Object(cmd_map));
    }
    map.insert("custom_commands".to_string(), serde_json::Value::Array(custom_cmds_json));
    map.insert(
        "capture_initial_delay".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_initial_delay as f64).unwrap()),
    );
    map.insert(
        "capture_fast_forward_speed".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_fast_forward_speed as f64).unwrap()),
    );
    map.insert(
        "capture_pre_record_buffer".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_pre_record_buffer as f64).unwrap()),
    );
    map.insert(
        "capture_record_start_lead".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_record_start_lead as f64).unwrap()),
    );
    map.insert(
        "capture_record_stop_trail".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_record_stop_trail as f64).unwrap()),
    );
    map.insert(
        "capture_post_record_buffer".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.post_record_buffer as f64).unwrap()),
    );
    map.insert(
        "hlae_path".to_string(),
        serde_json::Value::String(settings.hlae_path.clone()),
    );
    map.insert(
        "game_path".to_string(),
        serde_json::Value::String(settings.game_path.clone()),
    );
    if let Some(p) = &settings.primary_media_dir {
        map.insert("primary_media_dir".to_string(), serde_json::Value::String(p.clone()));
    }
    if let Some(b) = &settings.backup_media_dir {
        map.insert("backup_media_dir".to_string(), serde_json::Value::String(b.clone()));
    }
    if let Some(f) = &settings.ffmpeg_override_path {
        map.insert("ffmpeg_override_path".to_string(), serde_json::Value::String(f.clone()));
    }
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
    map.insert(
        "capture_init_commands".to_string(),
        serde_json::Value::Array(
            settings
                .capture_init_commands
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect(),
        ),
    );
    let mut custom_cmds_json = vec![];
    for cmd in &settings.custom_commands {
        let mut cmd_map = serde_json::Map::new();
        cmd_map.insert("command".to_string(), serde_json::Value::String(cmd.command.clone()));
        cmd_map.insert("offset".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(cmd.offset as f64).unwrap()));
        let rel_str = match cmd.relation {
            native::patch::CommandRelation::Before => "Before",
            native::patch::CommandRelation::After => "After",
        };
        cmd_map.insert("relation".to_string(), serde_json::Value::String(rel_str.to_string()));
        custom_cmds_json.push(serde_json::Value::Object(cmd_map));
    }
    map.insert("custom_commands".to_string(), serde_json::Value::Array(custom_cmds_json));
    map.insert(
        "capture_initial_delay".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_initial_delay as f64).unwrap()),
    );
    map.insert(
        "capture_fast_forward_speed".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_fast_forward_speed as f64).unwrap()),
    );
    map.insert(
        "capture_pre_record_buffer".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_pre_record_buffer as f64).unwrap()),
    );
    map.insert(
        "capture_record_start_lead".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_record_start_lead as f64).unwrap()),
    );
    map.insert(
        "capture_record_stop_trail".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.capture_record_stop_trail as f64).unwrap()),
    );
    map.insert(
        "capture_post_record_buffer".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(settings.post_record_buffer as f64).unwrap()),
    );
    map.insert(
        "hlae_path".to_string(),
        serde_json::Value::String(settings.hlae_path.clone()),
    );
    map.insert(
        "game_path".to_string(),
        serde_json::Value::String(settings.game_path.clone()),
    );
    if let Some(p) = &settings.primary_media_dir {
        map.insert("primary_media_dir".to_string(), serde_json::Value::String(p.clone()));
    }
    if let Some(b) = &settings.backup_media_dir {
        map.insert("backup_media_dir".to_string(), serde_json::Value::String(b.clone()));
    }
    if let Some(f) = &settings.ffmpeg_override_path {
        map.insert("ffmpeg_override_path".to_string(), serde_json::Value::String(f.clone()));
    }
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

pub fn resolve_ffmpeg_path(settings: &AppSettings) -> Result<std::path::PathBuf, String> {
    // 1. User Override (Settings)
    if let Some(ref path_str) = settings.ffmpeg_override_path {
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
