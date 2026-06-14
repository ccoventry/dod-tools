#[cfg(not(target_arch = "wasm32"))]
use crate::GuiMessage;
#[cfg(not(target_arch = "wasm32"))]
use egui::Context;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc;

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
pub struct SendWrapper<T>(pub T);

#[cfg(target_arch = "wasm32")]
unsafe impl<T> Send for SendWrapper<T> {}
#[cfg(target_arch = "wasm32")]
unsafe impl<T> Sync for SendWrapper<T> {}

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
pub struct WebFile {
    pub name: String,
    pub path: String,
    pub js_file: SendWrapper<web_sys::File>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
pub struct DirNode {
    pub name: String,
    pub path: String,
    pub subdirs: std::collections::BTreeMap<String, DirNode>,
}

#[cfg(target_arch = "wasm32")]
pub fn build_web_tree(files: &[WebFile]) -> DirNode {
    let mut root = DirNode {
        name: "[Root]".to_string(),
        path: ".".to_string(),
        subdirs: std::collections::BTreeMap::new(),
    };

    for file in files {
        let relative_path = &file.path;
        let parts: Vec<&str> = relative_path.split('/').collect();
        if parts.len() > 1 {
            let mut current = &mut root;
            for i in 0..parts.len() - 1 {
                let name = parts[i].to_string();
                let path = parts[0..=i].join("/");
                current = current
                    .subdirs
                    .entry(name.clone())
                    .or_insert_with(|| DirNode {
                        name,
                        path,
                        subdirs: std::collections::BTreeMap::new(),
                    });
            }
        }
    }
    root
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CachedDemo {
    pub path: String,
    pub name: String,
    pub map_name: String,
    pub date: String,
    pub demo_type: String,
    pub size_bytes: u64,
    pub modified_ms: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Default, Debug)]
pub struct DemoCache {
    pub demos: std::collections::HashMap<String, CachedDemo>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct DemoListItem {
    pub path: PathBuf,
    pub name: String,
    pub map_name: String,
    pub date: String,
    pub demo_type: String,
}

#[cfg(not(target_arch = "wasm32"))]
fn get_demo_map_name(path: &Path) -> String {
    use std::io::Read;
    if let Ok(mut file) = std::fs::File::open(path) {
        let mut header = [0u8; 276];
        if file.read_exact(&mut header).is_ok() {
            if &header[0..6] == b"HLDEMO" {
                let map_bytes = &header[16..];
                let len = map_bytes.iter().position(|&c| c == 0).unwrap_or(260);
                return String::from_utf8_lossy(&map_bytes[..len]).into_owned();
            }
        }
    }
    "-".to_string()
}

#[cfg(not(target_arch = "wasm32"))]
fn process_demo_file(p: PathBuf, cache: &DemoCache) -> DemoListItem {
    let name = p.file_name().unwrap().to_string_lossy().into_owned();
    let path_str = p.to_string_lossy().into_owned();

    let metadata = std::fs::metadata(&p).ok();
    let size_bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
    let modified_ms = metadata.as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Try to retrieve from cache first
    if let Some(cached) = cache.demos.get(&path_str) {
        if cached.size_bytes == size_bytes && cached.modified_ms == modified_ms {
            return DemoListItem {
                path: p,
                name: cached.name.clone(),
                map_name: cached.map_name.clone(),
                date: cached.date.clone(),
                demo_type: cached.demo_type.clone(),
            };
        }
    }

    // Cache miss or modified: read from file header
    let map_name = get_demo_map_name(&p);
    let date = if let Some(m) = metadata {
        if let Ok(created) = m.created().or_else(|_| m.modified()) {
            chrono::DateTime::<chrono::Local>::from(created)
                .format("%Y-%m-%d %I:%M %p")
                .to_string()
        } else {
            "-".to_string()
        }
    } else {
        "-".to_string()
    };

    // Pre-calculate demo type using filename heuristic as fallback
    let demo_type = if name.to_lowercase().contains("hltv") {
        "HLTV".to_string()
    } else {
        "POV".to_string()
    };

    DemoListItem {
        path: p,
        name,
        map_name,
        date,
        demo_type,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn get_dir_contents_parallel(path: &Path) -> Vec<DemoListItem> {
    // Load central cache from disk
    let cache_path = Path::new(".dod-tools-cache.json");
    let cache = if cache_path.exists() {
        std::fs::read_to_string(cache_path)
            .ok()
            .and_then(|content| serde_json::from_str::<DemoCache>(&content).ok())
            .unwrap_or_default()
    } else {
        DemoCache::default()
    };

    let cache_arc = std::sync::Arc::new(cache);
    let mut file_paths = vec![];
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(Result::ok) {
            let p = entry.path();
            if !p.is_dir() && p.extension().map_or(false, |ext| ext == "dem") {
                file_paths.push(p);
            }
        }
    }

    if file_paths.is_empty() {
        return vec![];
    }

    let num_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(file_paths.len());

    let mut results = vec![];

    if num_threads <= 1 {
        for p in file_paths {
            results.push(process_demo_file(p, &cache_arc));
        }
    } else {
        let (tx, rx) = std::sync::mpsc::channel();
        let chunk_size = (file_paths.len() + num_threads - 1) / num_threads;

        std::thread::scope(|s| {
            for chunk in file_paths.chunks(chunk_size) {
                let tx = tx.clone();
                let cache_ref = cache_arc.clone();
                s.spawn(move || {
                    for p in chunk {
                        let item = process_demo_file(p.clone(), &cache_ref);
                        let _ = tx.send(item);
                    }
                });
            }
        });

        drop(tx);

        while let Ok(item) = rx.recv() {
            results.push(item);
        }
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    results
}

#[cfg(not(target_arch = "wasm32"))]
pub fn scan_dir_async(ctx: Context, tx: mpsc::Sender<GuiMessage>, dir: PathBuf) {
    tokio::task::spawn_blocking(move || {
        let files = get_dir_contents_parallel(&dir);
        let _ = tx.send(GuiMessage::DirScanComplete { dir, files });
        ctx.request_repaint();
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn get_subdirs(path: &Path, cache: &mut HashMap<PathBuf, Vec<PathBuf>>) -> Vec<PathBuf> {
    if let Some(dirs) = cache.get(path) {
        return dirs.clone();
    }

    let mut dirs = vec![];
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(Result::ok) {
            let p = entry.path();
            if p.is_dir() {
                if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') || name.starts_with('$') {
                        continue;
                    }
                }
                dirs.push(p);
            }
        }
    }
    dirs.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    cache.insert(path.to_path_buf(), dirs.clone());
    dirs
}

#[cfg(not(target_arch = "wasm32"))]
pub fn get_native_roots() -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let mut drives = vec![];
        for letter in b'A'..=b'Z' {
            let drive_str = format!("{}:\\", letter as char);
            let drive_path = PathBuf::from(&drive_str);
            if drive_path.exists() {
                drives.push(drive_path);
            }
        }
        drives
    }
    #[cfg(not(target_os = "windows"))]
    {
        vec![PathBuf::from("/")]
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn count_demo_files(path: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(Result::ok) {
            let p = entry.path();
            if !p.is_dir() && p.extension().map_or(false, |ext| ext == "dem") {
                count += 1;
            }
        }
    }
    count
}

#[cfg(not(target_arch = "wasm32"))]
pub fn render_native_dir_node(
    ui: &mut egui::Ui,
    path: &Path,
    current_dir: Option<&Path>,
    next_dir: &mut Option<PathBuf>,
    cache: &mut HashMap<PathBuf, Vec<PathBuf>>,
    scan_folders: bool,
    demo_cache: &mut HashMap<PathBuf, usize>,
) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let is_selected = current_dir == Some(path);

    let demo_count = if scan_folders {
        *demo_cache.entry(path.to_path_buf()).or_insert_with(|| {
            count_demo_files(path)
        })
    } else {
        0
    };

    let folder_icon = if demo_count > 0 { "📂" } else { "📁" };
    let display_name = if demo_count > 0 {
        format!("{} ({})", name, demo_count)
    } else {
        name
    };

    let subdirs = get_subdirs(path, cache);
    let has_subdirs = !subdirs.is_empty();

    if !has_subdirs {
        ui.horizontal(|ui| {
            ui.add_enabled_ui(false, |ui| {
                ui.set_invisible();
                let _ = ui.selectable_label(false, "⏵");
            });
            if ui
                .selectable_label(is_selected, format!("{} {}", folder_icon, display_name))
                .clicked()
            {
                *next_dir = Some(path.to_path_buf());
            }
        });
    } else {
        let is_ancestor = current_dir.map_or(false, |curr| curr.starts_with(path) && curr != path);
        let collapsing_id = ui.make_persistent_id(path);
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            collapsing_id,
            false,
        );
        if is_ancestor {
            state.set_open(true);
            state.store(ui.ctx());
        }
        let is_open = state.is_open();

        ui.horizontal(|ui| {
            let symbol = if is_open { "⏷" } else { "⏵" };
            if ui.selectable_label(false, symbol).clicked() {
                state.toggle(ui);
                state.store(ui.ctx());
                if !state.is_open() {
                    if let Some(curr) = current_dir {
                        if curr.starts_with(path) && curr != path {
                            *next_dir = Some(path.to_path_buf());
                        }
                    }
                }
            }
            if ui
                .selectable_label(is_selected, format!("{} {}", folder_icon, display_name))
                .clicked()
            {
                *next_dir = Some(path.to_path_buf());
            }
        });

        if is_open {
            ui.indent(ui.make_persistent_id((path, "indent")), |ui| {
                for subdir in subdirs {
                    render_native_dir_node(
                        ui,
                        &subdir,
                        current_dir,
                        next_dir,
                        cache,
                        scan_folders,
                        demo_cache,
                    );
                }
            });
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn render_web_dir_node(ui: &mut egui::Ui, node: &DirNode, selected_folder: &mut String) {
    let is_selected = selected_folder == &node.path;
    let has_subdirs = !node.subdirs.is_empty();

    if !has_subdirs {
        ui.horizontal(|ui| {
            ui.add_enabled_ui(false, |ui| {
                ui.set_invisible();
                let _ = ui.selectable_label(false, "⏵");
            });
            if ui
                .selectable_label(is_selected, format!("📁 {}", node.name))
                .clicked()
            {
                *selected_folder = node.path.clone();
            }
        });
    } else {
        let collapsing_id = ui.make_persistent_id(&node.path);
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            collapsing_id,
            false,
        );
        let is_open = state.is_open();

        ui.horizontal(|ui| {
            let symbol = if is_open { "⏷" } else { "⏵" };
            if ui.selectable_label(false, symbol).clicked() {
                state.toggle(ui);
                state.store(ui.ctx());
                if !state.is_open() {
                    let prefix = format!("{}/", node.path);
                    if selected_folder.starts_with(&prefix) {
                        *selected_folder = node.path.clone();
                    }
                }
            }
            if ui
                .selectable_label(is_selected, format!("📁 {}", node.name))
                .clicked()
            {
                *selected_folder = node.path.clone();
            }
        });

        if is_open {
            ui.indent(ui.make_persistent_id((&node.path, "indent")), |ui| {
                for subdir in node.subdirs.values() {
                    render_web_dir_node(ui, subdir, selected_folder);
                }
            });
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn is_drive_root(path: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        let s = path.to_string_lossy();
        s.len() <= 3 && s.ends_with(":\\")
    }
    #[cfg(not(target_os = "windows"))]
    {
        path == Path::new("/")
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn scan_demo_folders_async(
    ctx: Context,
    tx: mpsc::Sender<GuiMessage>,
    root_dir: PathBuf,
    scan_id: usize,
) {
    tokio::task::spawn_blocking(move || {
        let mut folders = Vec::new();
        let walk_recursive = !is_drive_root(&root_dir);

        let mut stack = vec![(root_dir.clone(), 0)];
        let max_depth = 4;
        let mut folders_checked = 0;
        let max_folders_checked = 2000;

        while let Some((dir, depth)) = stack.pop() {
            folders_checked += 1;
            if folders_checked > max_folders_checked {
                break;
            }

            let mut demo_count = 0;
            let mut subdirs = Vec::new();

            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.filter_map(Result::ok) {
                    let path = entry.path();
                    if path.is_dir() {
                        if walk_recursive && depth < max_depth {
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                let name_lower = name.to_lowercase();
                                if !name.starts_with('.') 
                                    && !name.starts_with('$') 
                                    && name_lower != "target" 
                                    && name_lower != "node_modules"
                                    && name_lower != ".git"
                                    && name_lower != "src"
                                    && name_lower != "assets"
                                {
                                    subdirs.push(path);
                                }
                            }
                        }
                    } else if path.extension().map_or(false, |ext| ext == "dem") {
                        demo_count += 1;
                    }
                }
            }

            if demo_count > 0 {
                folders.push((dir, demo_count));
            }

            subdirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
            for subdir in subdirs {
                stack.push((subdir, depth + 1));
            }
        }

        folders.sort_by(|a, b| a.0.cmp(&b.0));

        let _ = tx.send(GuiMessage::DemoFoldersScanComplete { scan_id, folders });
        ctx.request_repaint();
    });
}
