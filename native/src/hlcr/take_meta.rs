//! What a take was actually captured at, recorded beside it.
//!
//! Render Studio's FPS is a per-job setting the user picks (the VirtualDub-style
//! model), and nothing ever connected it to the FPS the take was recorded at.
//! `-framerate` before the BMP input tells FFmpeg how to interpret the
//! sequence's timing, so a value disagreeing with the capture produces a wrong
//! computed duration and `-shortest` trims the audio against it: a 120fps take
//! rendered at 300 comes out 2.5x too fast, silently, and the render reports
//! success. `docs/engineering_backlog.md` has the full diagnosis — it was found
//! by ear, from a render that "sounds like a helicopter".
//!
//! There was no source of truth to check the render setting against. This is it:
//! one small file per capture session, written after the batch has been verified,
//! recording the `mirv_movie_fps` the takes under it were produced at.
//!
//! Deliberately **advisory**. Nothing here overrides the user's render setting —
//! a take folder can be moved, copied or hand-assembled, and a renderer that
//! silently substituted a number found in a neighbouring file would be a worse
//! surprise than the one it fixes. It exists so the disagreement can be *stated*.

use std::path::{Path, PathBuf};

/// Bump when the shape changes. A file with an unrecognised format is ignored
/// rather than guessed at — an advisory check that mis-reads is worse than one
/// that stays quiet.
pub const FORMAT: u32 = 1;

/// Lives in the session folder, one level above each block folder. Not in the
/// block folder itself: `take_folder_has_content` decides a take was captured by
/// asking whether that folder is non-empty, and a file we wrote would make an
/// empty take look successful.
pub const SESSION_FILE: &str = "dodtools_session.json";

/// How far up from a take folder to look. Takes land at
/// `<capture_dir>/<session_id>/chain_JJ_bN/take0000/`, so the session folder is
/// two or three levels up depending on whether the caller passed the block
/// folder or the nested take. Four bounds the walk without reaching a capture
/// drive's root, where a stray file would belong to nothing in particular.
const MAX_ANCESTOR_DEPTH: usize = 4;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct SessionMeta {
    pub format: u32,
    pub session_id: String,
    /// The `mirv_movie_fps` the batch was captured at.
    pub capture_fps: i32,
}

impl SessionMeta {
    pub fn new(session_id: impl Into<String>, capture_fps: i32) -> Self {
        Self {
            format: FORMAT,
            session_id: session_id.into(),
            capture_fps,
        }
    }
}

/// Writes the session's capture settings into `session_folder`.
///
/// Best-effort by design: a capture that succeeded must not be reported as
/// failed because a metadata file could not be written, so callers log the
/// error and carry on.
pub fn write(session_folder: &Path, meta: &SessionMeta) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(meta)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(session_folder.join(SESSION_FILE), json)
}

/// The capture settings recorded for a take, or `None` when there are none.
///
/// `None` is the normal answer for every take captured before this existed, and
/// for any folder assembled by hand — so it must read as "nothing to say", never
/// as a problem.
pub fn read_for_take(take_folder: &Path) -> Option<SessionMeta> {
    for dir in ancestors(take_folder) {
        let candidate = dir.join(SESSION_FILE);
        let Ok(bytes) = std::fs::read(&candidate) else {
            continue;
        };
        let Ok(meta) = serde_json::from_slice::<SessionMeta>(&bytes) else {
            continue;
        };
        if meta.format == FORMAT {
            return Some(meta);
        }
    }
    None
}

fn ancestors(take_folder: &Path) -> Vec<PathBuf> {
    take_folder
        .ancestors()
        .skip(1) // the take folder's own contents are not where this lives
        .take(MAX_ANCESTOR_DEPTH)
        .map(PathBuf::from)
        .collect()
}

/// The warning to show when a render is about to interpret a take at a rate it
/// was not captured at, or `None` when there is nothing to say.
///
/// Returns a message rather than logging, so the same wording can go to the
/// activity log and to a job's own render log without drifting apart.
pub fn fps_mismatch_warning(take_folder: &Path, render_fps: u32) -> Option<String> {
    let meta = read_for_take(take_folder)?;
    // A non-positive recorded rate is meaningless and would divide by zero, so
    // there is nothing trustworthy to compare against. The cast below is safe
    // only because of this guard.
    if meta.capture_fps <= 0 || meta.capture_fps as u32 == render_fps {
        return None;
    }
    // Frames are interpreted at the render rate, so the output runs fast when
    // the render rate is the higher of the two and slow when it is lower.
    let ratio = render_fps as f32 / meta.capture_fps as f32;
    Some(format!(
        "this take was captured at {} fps but is being rendered at {} — FFmpeg reads the frame \
         sequence at the render rate, so the result will run {:.2}x {} and the audio will be cut \
         to match. Set the render FPS to {} unless this is deliberate.",
        meta.capture_fps,
        render_fps,
        if ratio > 1.0 { ratio } else { 1.0 / ratio },
        if ratio > 1.0 { "fast" } else { "slow" },
        meta.capture_fps,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dod_take_meta_{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    /// The real layout: `<capture>/<session>/chain_01_b0/take0000/`.
    fn session_with_take(name: &str) -> (PathBuf, PathBuf) {
        let root = scratch(name);
        let session = root.join("session_20260827_120000");
        let take = session.join("chain_01_b0").join("take0000");
        std::fs::create_dir_all(&take).expect("take dirs");
        (session, take)
    }

    #[test]
    fn a_take_finds_the_session_it_belongs_to() {
        let (session, take) = session_with_take("finds");
        write(&session, &SessionMeta::new("session_20260827_120000", 120)).expect("write");

        // Both the nested take folder and the block folder above it resolve,
        // because Render Studio's scanner hands out either depending on how the
        // take was found.
        assert_eq!(read_for_take(&take).map(|m| m.capture_fps), Some(120));
        assert_eq!(
            read_for_take(take.parent().unwrap()).map(|m| m.capture_fps),
            Some(120)
        );
    }

    #[test]
    fn a_take_with_no_metadata_is_not_a_problem() {
        // Every take captured before this existed, and any folder assembled by
        // hand. Silence is the correct answer, not a warning.
        let (_session, take) = session_with_take("absent");
        assert_eq!(read_for_take(&take), None);
        assert_eq!(fps_mismatch_warning(&take, 300), None);
    }

    #[test]
    fn an_unreadable_or_future_format_is_ignored_rather_than_guessed() {
        let (session, take) = session_with_take("garbage");
        std::fs::write(session.join(SESSION_FILE), b"{not json").expect("write");
        assert_eq!(read_for_take(&take), None);

        let ahead = format!(
            r#"{{"format":{},"session_id":"s","capture_fps":120}}"#,
            FORMAT + 1
        );
        std::fs::write(session.join(SESSION_FILE), ahead).expect("write");
        assert_eq!(read_for_take(&take), None, "a newer format was read anyway");
    }

    #[test]
    fn a_matching_rate_says_nothing() {
        let (session, take) = session_with_take("match");
        write(&session, &SessionMeta::new("s", 120)).expect("write");
        assert_eq!(fps_mismatch_warning(&take, 120), None);
    }

    #[test]
    fn the_warning_states_the_direction_and_the_factor() {
        let (session, take) = session_with_take("mismatch");
        write(&session, &SessionMeta::new("s", 120)).expect("write");

        // The bug as it actually happened: captured at 120, rendered at 300.
        let fast = fps_mismatch_warning(&take, 300).expect("a mismatch must be reported");
        assert!(fast.contains("2.50x fast"), "{}", fast);
        assert!(fast.contains("Set the render FPS to 120"), "{}", fast);

        // And the other direction, which is just as wrong and reads differently.
        let slow = fps_mismatch_warning(&take, 60).expect("a mismatch must be reported");
        assert!(slow.contains("2.00x slow"), "{}", slow);
    }

    #[test]
    fn a_nonsense_recorded_rate_is_not_used_to_scold_the_user() {
        // A zero would divide by zero and a negative is meaningless; either way
        // there is nothing trustworthy to compare against.
        let (session, take) = session_with_take("zero");
        write(&session, &SessionMeta::new("s", 0)).expect("write");
        assert_eq!(fps_mismatch_warning(&take, 300), None);
    }

    #[test]
    fn the_walk_does_not_climb_out_of_the_session() {
        // A file this far up belongs to a capture drive, not to this take, and
        // claiming it does would be worse than saying nothing.
        let root = scratch("too_far");
        let deep = root
            .join("a")
            .join("b")
            .join("c")
            .join("d")
            .join("e")
            .join("take0000");
        std::fs::create_dir_all(&deep).expect("dirs");
        write(&root, &SessionMeta::new("s", 120)).expect("write");
        assert_eq!(read_for_take(&deep), None);
    }
}
