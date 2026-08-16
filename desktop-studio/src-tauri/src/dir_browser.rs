// dir_browser.rs — lightweight, non-recursive filesystem browsing for the
// Demo Analyzer's picker widgets: a folder tree (drives -> subfolders) and a
// list of .dem files sitting directly in whichever folder is selected. This
// mirrors the `dev` branch egui GUI's SidePanel::left explorer + browser.rs
// list (see native/src/bin/gui/tree.rs's get_native_roots/get_subdirs and
// views/browser.rs), minus the persisted pinned/recent quick-links and
// multi-threaded per-demo metadata parsing — those aren't needed just to
// pick a file.

use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
pub struct DirEntryLite {
    pub name: String,
    pub path: String,
}

#[derive(Serialize)]
pub struct DemoFileEntry {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub modified_unix_secs: f64,
}

#[derive(Serialize)]
pub struct DirListing {
    pub path: Option<String>,
    pub parent: Option<String>,
    pub subdirs: Vec<DirEntryLite>,
    pub demos: Vec<DemoFileEntry>,
}

fn native_roots() -> Vec<DirEntryLite> {
    #[cfg(target_os = "windows")]
    {
        (b'A'..=b'Z')
            .filter_map(|letter| {
                let drive = format!("{}:\\", letter as char);
                if PathBuf::from(&drive).is_dir() {
                    Some(DirEntryLite { name: drive.clone(), path: drive })
                } else {
                    None
                }
            })
            .collect()
    }
    #[cfg(not(target_os = "windows"))]
    {
        vec![DirEntryLite { name: "/".to_string(), path: "/".to_string() }]
    }
}

/// Lists the immediate subfolders and `.dem` files of `path`. `path: None`
/// returns the drive roots (Windows) or `/` (elsewhere) as the top of the tree.
#[tauri::command]
pub fn browse_directory(path: Option<String>) -> Result<DirListing, String> {
    let Some(path) = path else {
        return Ok(DirListing {
            path: None,
            parent: None,
            subdirs: native_roots(),
            demos: Vec::new(),
        });
    };

    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(format!("Not a directory: {}", path));
    }

    let mut subdirs = Vec::new();
    let mut demos = Vec::new();

    let entries = std::fs::read_dir(&dir).map_err(|e| format!("Failed to read {}: {}", path, e))?;
    for entry in entries.filter_map(Result::ok) {
        let entry_path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();

        if entry_path.is_dir() {
            if name.starts_with('.') || name.starts_with('$') {
                continue;
            }
            subdirs.push(DirEntryLite { name, path: entry_path.to_string_lossy().into_owned() });
        } else if entry_path
            .extension()
            .map(|ext| ext.eq_ignore_ascii_case("dem"))
            .unwrap_or(false)
        {
            let meta = entry.metadata().ok();
            let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified_unix_secs = meta
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            demos.push(DemoFileEntry {
                name,
                path: entry_path.to_string_lossy().into_owned(),
                size_bytes,
                modified_unix_secs,
            });
        }
    }

    subdirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    demos.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    // Drive roots (e.g. "C:\") have no useful parent; going "up" from one
    // should return to the drive-list root instead of erroring on `.parent()`.
    let parent = dir.parent().map(|p| p.to_string_lossy().into_owned());

    Ok(DirListing { path: Some(path), parent, subdirs, demos })
}

/// A sensible starting folder for the picker — the user's Documents
/// directory, falling back to their home directory.
#[tauri::command]
pub fn default_browse_dir() -> Option<String> {
    dirs::document_dir()
        .or_else(dirs::home_dir)
        .map(|p| p.to_string_lossy().into_owned())
}
