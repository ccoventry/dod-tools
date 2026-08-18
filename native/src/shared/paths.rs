use std::path::{Path, PathBuf};

pub fn get_appdata_dir() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    path.push("dod-tools");
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Stable identity for one capture take, shared by the capture and render
/// pipelines so they can correlate without either knowing about the other.
///
/// Takes land at `<capture_dir>/<session_id>/chain_JJ_bN/`, so the key is the
/// last two path components lowercased: `session_20260818_142233/chain_01_b0`.
/// Deliberately *not* the absolute path — capture output routinely gets moved
/// onto a different drive before rendering, which would invalidate it — and
/// deliberately not the take name alone, which repeats every batch.
pub fn take_key(take_folder: &Path) -> Option<String> {
    let take_name = take_folder.file_name()?.to_string_lossy().to_lowercase();
    let session = take_folder.parent()?.file_name()?.to_string_lossy().to_lowercase();
    Some(format!("{}/{}", session, take_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_take_key_uses_last_two_components_lowercased() {
        let key = take_key(Path::new(r"D:\Captures\Session_20260818_142233\Chain_01_b0"));
        assert_eq!(key, Some("session_20260818_142233/chain_01_b0".to_string()));
    }

    #[test]
    fn test_take_key_is_stable_across_drives() {
        // The same take copied to a different drive must produce the same key —
        // this is the whole reason the absolute path isn't used.
        let a = take_key(Path::new(r"D:\Captures\session_1\chain_01_b0"));
        let b = take_key(Path::new(r"X:\somewhere\else\session_1\chain_01_b0"));
        assert_eq!(a, b);
        assert!(a.is_some());
    }

    #[test]
    fn test_take_key_distinguishes_sessions() {
        let a = take_key(Path::new(r"D:\c\session_1\chain_01_b0"));
        let b = take_key(Path::new(r"D:\c\session_2\chain_01_b0"));
        assert_ne!(a, b);
    }

    #[test]
    fn test_take_key_none_without_a_parent_component() {
        assert_eq!(take_key(Path::new("")), None);
    }
}
