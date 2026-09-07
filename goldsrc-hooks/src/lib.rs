//! Standalone companion DLL for DoD 1.3 GoldSrc capture sessions, injected
//! into `hl.exe` alongside (not instead of) HLAE's own `AfxHookGoldSrc.dll`.
//!
//! Unlike patching HLAE's own source, this doesn't need HLAE's build
//! toolchain, and doesn't touch any function HLAE itself already hooks (see
//! `engine.rs` for exactly which functions this DLL depends on). It's built
//! as a separate workspace crate specifically so it doesn't have to live
//! inside a forked copy of HLAE's own repository -- though `addresses.rs`'s
//! byte-signature scan for `pEngfuncs` is a direct, faithful port of
//! HLAE's own `hl_addresses.cpp` technique, since live testing proved the
//! more "obvious" approach (hooking `client.dll`'s exported `Initialize`)
//! doesn't actually work on at least some engine builds -- see that
//! module's docs.
//!
//! Implements two fixes, each independently toggled and each safe to inject
//! without the other:
//! - `sound_fix`: force full-volume weapon-fire audio while spectating.
//! - `anim_fix`: correct MG42/MG34/BAR/Bren viewmodel deploy animations
//!   while spectating in-eye.
//!
//! See each module's docs for the full R&D reasoning.
//!
//! Each fix has a default set via an environment variable on the `hl.exe`
//! process before launch (`GOLDSRC_HOOKS_FORCE_WEAPON_VOLUME=1`,
//! `GOLDSRC_HOOKS_ANIM_FIX=1`), and can also be toggled live from the game
//! console via `dodtools_hltv_gunshots_fix <0|1>` / `dodtools_hltv_animation_fix
//! <0|1>` (see `commands.rs`) -- either mechanism flips the same runtime
//! flag, so whichever is more convenient for a given session works.

mod addresses;
mod anim_fix;
mod binscan;
mod commands;
mod debug;
mod engine;
mod pe;
mod sound_fix;

use std::sync::atomic::Ordering;
use windows_sys::Win32::Foundation::{BOOL, HINSTANCE, TRUE};
use windows_sys::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows_sys::Win32::System::Threading::{CreateThread, Sleep};

fn env_flag_set(name: &str) -> bool {
    std::env::var(name).map(|v| v == "1").unwrap_or(false)
}

unsafe extern "system" fn worker_thread(_lp_param: *mut std::ffi::c_void) -> u32 {
    sound_fix::ENABLED.store(env_flag_set("GOLDSRC_HOOKS_FORCE_WEAPON_VOLUME"), Ordering::Relaxed);
    anim_fix::ENABLED.store(env_flag_set("GOLDSRC_HOOKS_ANIM_FIX"), Ordering::Relaxed);

    unsafe { debug::new_session_separator() };
    unsafe { debug::report("goldsrc-hooks worker thread started") };

    // Hooks hw.dll's LoadLibraryA import to detect the moment client.dll
    // loads, then signature-scans it for pEngfuncs/pstudio -- see engine.rs
    // and addresses.rs's module docs for the full mechanism and why it
    // doesn't hook client.dll's own Initialize export.
    engine::install();

    // The scan runs synchronously the instant client.dll is detected loading
    // (it reads static bytes already in the file image, no engine call
    // needs to happen first), so this is normally near-instant -- the wait
    // loop is just a safety margin in case client.dll takes a while to load
    // at all.
    let mut waited = 0u32;
    while engine::engfuncs().is_none() {
        if waited >= 30_000 {
            unsafe { debug::report("goldsrc-hooks: timed out waiting for client.dll to load and be scanned; fixes not installed this session") };
            return 0;
        }
        unsafe { Sleep(50) };
        waited += 50;
    }

    unsafe { debug::report("goldsrc-hooks: engfuncs captured, installing sound_fix") };
    sound_fix::install();

    // anim_fix additionally needs engine_studio (captured via
    // HUD_GetStudioModelInterface, a separate one-time export hook -- see
    // engine.rs); install() itself only registers the per-frame poll, which
    // checks for that availability on every call, so it's safe to install
    // even if that capture hasn't landed yet.
    anim_fix::install();

    // In-game mirv_dod_hltv_gunshots_fix / mirv_dod_hltv_animation_fix
    // console commands -- toggle the same ENABLED flags the env vars above
    // set as the initial default, so either mechanism works.
    commands::install();

    0
}

/// # Safety
///
/// Only ever called by the Windows loader itself, per the standard `DllMain`
/// contract -- never call this directly.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllMain(_hinst: HINSTANCE, reason: u32, _reserved: *mut std::ffi::c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        // Do as little as possible directly in DllMain (loader-lock rules --
        // no LoadLibrary, no waiting, ideally no allocation). Hand off to a
        // worker thread immediately instead.
        unsafe {
            CreateThread(std::ptr::null(), 0, Some(worker_thread), std::ptr::null(), 0, std::ptr::null_mut());
        }
    }
    TRUE
}
