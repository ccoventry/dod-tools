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

// ── Recursive multi-folder demo listing (Analyzer browser: search/filter/
// sort/group over every demo across all watched folders at once, mirroring
// dev's `desktop_files` list — see docs/tauri_parity_audit.md Area 3). ──────

#[derive(Serialize)]
pub struct DemoListEntry {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub modified_unix_secs: f64,
    /// `None` when this demo isn't in the analyzer cache yet — the frontend
    /// lazily resolves these via `resolve_demo_summary` in the background
    /// instead of blocking the whole listing on a full parse per file.
    pub map_name: Option<String>,
    pub demo_type: Option<String>,
}

fn collect_dem_files(dir: &PathBuf, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name.starts_with('$') {
                continue;
            }
            collect_dem_files(&path, out);
        } else if path
            .extension()
            .map(|ext| ext.eq_ignore_ascii_case("dem"))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
}

/// Recursively walks every given folder for `.dem` files — filesystem-only,
/// no demo parsing, so this returns near-instantly regardless of library
/// size. `map_name`/`demo_type` are filled in from the analyzer cache when
/// already present (e.g. from a prior Capture Studio scan or Analyzer open)
/// and left `None` otherwise for the frontend to resolve lazily.
#[tauri::command]
pub async fn list_demos_recursive(paths: Vec<String>) -> Result<Vec<DemoListEntry>, String> {
    tokio::task::spawn_blocking(move || {
        let mut found = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for p in paths {
            let dir = PathBuf::from(&p);
            if !dir.is_dir() {
                continue;
            }
            let mut in_this_dir = Vec::new();
            collect_dem_files(&dir, &mut in_this_dir);
            for path in in_this_dir {
                if seen.insert(path.clone()) {
                    found.push(path);
                }
            }
        }

        found
            .into_iter()
            .map(|path| {
                let meta = std::fs::metadata(&path).ok();
                let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let modified_unix_secs = meta
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0);
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();

                let (map_name, demo_type) = native::peek_analyzer_cache(&path)
                    .map(|(_, analysis)| (Some(analysis.demo_info.map_name), Some(analysis.demo_info.demo_type)))
                    .unwrap_or((None, None));

                DemoListEntry {
                    name,
                    path: path.to_string_lossy().into_owned(),
                    size_bytes,
                    modified_unix_secs,
                    map_name,
                    demo_type,
                }
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))
}

#[derive(Serialize)]
pub struct DemoSummary {
    pub path: String,
    pub map_name: String,
    pub demo_type: String,
}

/// Lazy-fill counterpart to `list_demos_recursive`: does the full (possibly
/// ~1.3s cold) parse for one demo, via the same on-disk cache
/// `analyze_demo_full` uses, so the cost is paid once per demo rather than
/// once per browse. Called in the background per cache-miss row.
#[tauri::command]
pub async fn resolve_demo_summary(path: String) -> Result<DemoSummary, String> {
    tokio::task::spawn_blocking(move || {
        let pb = PathBuf::from(&path);
        let (_, analysis, _) = native::run_analyzer_cached(&pb, |_, _| {})?;
        Ok(DemoSummary {
            path,
            map_name: analysis.demo_info.map_name,
            demo_type: analysis.demo_info.demo_type,
        })
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}
