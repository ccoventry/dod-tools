// patch/cfg_scan.rs
// What the game's own config files set behind the pipeline's back.
//
// STRICTLY READ-ONLY. This module never writes, edits, moves or deletes a
// config file. The user's configs are theirs; the most this does is say what it
// found and let them decide.
//
// The problem it solves is concrete. The flush sizes its sweep to `r_decals`
// and derives its on-screen test from `mirv_fov`, and both were assumed to come
// from the app's own init commands. They do not have to. A `config.cfg` ending
// in `exec movie.cfg`, and a `movie.cfg` setting `mirv_fov 105`, means the
// engine renders at a FOV the pipeline never hears about — and the only symptom
// is a slightly-too-narrow cone deciding what a camera can see.
//
// Two things this deliberately does not treat as settings:
//
//   * `bind "F7" "r_decals 4000"` — the cvar is a payload, not an execution.
//     Nothing happens until a key is pressed.
//   * `alias foo "r_decals 0"` — same.
//
// Both appear in real DoD configs, and counting them would raise a warning
// about a value the engine never applied.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Cvars the pipeline reads or depends on. Anything here, set anywhere but the
/// app's init commands, is worth telling the user about.
pub const WATCHED_CVARS: &[&str] = &["r_decals", "mirv_fov", "default_fov"];

/// Configs the engine executes on its own at start-up. Everything else is only
/// reached by being `exec`'d from one of these.
const ENTRY_POINTS: &[&str] = &["valve.rc", "config.cfg", "autoexec.cfg", "userconfig.cfg"];

/// Bounds on following `exec` chains, so a config that execs itself — or a
/// folder of hundreds — cannot turn a scan into a hang.
const MAX_DEPTH: usize = 8;
const MAX_FILES: usize = 64;
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// Commands whose arguments are stored, not run.
const NOT_EXECUTED: &[&str] = &["bind", "unbind", "alias", "bindtoggle", "+bind"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CvarSetting {
    pub cvar: String,
    pub value: String,
    pub file: PathBuf,
    pub line: usize,
    /// Whether this file is reached from a config the engine runs by itself.
    /// A config nobody execs sets nothing.
    pub auto_executed: bool,
}

impl CvarSetting {
    pub fn file_name(&self) -> String {
        self.file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.file.to_string_lossy().to_string())
    }
}

#[derive(Debug, Clone, Default)]
pub struct CfgScan {
    /// Every watched cvar assignment found, in the order the engine would reach
    /// them — so the last entry for a cvar is the one that wins.
    pub settings: Vec<CvarSetting>,
    pub files_read: usize,
    /// Configs present in the folder that nothing execs. Reported separately:
    /// they set nothing today, but a `movie.cfg` is one `exec` away from doing
    /// so, and a user reading a warning deserves to know it is there.
    pub unreferenced: Vec<PathBuf>,
}

impl CfgScan {
    /// The value the engine would end up with for a cvar, considering only
    /// configs it actually executes. Last one wins, as the console does.
    pub fn effective(&self, cvar: &str) -> Option<&CvarSetting> {
        self.settings
            .iter()
            .filter(|s| s.auto_executed && s.cvar.eq_ignore_ascii_case(cvar))
            .next_back()
    }

    /// Every watched cvar that an executed config sets.
    pub fn effective_settings(&self) -> Vec<&CvarSetting> {
        WATCHED_CVARS
            .iter()
            .filter_map(|c| self.effective(c))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.settings.is_empty() && self.unreferenced.is_empty()
    }
}

/// Scan a mod folder's configs. Never writes anything.
///
/// `game_dir` is the folder holding the configs — for DoD, `<hl.exe dir>/dod`.
pub fn scan(game_dir: &Path) -> CfgScan {
    let mut out = CfgScan::default();
    if !game_dir.is_dir() {
        return out;
    }

    let mut visited: HashSet<PathBuf> = HashSet::new();
    for entry in ENTRY_POINTS {
        let path = game_dir.join(entry);
        if path.is_file() {
            read_config(&path, game_dir, 0, &mut visited, &mut out, true);
        }
    }

    // Configs sitting in the folder that nothing reached.
    if let Ok(dir) = std::fs::read_dir(game_dir) {
        let mut loose: Vec<PathBuf> = dir
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.is_file()
                    && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("cfg"))
                    && !visited.contains(&normalise(p))
            })
            .collect();
        loose.sort();
        out.unreferenced = loose;
    }

    out
}

/// `dirs::canonicalize` would resolve symlinks and fail on missing files; all
/// this needs is a stable key so an `exec` cycle is recognised.
fn normalise(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().to_lowercase().replace('\\', "/"))
}

fn read_config(
    path: &Path,
    game_dir: &Path,
    depth: usize,
    visited: &mut HashSet<PathBuf>,
    out: &mut CfgScan,
    auto_executed: bool,
) {
    if depth > MAX_DEPTH || out.files_read >= MAX_FILES {
        return;
    }
    if !visited.insert(normalise(path)) {
        return;
    }
    if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) > MAX_FILE_BYTES {
        return;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    out.files_read += 1;

    for (index, raw) in text.lines().enumerate() {
        for command in commands_in(raw) {
            let mut parts = command.split_whitespace();
            let Some(head) = parts.next() else { continue };
            let head_lower = head.to_lowercase();

            if NOT_EXECUTED.contains(&head_lower.as_str()) {
                continue;
            }

            if head_lower == "exec" {
                if let Some(target) = parts.next() {
                    let name = unquote(target);
                    // Config paths are relative to the mod folder, and a config
                    // is not a place to accept a path that climbs out of it.
                    if name.contains("..") || name.starts_with('/') || name.starts_with('\\') {
                        continue;
                    }
                    let next = game_dir.join(&name);
                    let next = if next.extension().is_none() {
                        next.with_extension("cfg")
                    } else {
                        next
                    };
                    if next.is_file() {
                        read_config(&next, game_dir, depth + 1, visited, out, auto_executed);
                    }
                }
                continue;
            }

            if let Some(cvar) = WATCHED_CVARS
                .iter()
                .find(|c| head_lower == c.to_lowercase())
            {
                let Some(value) = parts.next() else { continue };
                // `mirv_fov handleZoom enabled 1` is a sub-command, not an
                // assignment — a numeric first argument is what makes it one.
                let value = unquote(value);
                if value.parse::<f32>().is_err() {
                    continue;
                }
                out.settings.push(CvarSetting {
                    cvar: (*cvar).to_string(),
                    value,
                    file: path.to_path_buf(),
                    line: index + 1,
                    auto_executed,
                });
            }
        }
    }
}

/// Split one config line into the commands the engine would run: comments
/// stripped, `;` separating commands, and semicolons inside quotes left alone.
fn commands_in(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                current.push(c);
            }
            '/' if !in_quotes && chars.peek() == Some(&'/') => break,
            ';' if !in_quotes => {
                out.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
    }
    out.push(current);

    out.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn unquote(token: &str) -> String {
    token.trim().trim_matches('"').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dod_cfg_scan_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_exec_chain_is_followed_and_the_last_value_wins() {
        // The real shape: config.cfg ends with `exec movie.cfg`, and movie.cfg
        // is where the interesting values are.
        let dir = scratch("chain");
        std::fs::write(dir.join("config.cfg"), "r_decals \"300\"\nexec movie.cfg\n").unwrap();
        std::fs::write(dir.join("movie.cfg"), "r_decals\t\"0\"\nmirv_fov \"105\"\n").unwrap();

        let scan = scan(&dir);
        assert_eq!(scan.effective("r_decals").unwrap().value, "0");
        assert_eq!(scan.effective("r_decals").unwrap().file_name(), "movie.cfg");
        assert_eq!(scan.effective("mirv_fov").unwrap().value, "105");
    }

    #[test]
    fn a_bound_key_is_not_a_setting() {
        // `bind "F7" "r_decals 4000"` changes nothing until F7 is pressed.
        // Counting it would warn about a value the engine never applied.
        let dir = scratch("bind");
        std::fs::write(
            dir.join("config.cfg"),
            "bind \"F6\" \"r_decals 0; hud_deathnotice_time 5\"\nbind \"F7\" \"r_decals 4000\"\nalias clean \"r_decals 1\"\n",
        )
        .unwrap();

        let scan = scan(&dir);
        assert!(scan.effective("r_decals").is_none(), "{:?}", scan.settings);
    }

    #[test]
    fn a_comment_is_not_a_setting_and_a_semicolon_separates_commands() {
        let dir = scratch("comment");
        std::fs::write(
            dir.join("config.cfg"),
            "// r_decals 999\nmirv_fov 90; r_decals 128\n",
        )
        .unwrap();

        let scan = scan(&dir);
        assert_eq!(scan.effective("mirv_fov").unwrap().value, "90");
        assert_eq!(scan.effective("r_decals").unwrap().value, "128");
    }

    #[test]
    fn a_subcommand_is_not_an_assignment() {
        // `mirv_fov handleZoom enabled 1` appears in real movie configs.
        let dir = scratch("subcommand");
        std::fs::write(dir.join("config.cfg"), "mirv_fov handleZoom enabled \"1\"\n").unwrap();

        assert!(scan(&dir).effective("mirv_fov").is_none());
    }

    #[test]
    fn a_config_nothing_execs_sets_nothing_but_is_still_reported() {
        // A movie.cfg no one runs is not a warning about the current capture —
        // but it is one `exec` away from being one.
        let dir = scratch("unreferenced");
        std::fs::write(dir.join("config.cfg"), "cl_showfps 1\n").unwrap();
        std::fs::write(dir.join("movie.cfg"), "mirv_fov \"105\"\n").unwrap();

        let scan = scan(&dir);
        assert!(scan.effective("mirv_fov").is_none());
        assert_eq!(scan.unreferenced.len(), 1);
        assert_eq!(scan.unreferenced[0].file_name().unwrap(), "movie.cfg");
    }

    #[test]
    fn a_config_that_execs_itself_terminates() {
        let dir = scratch("cycle");
        std::fs::write(dir.join("config.cfg"), "exec loop.cfg\n").unwrap();
        std::fs::write(dir.join("loop.cfg"), "exec config.cfg\nr_decals 8\n").unwrap();

        assert_eq!(scan(&dir).effective("r_decals").unwrap().value, "8");
    }

    #[test]
    fn an_exec_cannot_climb_out_of_the_mod_folder() {
        let dir = scratch("escape");
        std::fs::write(dir.join("config.cfg"), "exec ../../../windows/win.ini\n").unwrap();

        // Nothing to assert about the outcome beyond it not being read: the
        // point is that the traversal is refused rather than attempted.
        assert_eq!(scan(&dir).files_read, 1);
    }
}
