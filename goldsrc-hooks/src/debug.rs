//! Best-effort, non-blocking diagnostics. Deliberately never pops a message
//! box or otherwise blocks -- this DLL runs inside an automated, headless
//! capture pipeline, and a blocking dialog would hang the whole batch.

use std::io::Write;

use windows_sys::Win32::System::SystemInformation::GetLocalTime;

/// Local wall-clock time as `HH:MM:SS.mmm`.
///
/// Wall clock rather than time-since-load: the point of the timestamps is to
/// line log lines up against something that happened in the game while
/// watching it, and milliseconds are kept because most of what this logs
/// changes at frame rate, where second resolution would collapse a burst of
/// distinct events into one indistinguishable clump.
fn timestamp() -> String {
    // Safety: fills a plain struct we own; cannot fail.
    let mut now = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut now) };
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        now.wHour, now.wMinute, now.wSecond, now.wMilliseconds
    )
}

/// Appends a line to `%TEMP%\goldsrc_hooks.log`. Failures are swallowed --
/// logging must never be the thing that destabilizes the host process.
pub unsafe fn report(message: &str) {
    let Some(mut path) = std::env::var_os("TEMP").map(std::path::PathBuf::from) else {
        return;
    };
    path.push("goldsrc_hooks.log");

    // Demo time alongside wall clock. Wall clock cannot be matched against
    // something seen on screen once playback is paused, seeked or
    // fast-forwarded; the demo clock can. Omitted before the first frame,
    // when there is no playback to be at a position in.
    let demo = match crate::engine::client_time() {
        t if t > 0.0 => format!(" [demo {t:9.3}]"),
        _ => String::new(),
    };

    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "[{}]{demo} [goldsrc-hooks] {message}", timestamp());
    }
}

/// Writes a visual break before a new session's first line, so the log file
/// can be left in place across runs instead of deleted each time -- just
/// copy from the last separator down.
pub unsafe fn new_session_separator() {
    // The date goes here rather than on every line: the log spans days, but a
    // session does not, so it only needs stating once per session.
    let mut now = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut now) };
    unsafe {
        report(&format!(
            "========== new session, {:04}-{:02}-{:02} ==========",
            now.wYear, now.wMonth, now.wDay
        ))
    };
}
