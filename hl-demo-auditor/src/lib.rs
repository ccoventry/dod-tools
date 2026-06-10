use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Eq, PartialEq, Hash, Debug, Clone)]
pub struct FileKey {
    pub size: u64,
    pub header_hash: u64,
}

#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    pub key: FileKey,
    pub original: PathBuf,
    pub duplicates: Vec<PathBuf>,
}

/// Recursively scans a directory for `.dem` files.
pub fn scan_dir(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_dir(&path, files);
            } else if path.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("dem")) {
                files.push(path);
            }
        }
    }
}

/// Generates a `FileKey` for a file based on its size and a hash of its first 64KB.
pub fn get_file_key(path: &Path) -> Result<FileKey, std::io::Error> {
    let metadata = fs::metadata(path)?;
    let size = metadata.len();
    
    // Hash the first 64KB (or less if the file is smaller)
    let mut file = fs::File::open(path)?;
    let read_size = std::cmp::min(size, 65536) as usize;
    let mut buffer = vec![0; read_size];
    file.read_exact(&mut buffer)?;
    
    let mut hasher = DefaultHasher::new();
    buffer.hash(&mut hasher);
    let header_hash = hasher.finish();
    
    Ok(FileKey { size, header_hash })
}

/// Identifies duplicates within a list of file paths.
/// Returns unique files, structured duplicate groups, total duplicates count, and total wasted space in bytes.
pub fn find_duplicates(files: Vec<PathBuf>) -> (Vec<PathBuf>, Vec<DuplicateGroup>, usize, u64) {
    // Canonicalize and deduplicate paths to prevent counting/grouping a file with itself
    let mut unique_paths = std::collections::HashSet::new();
    for path in files {
        if let Ok(canonical) = fs::canonicalize(&path) {
            unique_paths.insert(canonical);
        } else {
            unique_paths.insert(path);
        }
    }
    let files: Vec<PathBuf> = unique_paths.into_iter().collect();

    let mut groups: HashMap<FileKey, Vec<PathBuf>> = HashMap::new();
    for path in files {
        if let Ok(key) = get_file_key(&path) {
            groups.entry(key).or_default().push(path);
        } else {
            // Unreadable files get a unique key based on path
            let mut hasher = DefaultHasher::new();
            path.hash(&mut hasher);
            let dummy_key = FileKey { size: 0, header_hash: hasher.finish() };
            groups.entry(dummy_key).or_default().push(path);
        }
    }

    // Sort paths alphabetically within each group so original vs duplicate selection is stable
    for paths in groups.values_mut() {
        paths.sort();
    }

    let mut unique_files = vec![];
    let mut duplicate_groups = vec![];
    let mut duplicate_count = 0;
    let mut space_wasted_bytes: u64 = 0;

    for (key, paths) in groups {
        if !paths.is_empty() {
            unique_files.push(paths[0].clone());
            if paths.len() > 1 {
                duplicate_groups.push(DuplicateGroup {
                    key: key.clone(),
                    original: paths[0].clone(),
                    duplicates: paths[1..].to_vec(),
                });
                duplicate_count += paths.len() - 1;
                space_wasted_bytes += key.size * (paths.len() - 1) as u64;
            }
        }
    }

    // Sort unique files alphabetically so traversal is deterministic
    unique_files.sort();

    // Sort duplicate groups by wasted space descending
    duplicate_groups.sort_by(|a, b| {
        let space_a = a.key.size * a.duplicates.len() as u64;
        let space_b = b.key.size * b.duplicates.len() as u64;
        space_b.cmp(&space_a)
    });

    (unique_files, duplicate_groups, duplicate_count, space_wasted_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_demos_are_unique() {
        let demos_dir = Path::new("../demos");
        if !demos_dir.exists() {
            return;
        }
        let mut files = vec![];
        scan_dir(demos_dir, &mut files);
        
        let (_, duplicates, dup_count, wasted) = find_duplicates(files);
        
        // Assert that none of our test demos (including both allied/axis POVs) are duplicates
        assert_eq!(dup_count, 0, "Demos in test folder should all be unique, but found duplicates: {:?}", duplicates);
        assert_eq!(wasted, 0);
    }

    #[test]
    fn test_duplicate_paths_deduplicated() {
        let demos_dir = Path::new("../demos");
        if !demos_dir.exists() {
            return;
        }
        let mut files = vec![];
        scan_dir(demos_dir, &mut files);
        
        // Intentionally duplicate the entire list of file paths
        let mut duplicated_list = files.clone();
        duplicated_list.extend(files);
        
        let (_, duplicates, dup_count, wasted) = find_duplicates(duplicated_list);
        
        // They should be deduplicated by path, resulting in 0 duplicate groups
        assert_eq!(dup_count, 0, "Duplicate paths should be resolved and not reported as duplicates: {:?}", duplicates);
        assert_eq!(wasted, 0);
    }
}
