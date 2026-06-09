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
                current = current.subdirs.entry(name.clone()).or_insert_with(|| DirNode {
                    name,
                    path,
                    subdirs: std::collections::BTreeMap::new(),
                });
            }
        }
    }
    root
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct DemoListItem {
    pub path: PathBuf,
    pub name: String,
    pub map_name: String,
    pub date: String,
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
fn process_demo_file(p: PathBuf) -> DemoListItem {
    let name = p.file_name().unwrap().to_string_lossy().into_owned();
    let map_name = get_demo_map_name(&p);
    let date = if let Ok(metadata) = std::fs::metadata(&p) {
        if let Ok(created) = metadata.created().or_else(|_| metadata.modified()) {
            chrono::DateTime::<chrono::Local>::from(created)
                .format("%Y-%m-%d %I:%M %p")
                .to_string()
        } else {
            "-".to_string()
        }
    } else {
        "-".to_string()
    };

    DemoListItem {
        path: p,
        name,
        map_name,
        date,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn get_dir_contents_parallel(path: &Path) -> Vec<DemoListItem> {
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
            results.push(process_demo_file(p));
        }
    } else {
        let (tx, rx) = std::sync::mpsc::channel();
        let chunk_size = (file_paths.len() + num_threads - 1) / num_threads;

        std::thread::scope(|s| {
            for chunk in file_paths.chunks(chunk_size) {
                let tx = tx.clone();
                s.spawn(move || {
                    for p in chunk {
                        let item = process_demo_file(p.clone());
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
fn get_subdirs(
    path: &Path,
    cache: &mut HashMap<PathBuf, Vec<PathBuf>>,
) -> Vec<PathBuf> {
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
pub fn render_native_dir_node(
    ui: &mut egui::Ui,
    path: &Path,
    current_dir: Option<&Path>,
    next_dir: &mut Option<PathBuf>,
    cache: &mut HashMap<PathBuf, Vec<PathBuf>>,
) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let is_selected = current_dir == Some(path);

    let subdirs = get_subdirs(path, cache);
    let has_subdirs = !subdirs.is_empty();

    if !has_subdirs {
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            if ui.selectable_label(is_selected, format!("📁 {}", name)).clicked() {
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
            }
            if ui.selectable_label(is_selected, format!("📁 {}", name)).clicked() {
                *next_dir = Some(path.to_path_buf());
            }
        });

        if is_open {
            ui.indent(ui.make_persistent_id((path, "indent")), |ui| {
                for subdir in subdirs {
                    render_native_dir_node(ui, &subdir, current_dir, next_dir, cache);
                }
            });
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn render_web_dir_node(
    ui: &mut egui::Ui,
    node: &DirNode,
    selected_folder: &mut String,
) {
    let is_selected = selected_folder == &node.path;
    let has_subdirs = !node.subdirs.is_empty();

    if !has_subdirs {
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            if ui.selectable_label(is_selected, format!("📁 {}", node.name)).clicked() {
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
            }
            if ui.selectable_label(is_selected, format!("📁 {}", node.name)).clicked() {
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
