//! In-game console commands so the two fixes can be toggled live instead of
//! only via environment variables set before `hl.exe` launches:
//! `dodtools_hltv_gunshots_fix <0|1>` and `dodtools_hltv_animation_fix
//! <0|1>` -- own `dodtools_*` prefix rather than HLAE's `mirv_*`, since these
//! commands are ours, not HLAE's. Each command also reports its current
//! state when called with no argument.
//!
//! Registration must happen after `engine::engfuncs()` is captured (i.e.
//! after `client.dll`'s real `Initialize` has run), since `pfnAddCommand`
//! lives on that same table.

use std::ffi::{c_char, CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::engine;
use crate::{anim_fix, sound_fix};

const GUNSHOTS_FIX_NAME: &str = "dodtools_hltv_gunshots_fix";
const ANIMATION_FIX_NAME: &str = "dodtools_hltv_animation_fix";

fn console_print(text: &str) {
    let Some(engfuncs) = engine::engfuncs() else { return };
    let Ok(c_text) = CString::new(text) else { return };
    unsafe { (engfuncs.pfn_console_print)(c_text.as_ptr()) };
}

/// Reads argv(1) (if present) as "0"/"1" and stores it into `flag`, then
/// prints the resulting state -- shared by both commands below since they
/// only differ in which flag and name they report.
fn handle_toggle(name: &str, flag: &AtomicBool, status: fn() -> String) {
    let Some(engfuncs) = engine::engfuncs() else { return };

    // Cmd_Argc counts the command name itself, so a bare invocation is 1 and
    // an argument makes it 2. Bare is a query, not a no-op.
    let argc = unsafe { (engfuncs.cmd_argc)() };
    let mut assigned = false;
    if argc >= 2 {
        let arg1 = unsafe { (engfuncs.cmd_argv)(1) };
        if !arg1.is_null() {
            let value = unsafe { CStr::from_ptr(arg1 as *const c_char) }.to_string_lossy();
            match value.trim() {
                "0" => {
                    flag.store(false, Ordering::Relaxed);
                    assigned = true;
                }
                "1" => {
                    flag.store(true, Ordering::Relaxed);
                    assigned = true;
                }
                other => {
                    console_print(&format!("{name}: expected 0 or 1, got \"{other}\"\n"));
                    unsafe { crate::debug::report(&format!("commands: {name} rejected argument \"{other}\"")) };
                    return;
                }
            }
        }
    }

    let state = if flag.load(Ordering::Relaxed) { "1 (on)" } else { "0 (off)" };
    if assigned {
        console_print(&format!("{name} = {state}\n"));
    } else {
        // Selecting the name from the console's type-ahead and pressing enter
        // sends it bare, which reads as "the toggle didn't work" unless the
        // reply says otherwise. DoD's own commands answer this shape the same
        // way (`usage: spec_menu <0|1>`), so match that.
        //
        // A bare call is also the natural "is this thing doing anything?"
        // question, so answer that too rather than only echoing the flag --
        // the flag being on says nothing about whether the fix's preconditions
        // are being met in the current view.
        let report = status();
        console_print(&format!("{name} = {state}\nusage: {name} <0|1>\n{report}\n"));
        unsafe { crate::debug::report(&format!("commands: {name} status -- {report}")) };
    }
    // Also to the log, so it stays a complete record of what was actually
    // enabled during a capture -- the console scrollback doesn't survive the
    // session, and "was the fix even on for that take?" is the first question
    // worth answering when a capture looks unchanged. argc distinguishes a
    // query from an assignment that didn't take.
    unsafe {
        crate::debug::report(&format!(
            "commands: {name} = {state} ({}, argc={argc})",
            if assigned { "set" } else { "queried, unchanged" }
        ))
    };
}

unsafe extern "C" fn cmd_gunshots_fix() {
    handle_toggle(GUNSHOTS_FIX_NAME, &sound_fix::ENABLED, sound_fix::status);
}

unsafe extern "C" fn cmd_animation_fix() {
    handle_toggle(ANIMATION_FIX_NAME, &anim_fix::ENABLED, anim_fix::status);
}

/// Registers both console commands. Must be called after `engine::engfuncs()`
/// returns `Some`.
pub fn install() {
    let Some(engfuncs) = engine::engfuncs() else {
        unsafe { crate::debug::report("commands::install called before engfuncs were captured -- this is a bug in install ordering") };
        return;
    };

    let gunshots_name = CString::new(GUNSHOTS_FIX_NAME).unwrap();
    let animation_name = CString::new(ANIMATION_FIX_NAME).unwrap();
    unsafe {
        (engfuncs.pfn_add_command)(gunshots_name.as_ptr(), cmd_gunshots_fix);
        (engfuncs.pfn_add_command)(animation_name.as_ptr(), cmd_animation_fix);
    }
    // Leak intentionally: pfnAddCommand keeps this pointer for the life of
    // the engine session, same lifetime as the DLL itself.
    std::mem::forget(gunshots_name);
    std::mem::forget(animation_name);

    unsafe { crate::debug::report("commands: registered dodtools_hltv_gunshots_fix and dodtools_hltv_animation_fix") };
}
