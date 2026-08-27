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
/// Takes land at `<capture_dir>/<session_id>/chain_JJ_bN/`, so the key is
/// normally the last two path components lowercased:
/// `session_20260818_142233/chain_01_b0`. Deliberately *not* the absolute
/// path — capture output routinely gets moved onto a different drive before
/// rendering, which would invalidate it — and deliberately not the take name
/// alone, which repeats every batch.
///
/// HLAE's `mirv_movie` plugin auto-numbers each recording into a `take0000`,
/// `take0001`, ... subfolder *under* the block folder (`hlcr::scanner`'s own
/// `is_renderable_take` has to account for the same nesting) — Render
/// Studio's real folder scanner finds takes at that nested path, not the
/// block folder itself, so a trailing `take*` component is skipped to keep
/// both sides keying off the same block folder regardless of which literal
/// path was passed in.
pub fn take_key(take_folder: &Path) -> Option<String> {
    let is_take_number_folder = take_folder
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase().starts_with("take"))
        .unwrap_or(false);
    let block_folder = if is_take_number_folder {
        take_folder.parent()?
    } else {
        take_folder
    };
    let take_name = block_folder.file_name()?.to_string_lossy().to_lowercase();
    let session = block_folder.parent()?.file_name()?.to_string_lossy().to_lowercase();
    Some(format!("{}/{}", session, take_name))
}

/// Deletes the engine's `qconsole.log`.
///
/// `-condebug` writes it to `hl.exe`'s own folder, not the mod folder, so
/// cleanup that named `dod/qconsole.log` never matched anything and the log
/// accumulated across every session indefinitely. (`condump` is the separate
/// console command that drops a numbered `condump_NNN.txt`; nothing here
/// issues it.)
#[cfg(not(target_arch = "wasm32"))]
pub fn remove_console_log(game_root: &Path) {
    let _ = std::fs::remove_file(game_root.join("qconsole.log"));
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

    #[test]
    fn test_take_key_matches_across_the_take_number_nesting() {
        // The capture side (native/src/patch/builder.rs) computes take_key
        // from the block folder it asked HLAE to write to. The render side
        // (render_manager.rs) computes it from ClipData.take_folder, which
        // scan_folder_background sets to wherever it actually found the
        // wav/bmp — one level deeper, inside HLAE's own take0000 auto-numbered
        // subfolder. Both must resolve to the same key or auto-Rendered can
        // never correlate a finished render back to its highlights.
        let capture_side = take_key(Path::new(r"D:\Captures\session_1\chain_01_b0"));
        let render_side = take_key(Path::new(r"D:\Captures\session_1\chain_01_b0\take0000"));
        assert_eq!(capture_side, render_side);
        assert_eq!(capture_side, Some("session_1/chain_01_b0".to_string()));
    }

    #[test]
    fn test_take_key_handles_higher_numbered_takes() {
        let key = take_key(Path::new(r"D:\Captures\session_1\chain_01_b0\take0003"));
        assert_eq!(key, Some("session_1/chain_01_b0".to_string()));
    }

    #[test]
    fn console_log_is_cleared_beside_hl_exe() {
        let root = std::env::temp_dir().join(format!("dod_qconsole_root_{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let log = root.join("qconsole.log");
        std::fs::write(&log, b"console spam").unwrap();

        remove_console_log(&root);

        assert!(!log.exists(), "qconsole.log beside hl.exe should be removed");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Cleanup runs whether or not the engine was launched with `-condebug`,
    /// so an absent log is ordinary rather than an error worth surfacing.
    #[test]
    fn a_missing_console_log_is_not_an_error() {
        let root = std::env::temp_dir().join(format!("dod_qconsole_none_{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();

        remove_console_log(&root);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Nothing but that one file may be touched — the game folder holds the
    /// user's own configs, and anything wider here would be unrecoverable.
    #[test]
    fn nothing_but_the_console_log_is_touched() {
        let root = std::env::temp_dir().join(format!("dod_qconsole_keep_{}", std::process::id()));
        let dod = root.join("dod");
        std::fs::create_dir_all(&dod).unwrap();
        let keep = [
            dod.join("config.cfg"),
            dod.join("movie.cfg"),
            root.join("debug.log"),
            root.join("condump_001.txt"),
        ];
        for f in &keep {
            std::fs::write(f, b"mine").unwrap();
        }
        std::fs::write(root.join("qconsole.log"), b"spam").unwrap();

        remove_console_log(&root);

        for f in &keep {
            assert!(f.exists(), "{:?} is the user's file and must survive", f);
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
