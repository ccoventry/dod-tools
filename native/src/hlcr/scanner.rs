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

/// `.wav` files sitting directly in a take folder, sorted case-insensitively
/// so take selection is deterministic.
fn collect_wav_files(take_folder: &Path) -> Vec<String> {
    let mut wav_files = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(take_folder) {
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
    wav_files.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    wav_files
}

/// Immediate subdirectories of a take folder that hold an HLAE frame sequence,
/// identified by a `00000.bmp` first frame.
fn collect_image_folders(take_folder: &Path) -> Vec<PathBuf> {
    let mut image_folders = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(take_folder) {
        for sub_entry in read_dir.flatten() {
            if let Ok(file_type) = sub_entry.file_type() {
                if file_type.is_dir() && sub_entry.path().join("00000.bmp").exists() {
                    image_folders.push(sub_entry.path());
                }
            }
        }
    }
    image_folders
}

/// Whether Render Studio's scanner would admit this folder as a renderable take.
///
/// Shared with the capture-side take verification so "the capture succeeded"
/// and "Render Studio can actually see it" can never silently disagree — if
/// this predicate changes, both sides change together.
///
/// HLAE's `mirv_movie` plugin auto-numbers each recording into a `take0000`,
/// `take0001`, ... subfolder under whatever directory `mirv_movie_filename`
/// points at, to avoid overwriting a previous take written to the same path —
/// so the wav/bmp sequence actually lands one level deeper than the folder we
/// asked it to write to. `scan_folder_background` below dodges this for free
/// because `WalkDir` recurses into every subdirectory on its own; this check
/// has to look explicitly since it only tests one specific folder.
pub fn is_renderable_take(take_folder: &Path) -> bool {
    if !collect_wav_files(take_folder).is_empty() && !collect_image_folders(take_folder).is_empty() {
        return true;
    }
    if let Ok(read_dir) = std::fs::read_dir(take_folder) {
        for entry in read_dir.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
                && entry.file_name().to_string_lossy().to_lowercase().starts_with("take")
            {
                let sub = entry.path();
                if !collect_wav_files(&sub).is_empty() && !collect_image_folders(&sub).is_empty() {
                    return true;
                }
            }
        }
    }
    false
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

            let wav_files = collect_wav_files(&take_folder);
            if wav_files.is_empty() {
                continue;
            }

            let mut image_folders = collect_image_folders(&take_folder);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dod_scanner_test_{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
        dir
    }

    fn write_frames(take: &Path, stream: &str) {
        let folder = take.join(stream);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("00000.bmp"), b"bmp").unwrap();
    }

    #[test]
    fn test_renderable_take_needs_both_wav_and_frames() {
        let take = scratch_dir("complete");
        std::fs::write(take.join("sound.wav"), b"wav").unwrap();
        write_frames(&take, "all");
        assert!(is_renderable_take(&take));
    }

    #[test]
    fn test_take_without_wav_is_not_renderable() {
        // The realistic partial-capture case: frames landed, audio never flushed.
        let take = scratch_dir("no_wav");
        write_frames(&take, "all");
        assert!(!is_renderable_take(&take));
    }

    #[test]
    fn test_take_without_frames_is_not_renderable() {
        let take = scratch_dir("no_frames");
        std::fs::write(take.join("sound.wav"), b"wav").unwrap();
        assert!(!is_renderable_take(&take));
    }

    #[test]
    fn test_subfolder_without_first_frame_does_not_count() {
        // A frame folder is identified by 00000.bmp specifically — an empty or
        // partially-written stream folder must not qualify.
        let take = scratch_dir("empty_stream");
        std::fs::write(take.join("sound.wav"), b"wav").unwrap();
        std::fs::create_dir_all(take.join("all")).unwrap();
        assert!(!is_renderable_take(&take));
    }

    #[test]
    fn test_missing_take_folder_is_not_renderable() {
        let missing = std::env::temp_dir().join("dod_scanner_test_does_not_exist");
        let _ = std::fs::remove_dir_all(&missing);
        assert!(!is_renderable_take(&missing));
    }

    #[test]
    fn test_renderable_take_nested_under_hlae_take_number_folder() {
        // HLAE's mirv_movie plugin auto-numbers each recording into a
        // take0000, take0001, ... subfolder under the directory we point
        // mirv_movie_filename at, to avoid overwriting a previous take
        // written to the same path — confirmed on a real capture where the
        // block folder itself was empty except for exactly this layout.
        let block_folder = scratch_dir("nested_under_take_number");
        let take0000 = block_folder.join("take0000");
        std::fs::create_dir_all(&take0000).unwrap();
        std::fs::write(take0000.join("sound.wav"), b"wav").unwrap();
        write_frames(&take0000, "all");
        assert!(is_renderable_take(&block_folder));
    }
}
