//! Best-effort, non-blocking diagnostics. Deliberately never pops a message
//! box or otherwise blocks -- this DLL runs inside an automated, headless
//! capture pipeline, and a blocking dialog would hang the whole batch.

use std::io::Write;

/// Appends a line to `%TEMP%\goldsrc_hooks.log`. Failures are swallowed --
/// logging must never be the thing that destabilizes the host process.
pub unsafe fn report(message: &str) {
    let Some(mut path) = std::env::var_os("TEMP").map(std::path::PathBuf::from) else {
        return;
    };
    path.push("goldsrc_hooks.log");

    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "[goldsrc-hooks] {message}");
    }
}

/// Writes a visual break before a new session's first line, so the log file
/// can be left in place across runs instead of deleted each time -- just
/// copy from the last separator down.
pub unsafe fn new_session_separator() {
    unsafe { report("==================== new session ====================") };
}
