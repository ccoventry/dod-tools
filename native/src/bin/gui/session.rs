use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;
use native::utils::demo_hasher::calculate_demo_key;
use walkdir::WalkDir;

use native::patch::HighlightStatus;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HighlightMetadata {
    pub is_selected: bool,
    pub start_kill: i32,
    pub end_kill: i32,
    #[serde(default)]
    pub status: HighlightStatus,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DemoEntry {
    pub path: PathBuf,
    pub key: Option<(u64, u64)>,
    #[serde(default)]
    pub highlights: Vec<HighlightMetadata>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionData {
    pub entries: Vec<DemoEntry>,
}

pub async fn import_session_async(base_dir: PathBuf, session_data: Vec<DemoEntry>) -> Vec<(PathBuf, Vec<HighlightMetadata>)> {
    tokio::task::spawn_blocking(move || {
        let mut resolved_data = Vec::new();
        let mut index = HashMap::new();
        let mut index_built = false;

        for entry in session_data {
            if entry.path.exists() {
                resolved_data.push((entry.path.clone(), entry.highlights.clone()));
            } else if let Some(target_key) = entry.key {
                if !index_built {
                    for walk_entry in WalkDir::new(&base_dir).into_iter().filter_map(|e| e.ok()) {
                        let path = walk_entry.path();
                        if path.is_file() && path.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("dem")) {
                            if let Some(key) = calculate_demo_key(path) {
                                index.insert(key, path.to_path_buf());
                            }
                        }
                    }
                    index_built = true;
                }

                if let Some(found_path) = index.get(&target_key) {
                    resolved_data.push((found_path.clone(), entry.highlights.clone()));
                }
            }
        }
        resolved_data
    })
    .await
    .unwrap_or_default()
}
