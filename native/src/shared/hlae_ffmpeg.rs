//! Whether HLAE can reach an FFmpeg, which is a different question from whether
//! *we* can.
//!
//! `mirv_movie_ffmpeg` makes HLAE spawn FFmpeg itself, and HLAE does not consult
//! the app's own resolution chain (User Override → Bundled → System Path). Per
//! `<HLAE>/ffmpeg/readme.advancedfx.txt` it looks in exactly two places:
//!
//!   - `<HLAE>/ffmpeg/bin/ffmpeg.exe`, or
//!   - the path named by `[Ffmpeg] Path=` in `<HLAE>/ffmpeg/ffmpeg.ini`.
//!
//! With neither present the feature fails in the least helpful way available: a
//! capture that runs to completion and produces no video. So the state is worth
//! reporting before a batch, not after.
//!
//! **Linking writes a two-line ini rather than copying the binary.** A copy
//! duplicates ~100 MB and creates a second FFmpeg to keep in step with the one
//! Render Studio uses; they drift, and then the two halves of the pipeline
//! encode with different builds.
//!
//! **An existing ini is never overwritten.** HLAE is shared with Source work and
//! other projects, so silently repointing it would break somebody else's
//! workflow to fix ours. Where one exists and disagrees, that is reported and
//! left alone — the same discipline `patch::cfg_scan` applies to the game's own
//! `.cfg` files. See `docs/direct_to_video_capture.md`.

#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};

const FFMPEG_DIR: &str = "ffmpeg";
const INI_NAME: &str = "ffmpeg.ini";
const BUNDLED_RELATIVE: &str = "bin/ffmpeg.exe";

/// What HLAE would find if it looked right now.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum HlaeFfmpeg {
    /// A binary sits in HLAE's own folder. It wins over any ini, so nothing
    /// else matters and nothing needs doing.
    Bundled { path: PathBuf },
    /// An `ffmpeg.ini` names a path. `target_exists` is reported separately
    /// because a stale ini pointing at a moved FFmpeg looks configured and is
    /// not — exactly the case a "configured / not configured" boolean would
    /// hide.
    Linked {
        ini: PathBuf,
        target: PathBuf,
        target_exists: bool,
    },
    /// HLAE's ffmpeg folder exists but holds neither. `mirv_movie_ffmpeg` will
    /// produce nothing.
    Missing { folder: PathBuf },
    /// The configured HLAE path does not resolve to an install at all, so this
    /// question cannot be answered yet. Distinct from `Missing`: nothing here
    /// is worth offering to fix.
    NoInstall,
}

impl HlaeFfmpeg {
    /// Whether HLAE would actually find something usable.
    pub fn is_usable(&self) -> bool {
        match self {
            HlaeFfmpeg::Bundled { .. } => true,
            HlaeFfmpeg::Linked { target_exists, .. } => *target_exists,
            _ => false,
        }
    }

    /// Whether offering to link would make sense. False when a binary is
    /// already bundled, when an ini already exists (never overwritten), and
    /// when there is no install to write into.
    pub fn can_link(&self) -> bool {
        matches!(self, HlaeFfmpeg::Missing { .. })
    }
}

#[derive(Debug)]
pub enum LinkError {
    /// No HLAE install at the configured path.
    NoInstall,
    /// An `ffmpeg.ini` is already there. Refused rather than overwritten.
    AlreadyLinked { ini: PathBuf, target: PathBuf },
    /// A binary is already bundled, so an ini would be ignored anyway.
    AlreadyBundled { path: PathBuf },
    /// The FFmpeg offered is not a file that exists.
    NoSuchFfmpeg { path: PathBuf },
    Io(std::io::Error),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkError::NoInstall => write!(f, "no HLAE install at the configured path"),
            LinkError::AlreadyLinked { ini, target } => write!(
                f,
                "{} already points at {} — left alone, since HLAE is shared with other projects",
                ini.display(),
                target.display()
            ),
            LinkError::AlreadyBundled { path } => write!(
                f,
                "HLAE already has its own FFmpeg at {}, which takes precedence over any ini",
                path.display()
            ),
            LinkError::NoSuchFfmpeg { path } => {
                write!(f, "no FFmpeg at {}", path.display())
            }
            LinkError::Io(e) => write!(f, "{}", e),
        }
    }
}

/// HLAE's `ffmpeg` folder, given the path to `HLAE.exe`.
///
/// The configured value names the executable, not the folder — the same
/// convention `game_path` follows, and the same trap: reading it as a directory
/// resolves one level too high and silently looks somewhere else.
pub fn ffmpeg_dir(hlae_exe: &Path) -> Option<PathBuf> {
    // The executable itself has to be there. Checking only the parent would
    // accept a path whose folder happens to exist — a typo'd exe name, or the
    // folder typed in place of the exe — and then offer to write an ini into
    // somewhere that is not an HLAE install at all.
    if !hlae_exe.is_file() {
        return None;
    }
    Some(hlae_exe.parent()?.join(FFMPEG_DIR))
}

pub fn detect(hlae_exe: &Path) -> HlaeFfmpeg {
    let Some(folder) = ffmpeg_dir(hlae_exe) else {
        return HlaeFfmpeg::NoInstall;
    };

    // A bundled binary wins: HLAE finds it without consulting the ini at all,
    // so reporting the ini here would describe something with no effect.
    let bundled = folder.join(BUNDLED_RELATIVE);
    if bundled.is_file() {
        return HlaeFfmpeg::Bundled { path: bundled };
    }

    let ini = folder.join(INI_NAME);
    if let Some(target) = std::fs::read_to_string(&ini).ok().as_deref().and_then(parse_ini_path) {
        let target_exists = target.is_file();
        return HlaeFfmpeg::Linked {
            ini,
            target,
            target_exists,
        };
    }

    HlaeFfmpeg::Missing { folder }
}

/// Points HLAE at `ffmpeg_exe` by writing `ffmpeg.ini`.
///
/// Refuses rather than overwrites whenever HLAE already has an answer, so this
/// can never take a working setup away from whatever else uses this install.
pub fn link(hlae_exe: &Path, ffmpeg_exe: &Path) -> Result<PathBuf, LinkError> {
    if !ffmpeg_exe.is_file() {
        return Err(LinkError::NoSuchFfmpeg {
            path: ffmpeg_exe.to_path_buf(),
        });
    }

    match detect(hlae_exe) {
        HlaeFfmpeg::NoInstall => return Err(LinkError::NoInstall),
        HlaeFfmpeg::Bundled { path } => return Err(LinkError::AlreadyBundled { path }),
        HlaeFfmpeg::Linked { ini, target, .. } => {
            return Err(LinkError::AlreadyLinked { ini, target })
        }
        HlaeFfmpeg::Missing { .. } => {}
    }

    let folder = ffmpeg_dir(hlae_exe).ok_or(LinkError::NoInstall)?;
    std::fs::create_dir_all(&folder).map_err(LinkError::Io)?;
    let ini = folder.join(INI_NAME);

    // The readme's own format. The comment is for whoever opens this months
    // from now wondering where it came from.
    let body = format!(
        "; Written by dod-tools so HLAE's mirv_movie_ffmpeg can find FFmpeg.\n\
         ; Delete this file to undo it; dod-tools will never overwrite it.\n\
         [Ffmpeg]\n\
         Path={}\n",
        ffmpeg_exe.display()
    );
    std::fs::write(&ini, body).map_err(LinkError::Io)?;
    Ok(ini)
}

/// An absolute path for the FFmpeg the app itself would use.
///
/// Render Studio's config stores a bare `"ffmpeg"` to mean "whatever is on
/// PATH", which HLAE cannot act on — its ini needs a real path — so that case
/// is resolved here rather than written through as-is.
pub fn resolve_absolute(configured: &str) -> Option<PathBuf> {
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        return None;
    }

    let direct = Path::new(trimmed);
    if direct.is_file() {
        return std::fs::canonicalize(direct).ok().or_else(|| Some(direct.to_path_buf()));
    }
    if direct.components().count() > 1 {
        // A path was given and it is not there. Searching PATH for its file
        // name would silently substitute a different binary.
        return None;
    }

    search_path(trimmed)
}

fn search_path(name: &str) -> Option<PathBuf> {
    let candidates: Vec<String> = if cfg!(windows) && !name.to_ascii_lowercase().ends_with(".exe") {
        vec![format!("{}.exe", name), name.to_string()]
    } else {
        vec![name.to_string()]
    };

    for dir in std::env::split_paths(&std::env::var_os("PATH")?) {
        for candidate in &candidates {
            let full = dir.join(candidate);
            if full.is_file() {
                return std::fs::canonicalize(&full).ok().or(Some(full));
            }
        }
    }
    None
}

/// The `Path=` value from an `ffmpeg.ini`, if there is one.
///
/// Deliberately lenient about the section header and whitespace and strict
/// about nothing: this is reading somebody else's file to report on it, so a
/// shape we do not recognise should read as "no answer" rather than an error.
fn parse_ini_path(body: &str) -> Option<PathBuf> {
    for line in body.lines() {
        let line = line.trim();
        if line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("path") {
            let value = value.trim().trim_matches('"');
            if !value.is_empty() {
                return Some(PathBuf::from(value));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dod_hlae_ffmpeg_{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    /// An HLAE install: the exe, and the `ffmpeg` folder its readme lives in.
    fn install(name: &str) -> PathBuf {
        let root = scratch(name);
        std::fs::create_dir_all(root.join(FFMPEG_DIR)).expect("ffmpeg dir");
        let exe = root.join("HLAE.exe");
        std::fs::write(&exe, b"").expect("exe");
        exe
    }

    fn a_real_ffmpeg(dir: &Path) -> PathBuf {
        let exe = dir.join("ffmpeg.exe");
        std::fs::write(&exe, b"").expect("ffmpeg");
        exe
    }

    #[test]
    fn the_working_install_shape_is_reported_as_missing() {
        // What is actually on this machine: an ffmpeg folder holding nothing
        // but the readme. It looks set up and is not.
        let hlae = install("missing");
        std::fs::write(
            hlae.parent().unwrap().join(FFMPEG_DIR).join("readme.advancedfx.txt"),
            b"install ffmpeg here",
        )
        .expect("readme");

        let state = detect(&hlae);
        assert!(matches!(state, HlaeFfmpeg::Missing { .. }), "{:?}", state);
        assert!(!state.is_usable());
        assert!(state.can_link(), "this is exactly the case worth offering to fix");
    }

    #[test]
    fn a_bundled_binary_needs_nothing_and_is_never_offered_an_ini() {
        let hlae = install("bundled");
        let bin = hlae.parent().unwrap().join(FFMPEG_DIR).join("bin");
        std::fs::create_dir_all(&bin).expect("bin");
        std::fs::write(bin.join("ffmpeg.exe"), b"").expect("exe");

        let state = detect(&hlae);
        assert!(matches!(state, HlaeFfmpeg::Bundled { .. }), "{:?}", state);
        assert!(state.is_usable());
        assert!(!state.can_link());
    }

    #[test]
    fn a_bundled_binary_wins_over_an_ini() {
        // HLAE finds the binary without consulting the ini, so reporting the
        // ini would describe something that has no effect on anything.
        let hlae = install("both");
        let folder = hlae.parent().unwrap().join(FFMPEG_DIR);
        let bin = folder.join("bin");
        std::fs::create_dir_all(&bin).expect("bin");
        std::fs::write(bin.join("ffmpeg.exe"), b"").expect("exe");
        std::fs::write(folder.join(INI_NAME), "[Ffmpeg]\nPath=C:\\elsewhere\\ffmpeg.exe\n")
            .expect("ini");

        assert!(matches!(detect(&hlae), HlaeFfmpeg::Bundled { .. }));
    }

    #[test]
    fn a_stale_ini_reads_as_linked_but_not_usable() {
        // The case a "configured / not configured" boolean would hide: HLAE has
        // an answer, and the answer is wrong.
        let hlae = install("stale");
        let folder = hlae.parent().unwrap().join(FFMPEG_DIR);
        std::fs::write(folder.join(INI_NAME), "[Ffmpeg]\nPath=C:\\gone\\ffmpeg.exe\n")
            .expect("ini");

        match detect(&hlae) {
            HlaeFfmpeg::Linked { target_exists, .. } => assert!(!target_exists),
            other => panic!("{:?}", other),
        }
        assert!(!detect(&hlae).is_usable());
        assert!(
            !detect(&hlae).can_link(),
            "an existing ini is reported, never replaced"
        );
    }

    #[test]
    fn linking_writes_an_ini_hlae_can_read_back() {
        let hlae = install("link");
        let ffmpeg = a_real_ffmpeg(hlae.parent().unwrap());

        let ini = link(&hlae, &ffmpeg).expect("link");
        assert!(ini.is_file());

        match detect(&hlae) {
            HlaeFfmpeg::Linked {
                target,
                target_exists,
                ..
            } => {
                assert!(target_exists);
                assert_eq!(target, ffmpeg);
            }
            other => panic!("{:?}", other),
        }
        assert!(detect(&hlae).is_usable());
    }

    #[test]
    fn an_existing_ini_is_refused_not_overwritten() {
        // HLAE is shared with Source work. Repointing it silently would break
        // somebody else's workflow to fix ours.
        let hlae = install("refuse");
        let folder = hlae.parent().unwrap().join(FFMPEG_DIR);
        let original = "[Ffmpeg]\nPath=D:\\someone-elses\\ffmpeg.exe\n";
        std::fs::write(folder.join(INI_NAME), original).expect("ini");
        let ffmpeg = a_real_ffmpeg(hlae.parent().unwrap());

        let err = link(&hlae, &ffmpeg).expect_err("must refuse");
        assert!(matches!(err, LinkError::AlreadyLinked { .. }), "{:?}", err);
        assert_eq!(
            std::fs::read_to_string(folder.join(INI_NAME)).unwrap(),
            original,
            "the existing file was modified"
        );
    }

    #[test]
    fn linking_refuses_an_ffmpeg_that_is_not_there() {
        let hlae = install("no_ffmpeg");
        let err = link(&hlae, Path::new("C:/nowhere/ffmpeg.exe")).expect_err("must refuse");
        assert!(matches!(err, LinkError::NoSuchFfmpeg { .. }), "{:?}", err);
    }

    #[test]
    fn a_path_whose_folder_exists_but_whose_exe_does_not_is_no_install() {
        // `hlae_path` names HLAE.exe, not the folder — the same convention
        // `game_path` follows, and the same one-level-too-high trap. Checking
        // only the parent would accept a typo'd exe name, or the folder typed
        // in place of the exe, and then offer to write an ini into somewhere
        // that is not an HLAE install.
        let root = scratch("folder_given");
        assert_eq!(detect(&root.join("HLAE.exe")), HlaeFfmpeg::NoInstall);
        assert_eq!(detect(&root), HlaeFfmpeg::NoInstall, "the folder itself");
        assert!(!detect(&root).can_link());
    }

    #[test]
    fn an_ini_we_cannot_make_sense_of_reads_as_no_answer() {
        for body in ["", "; just a comment\n", "[Ffmpeg]\n", "nonsense\n", "[Ffmpeg]\nPath=\n"] {
            assert_eq!(parse_ini_path(body), None, "parsed {:?}", body);
        }
    }

    #[test]
    fn the_ini_format_is_read_the_way_the_readme_writes_it() {
        assert_eq!(
            parse_ini_path("[Ffmpeg]\nPath=C:\\Users\\x\\ffmpeg\\bin\\ffmpeg.exe\n"),
            Some(PathBuf::from("C:\\Users\\x\\ffmpeg\\bin\\ffmpeg.exe"))
        );
        // Tolerating what a human might reasonably have typed instead.
        assert_eq!(
            parse_ini_path("[Ffmpeg]\n  path = \"C:\\a\\ffmpeg.exe\"  \n"),
            Some(PathBuf::from("C:\\a\\ffmpeg.exe"))
        );
    }

    #[test]
    fn a_configured_path_that_exists_resolves_to_itself() {
        let dir = scratch("resolve");
        let ffmpeg = a_real_ffmpeg(&dir);
        let got = resolve_absolute(&ffmpeg.to_string_lossy()).expect("resolve");
        assert!(got.is_file());
    }

    #[test]
    fn a_configured_path_that_is_wrong_is_not_quietly_swapped_for_another() {
        // Falling back to a PATH search here would hand HLAE a different binary
        // than the one Render Studio was told to use.
        assert_eq!(resolve_absolute("C:/nowhere/at/all/ffmpeg.exe"), None);
        assert_eq!(resolve_absolute("   "), None);
    }
}
