#![cfg(not(target_arch = "wasm32"))]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use walkdir::WalkDir;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ClipData {
    pub take_folder: String,
    pub clip_type: String, // "single" or "hud_only"
    pub img_folder: String,
    pub wav_file: String,
    pub base_name: String,
    pub frame_count: usize,
    pub date: String,
}

pub fn scan_folder_background(
    source_folders: Vec<PathBuf>,
    tx: mpsc::Sender<ClipData>,
    status_tx: mpsc::Sender<String>,
) -> usize {
    let mut count = 0;
    let mut processed_folders = HashSet::new();
    let mut accumulated_clips = Vec::new();

    for source_folder in source_folders {
        if !source_folder.exists() || !source_folder.is_dir() {
            continue;
        }

        for entry in WalkDir::new(&source_folder)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_dir() {
                continue;
            }

            let take_folder = entry.path().to_path_buf();
            if processed_folders.contains(&take_folder) {
                continue;
            }

            // Identify wav files in the current directory
            let mut wav_files = Vec::new();
            if let Ok(read_dir) = std::fs::read_dir(&take_folder) {
                for sub_entry in read_dir.flatten() {
                    if let Ok(file_type) = sub_entry.file_type() {
                        if file_type.is_file() {
                            let path = sub_entry.path();
                            if let Some(ext) = path.extension() {
                                if ext.to_string_lossy().to_lowercase() == "wav" {
                                    if let Some(name) = path.file_name() {
                                        wav_files.push(name.to_string_lossy().into_owned());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if wav_files.is_empty() {
                continue;
            }

            // Sort alphabetically to be deterministic
            wav_files.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

            // Scan subdirectories for HLAE frame outputs
            let mut image_folders = Vec::new();
            if let Ok(read_dir) = std::fs::read_dir(&take_folder) {
                for sub_entry in read_dir.flatten() {
                    if let Ok(file_type) = sub_entry.file_type() {
                        if file_type.is_dir() {
                            let bmp_check = sub_entry.path().join("00000.bmp");
                            if bmp_check.exists() {
                                image_folders.push(sub_entry.path());
                            }
                        }
                    }
                }
            }

            if image_folders.is_empty() {
                continue;
            }

            // Valid take found!
            processed_folders.insert(take_folder.clone());
            let _ = status_tx.send(format!("Found take: {}", take_folder.file_name().unwrap_or_default().to_string_lossy()));

            // Prioritize sound.wav if it exists
            let sound_wav_exists = wav_files.iter().any(|f| f.to_lowercase() == "sound.wav");
            let wav_to_use = if sound_wav_exists {
                "sound.wav".to_string()
            } else {
                wav_files[0].clone()
            };

            let wav_path = Path::new(&wav_to_use);
            let wav_stem = wav_path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            let take_name = take_folder.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let demo_name = take_folder.parent()
                .and_then(|p| p.file_name())
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();

            let base_name = if wav_stem.to_lowercase() == "sound" {
                format!("{}-{}-{}", demo_name, take_name, wav_stem)
            } else {
                wav_stem.clone()
            };

            let folder_names: HashMap<String, PathBuf> = image_folders.iter()
                .map(|p| (p.file_name().unwrap_or_default().to_string_lossy().into_owned(), p.clone()))
                .collect();

            // Bundle HLAE split streams if "all", "hudcolor", and "hudalpha" exist
            if folder_names.contains_key("all") && folder_names.contains_key("hudcolor") && folder_names.contains_key("hudalpha") {
                let all_folder = folder_names.get("all").unwrap();
                let frame_count = count_bmps(all_folder);
                let date = get_clip_date(all_folder);

                let clip_all = ClipData {
                    take_folder: take_folder.to_string_lossy().into_owned(),
                    clip_type: "single".to_string(),
                    img_folder: "all".to_string(),
                    wav_file: wav_to_use.clone(),
                    base_name: base_name.clone(),
                    frame_count,
                    date: date.clone(),
                };
                accumulated_clips.push(clip_all);

                let clip_hud = ClipData {
                    take_folder: take_folder.to_string_lossy().into_owned(),
                    clip_type: "hud_only".to_string(),
                    img_folder: "hudcolor".to_string(),
                    wav_file: wav_to_use.clone(),
                    base_name: base_name.clone(),
                    frame_count,
                    date: date.clone(),
                };
                accumulated_clips.push(clip_hud);

                // Remove bundled folders from list to avoid double-processing
                image_folders.retain(|p| {
                    let name = p.file_name().unwrap_or_default().to_string_lossy();
                    name != "all" && name != "hudcolor" && name != "hudalpha"
                });
            }

            // Process remaining folders
            for img_folder in image_folders {
                let frame_count = count_bmps(&img_folder);
                let folder_name = img_folder.file_name().unwrap_or_default().to_string_lossy().into_owned();
                let date = get_clip_date(&img_folder);
                let clip = ClipData {
                    take_folder: take_folder.to_string_lossy().into_owned(),
                    clip_type: "single".to_string(),
                    img_folder: folder_name,
                    wav_file: wav_to_use.clone(),
                    base_name: base_name.clone(),
                    frame_count,
                    date,
                };
                accumulated_clips.push(clip);
            }
        }
    }

    // Deterministic sorting
    accumulated_clips.sort_by(|a, b| {
        a.take_folder.cmp(&b.take_folder)
            .then_with(|| a.img_folder.cmp(&b.img_folder))
            .then_with(|| a.clip_type.cmp(&b.clip_type))
    });

    for clip in accumulated_clips {
        let _ = tx.send(clip);
        count += 1;
    }

    count
}

fn count_bmps(folder: &Path) -> usize {
    let mut count = 0;
    if let Ok(read_dir) = std::fs::read_dir(folder) {
        for entry in read_dir.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() {
                    let path = entry.path();
                    if let Some(ext) = path.extension() {
                        if ext.to_string_lossy().to_lowercase() == "bmp" {
                            count += 1;
                        }
                    }
                }
            }
        }
    }
    count
}

fn get_clip_date(img_folder_path: &Path) -> String {
    let bmp_path = img_folder_path.join("00000.bmp");
    if let Ok(metadata) = std::fs::metadata(&bmp_path).or_else(|_| std::fs::metadata(img_folder_path)) {
        if let Ok(created) = metadata.created().or_else(|_| metadata.modified()) {
            return chrono::DateTime::<chrono::Local>::from(created)
                .format("%Y-%m-%d %I:%M %p")
                .to_string();
        }
    }
    "-".to_string()
}
