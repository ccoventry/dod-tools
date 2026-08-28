//! One batch's worth of OBS driving.
//!
//! Owns the connection, turns console-log markers into `StartRecord` /
//! `StopRecord`, and folds each finished file into the take-folder layout the
//! rest of the pipeline already understands.
//!
//! **Why the output is moved rather than left where OBS put it.** Render
//! Studio's scanner, the shared `is_renderable_take` predicate, `take_key`
//! matching between the capture and render views, and the export-pool routing
//! all key off `<take_folder>/take0000/<stream>/`. Writing into that shape
//! means none of them need a second artefact layout taught to them — the same
//! trick `docs/direct_to_video_capture.md` used for `mirv_movie_ffmpeg`.
//!
//! The move is a rename, not a copy: `SetRecordDirectory` points OBS at the
//! destination folder *before* recording, so the file is already on the right
//! drive and only its name changes. That is also what preserves the per-block
//! export-pool routing across drives.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::log_markdown;
use crate::patch::ObsConfig;

use super::client::{ObsClient, ObsError, ObsPreflight};
use super::log_tail::{Marker, MarkerKind};
use super::{STOP_MARKER, TRIGGER_MARKER};

/// Stream folder a single-stream OBS take lands in.
///
/// `all` is HLAE's name for the composited stream, and the scanner, the
/// renderer's `clip.img_folder` and `ClipData` all already expect it. Separate
/// HUD is out of scope on this path, so there is only ever this one.
pub(super) const STREAM_FOLDER: &str = "all";

/// Take folder HLAE would have auto-numbered. Reproduced rather than invented:
/// `shared::paths::take_key` matches across this level, and the scanner skips a
/// trailing `take*` component, so writing anywhere else would break both.
pub(super) const TAKE_FOLDER: &str = "take0000";

/// What happened to one block.
#[derive(Debug, Clone)]
pub struct RecordedBlock {
    pub take_folder: PathBuf,
    /// Where the video ended up, once folded into the take folder.
    pub video: PathBuf,
    /// Wall-clock seconds OBS was recording. Compared against the block's
    /// expected duration by take verification — a check the frame-sequence
    /// path has no equivalent of, because there the duration is not known
    /// ahead of time.
    pub seconds: f64,
}

pub struct ObsSession {
    /// Shared with the cleanup guard so a cancel or crash can still stop a
    /// recording. `None` once the session has been shut down.
    client: Arc<Mutex<Option<ObsClient>>>,
    /// Kept for reconnection. OBS being closed or crashing mid-batch is a
    /// recoverable event — it comes back with the same address and password.
    cfg: ObsConfig,
    /// Set once the connection is gone and could not be re-established. The
    /// batch aborts on this rather than running to completion recording
    /// nothing, which is what it did before: every later block would fail its
    /// `SetRecordDirectory`, add a line to `skipped`, and the run would end
    /// looking successful with an empty take for every block.
    dead: bool,
    /// Block take folders in the order the batch will record them — the same
    /// flattened order the capture manifest uses.
    take_folders: Vec<PathBuf>,
    next_block: usize,
    /// Destination folder of the recording in progress, if any.
    active: Option<PathBuf>,
    active_since: Option<std::time::Instant>,
    /// Scene that was active before the batch switched away, to restore.
    previous_scene: Option<String>,
    pub recorded: Vec<RecordedBlock>,
    pub skipped: Vec<String>,
}

impl ObsSession {
    /// Connects, checks the install, and switches to the configured scene.
    ///
    /// Fails loudly rather than degrading: every failure here otherwise
    /// produces a batch that runs to completion and captures nothing, which is
    /// the single worst outcome this path can have.
    pub fn start(
        cfg: &ObsConfig,
        take_folders: Vec<PathBuf>,
        game_width: i32,
        game_height: i32,
    ) -> Result<(Self, ObsPreflight), ObsError> {
        let mut client = ObsClient::connect(&cfg.address(), &cfg.password)?;
        let preflight = client.preflight(game_width, game_height)?;

        if !preflight.missing_requests.is_empty() {
            return Err(ObsError::Request {
                request: preflight.missing_requests.join(", "),
                detail: format!(
                    "this OBS ({}) does not expose the requests the capture needs",
                    preflight.obs_version
                ),
            });
        }
        if preflight.recording {
            return Err(ObsError::Request {
                request: "StartRecord".into(),
                detail: "OBS is already recording. Stop it before starting a batch.".into(),
            });
        }
        // Refuse rather than warn: driving somebody's recorder mid-stream is
        // not a decision to make on their behalf, and the failure would be
        // visible to their audience rather than to them.
        if preflight.streaming {
            return Err(ObsError::Request {
                request: "StartRecord".into(),
                detail: "OBS is streaming. dod-tools will not drive its recorder during a live \
                         stream."
                    .into(),
            });
        }

        for warning in &preflight.warnings {
            log_markdown(&format!("⚠️ **OBS** — {warning}"));
        }

        // Switching scene is a live-state change, not a settings edit: it is
        // reversible, destroys nothing, and is exactly what choosing a scene
        // means. The previous one is remembered so the batch puts it back.
        let previous_scene = if cfg.scene.is_empty() {
            None
        } else {
            let current = client.current_scene().ok();
            if current.as_deref() != Some(cfg.scene.as_str()) {
                client.set_scene(&cfg.scene)?;
                log_markdown(&format!(
                    "🎬 **OBS** — switched to scene `{}` for this batch (restored afterwards).",
                    cfg.scene
                ));
            }
            current
        };

        Ok((
            Self {
                client: Arc::new(Mutex::new(Some(client))),
                cfg: cfg.clone(),
                dead: false,
                take_folders,
                next_block: 0,
                active: None,
                active_since: None,
                previous_scene,
                recorded: Vec::new(),
                skipped: Vec::new(),
            },
            preflight,
        ))
    }

    /// Handle shared with `CaptureCleanupGuard` so a cancel, a crashed game or
    /// a finished batch can stop a recording.
    ///
    /// Not a panic, despite what a `Drop` normally buys: release builds set
    /// `panic = "abort"`, so nothing unwinds and no destructor runs. That gap,
    /// together with a hard kill and a power cut, is what `obs::recover`
    /// exists to clean up on the next start.
    pub fn stop_handle(&self) -> Arc<Mutex<Option<ObsClient>>> {
        Arc::clone(&self.client)
    }

    /// Feeds one console-log marker in.
    ///
    /// Markers that are not stage boundaries are ignored, so breadcrumbs and
    /// custom-command echoes cost nothing.
    pub fn on_marker(&mut self, marker: &Marker) {
        match marker.kind {
            k if k == TRIGGER_MARKER => self.begin_block(),
            k if k == STOP_MARKER => self.end_block(),
            MarkerKind::BatchComplete => self.end_block(),
            _ => {}
        }
    }

    fn begin_block(&mut self) {
        if self.active.is_some() {
            // A second trigger with a recording already running means a
            // STOP_RECORD was missed. Closing the current one first is the only
            // way the next block gets its own file rather than being appended
            // to its predecessor.
            log_markdown(
                "⚠️ **OBS** — a new block started while still recording; closing the previous \
                 take first. A `STOP_RECORD` marker was missed.",
            );
            self.end_block();
        }
        let Some(take_folder) = self.take_folders.get(self.next_block).cloned() else {
            self.skipped
                .push(format!("block {} has no planned take folder", self.next_block));
            return;
        };
        self.next_block += 1;

        let dest = take_folder.join(TAKE_FOLDER).join(STREAM_FOLDER);
        if let Err(e) = std::fs::create_dir_all(&dest) {
            self.skipped
                .push(format!("could not create {}: {e}", dest.display()));
            return;
        }

        // One reconnect and one retry before giving up: OBS being closed or
        // crashing between blocks is survivable, and a batch has minutes of
        // fast-forward in it during which it can plausibly come back.
        if let Err(e) = self.try_begin(&dest) {
            let recovered = e.is_transport() && self.reconnect() && self.try_begin(&dest).is_ok();
            if !recovered {
                self.note_failure(format!("OBS failed to start recording: {e}"), &e);
                return;
            }
        }

        self.active = Some(dest);
        self.active_since = Some(std::time::Instant::now());
    }

    // Split out so `begin_block` can run it twice around a reconnect without
    // holding the client lock across the retry.
    fn try_begin(&self, dest: &Path) -> Result<(), ObsError> {
        let mut guard = self.client.lock().unwrap_or_else(|p| p.into_inner());
        let client = guard
            .as_mut()
            .ok_or_else(|| ObsError::Transport("not connected to OBS".into()))?;
        // Pointing OBS at the destination before recording is what makes the
        // later move a rename rather than a cross-drive copy, and it is what
        // keeps blocks routed across the export pool.
        client.set_record_directory(&dest.to_string_lossy())?;
        client.start_record()?;
        Ok(())
    }

    fn end_block(&mut self) {
        let Some(dest) = self.active.take() else {
            return;
        };
        let seconds = self
            .active_since
            .take()
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0);

        let path = match self.try_stop() {
            Ok(p) => p,
            Err(e) => {
                // The recording is gone but its bytes may not be: OBS writes
                // into `dest` from the moment it starts, so whatever is in
                // there is this block's. Salvaging it beats leaving a file the
                // scanner cannot see, which is what the timestamped name means
                // — `stream_video_path` matches `video.<ext>` and nothing else.
                match salvage_orphan(&dest) {
                    Some(video) => {
                        log_markdown(&format!(
                            "⚠️ **OBS** — lost the connection mid-block ({e}). Salvaged \
                             `{}`, which may be unfinalised if OBS crashed rather than closed \
                             — MKV survives that, MP4 usually does not.",
                            video.display()
                        ));
                        let take_folder = take_folder_of(&dest);
                        self.recorded.push(RecordedBlock { take_folder, video, seconds });
                    }
                    None => self
                        .skipped
                        .push(format!("OBS failed to stop recording: {e}")),
                }
                if e.is_transport() {
                    self.dead = true;
                }
                return;
            }
        };

        match fold_into_take(Path::new(&path), &dest) {
            Ok(video) => {
                let take_folder = take_folder_of(&dest);
                self.recorded.push(RecordedBlock { take_folder, video, seconds });
            }
            Err(e) => self.skipped.push(e),
        }
    }

    fn try_stop(&self) -> Result<String, ObsError> {
        let mut guard = self.client.lock().unwrap_or_else(|p| p.into_inner());
        let client = guard
            .as_mut()
            .ok_or_else(|| ObsError::Transport("not connected to OBS".into()))?;
        client.stop_record()
    }

    /// True once the connection is gone and could not be re-established.
    ///
    /// The engine polls this and aborts the batch. Continuing would mean
    /// playing the demo to the end capturing nothing, then reporting success.
    pub fn is_dead(&self) -> bool {
        self.dead
    }

    fn note_failure(&mut self, message: String, e: &ObsError) {
        self.skipped.push(message);
        if e.is_transport() {
            self.dead = true;
        }
    }

    fn reconnect(&mut self) -> bool {
        let fresh = ObsClient::connect(&self.cfg.address(), &self.cfg.password);
        let ok = {
            let mut guard = self.client.lock().unwrap_or_else(|p| p.into_inner());
            match fresh {
                Ok(client) => {
                    *guard = Some(client);
                    true
                }
                Err(_) => {
                    // Drop the dead socket so the cleanup guard does not try to
                    // send `StopRecord` down it on the way out.
                    *guard = None;
                    false
                }
            }
        };
        if ok {
            log_markdown("🔌 **OBS** — the connection dropped and was re-established.");
        } else {
            self.dead = true;
        }
        ok
    }

    /// Stops anything still recording and restores the previous scene.
    pub fn finish(&mut self) {
        self.end_block();
        let mut guard = self.client.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(client) = guard.as_mut() {
            if let Some(scene) = self.previous_scene.take() {
                if !scene.is_empty() {
                    let _ = client.set_scene(&scene);
                }
            }
        }
        *guard = None;
    }
}

/// The take folder two levels above a `<take>/take0000/all` stream folder.
fn take_folder_of(dest: &Path) -> PathBuf {
    dest.parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dest.to_path_buf())
}

/// Containers OBS can be configured to write. Kept in step with the scanner's
/// `VIDEO_EXTENSIONS` — this list may be wider, never narrower, because a file
/// salvaged under a name the scanner cannot resolve helps nobody.
const ORPHAN_VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "mov", "avi"];

/// Folds whatever video OBS left in a block's own folder into `video.<ext>`.
///
/// Only ever called against a folder `SetRecordDirectory` pointed at for this
/// block, so anything video-shaped inside it is this block's output — there is
/// no ambiguity to resolve. Used when the connection dies before `StopRecord`
/// can report the filename.
fn salvage_orphan(dest: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dest).ok()?;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension()?.to_string_lossy().to_lowercase();
        if !ORPHAN_VIDEO_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }
        if path.file_stem().is_some_and(|s| s == "video") {
            // Already folded — a previous block's output, or a retry.
            continue;
        }
        let modified = entry.metadata().ok()?.modified().ok()?;
        if newest.as_ref().is_none_or(|(t, _)| modified > *t) {
            newest = Some((modified, path));
        }
    }
    let (_, path) = newest?;
    fold_into_take(&path, dest).ok()
}

/// Renames OBS's output into the stream folder as `video.<ext>`.
///
/// The extension is kept from whatever OBS wrote rather than forced: the
/// container is the user's setting, the scanner matches `video.*` by
/// extension, and renaming an MP4 to `.avi` would produce a file that lies
/// about itself to every tool downstream.
pub(super) fn fold_into_take(recorded: &Path, dest: &Path) -> Result<PathBuf, String> {
    if !recorded.is_file() {
        return Err(format!(
            "OBS reported {} but no file is there",
            recorded.display()
        ));
    }
    let ext = recorded
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| "mp4".to_string());
    let target = dest.join(format!("video.{ext}"));
    if target == recorded {
        return Ok(target);
    }
    std::fs::rename(recorded, &target).map_err(|e| {
        format!(
            "could not move {} to {}: {e}",
            recorded.display(),
            target.display()
        )
    })?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("dod_obs_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// The layout is load-bearing: the scanner, `take_key` and the renderer all
    /// key off `<take>/take0000/<stream>/video.*`.
    #[test]
    fn folds_a_recording_into_the_stream_folder() {
        let root = scratch("fold");
        let dest = root.join("chain_01_b0").join(TAKE_FOLDER).join(STREAM_FOLDER);
        std::fs::create_dir_all(&dest).unwrap();
        let recorded = dest.join("2026-08-28 03-26-01.mp4");
        std::fs::write(&recorded, b"x").unwrap();

        let out = fold_into_take(&recorded, &dest).unwrap();
        assert_eq!(out.file_name().unwrap(), "video.mp4");
        assert!(out.is_file());
        assert!(!recorded.exists(), "the original name should be gone");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The container is the user's setting. Renaming an MKV to `.avi` would
    /// make it lie to every tool that reads it afterwards.
    #[test]
    fn keeps_the_container_extension_obs_actually_wrote() {
        let root = scratch("ext");
        let dest = root.join("b0").join(TAKE_FOLDER).join(STREAM_FOLDER);
        std::fs::create_dir_all(&dest).unwrap();
        for (name, want) in [("a.mkv", "video.mkv"), ("b.AVI", "video.avi")] {
            let rec = dest.join(name);
            std::fs::write(&rec, b"x").unwrap();
            let out = fold_into_take(&rec, &dest).unwrap();
            assert_eq!(out.file_name().unwrap(), want);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// OBS reporting a path that is not there is exactly the "capture ran and
    /// produced nothing" case this path has to fail loudly on.
    #[test]
    fn a_missing_recording_is_an_error_not_a_silent_pass() {
        let root = scratch("missing");
        let err = fold_into_take(&root.join("nope.mp4"), &root).unwrap_err();
        assert!(err.contains("no file is there"), "got: {err}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
