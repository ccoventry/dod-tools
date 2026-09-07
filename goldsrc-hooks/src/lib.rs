//! Standalone companion DLL for DoD 1.3 GoldSrc capture sessions, injected
//! into `hl.exe` alongside (not instead of) HLAE's own `AfxHookGoldSrc.dll`.
//!
//! Unlike patching HLAE's own source, this doesn't need HLAE's build
//! toolchain, and doesn't touch any function HLAE itself already hooks (see
//! `engine.rs` for exactly which functions this DLL depends on). It's built
//! as a separate workspace crate specifically so it doesn't have to live
//! inside a forked copy of HLAE's own repository. How it gets a foothold in
//! `client.dll` is genuinely non-obvious -- patching that DLL's export table
//! does nothing on a "secured" build, because the engine resolves the whole
//! client interface through a single `F` export instead. See `engine.rs`'s
//! module docs and `docs/goldsrc_client_dll_internals.md`.
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

mod anim_fix;
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

    // Hooks hw.dll's LoadLibraryA and GetProcAddress imports, so we see
    // client.dll load and can substitute our own entry points as the engine
    // resolves it -- see engine.rs's module docs for the full mechanism and
    // why patching client.dll's export table does nothing.
    engine::install();

    // pEngfuncs arrives when the engine calls client.dll's Initialize, which
    // happens during normal startup shortly after client.dll loads.
    let mut waited = 0u32;
    while engine::engfuncs().is_none() {
        if waited >= 30_000 {
            unsafe { debug::report("goldsrc-hooks: timed out waiting for client.dll's Initialize to run; fixes not installed this session") };
            return 0;
        }
        unsafe { Sleep(50) };
        waited += 50;
    }

    unsafe { debug::report("goldsrc-hooks: engfuncs captured, installing sound_fix") };
    sound_fix::install();

    // anim_fix additionally needs engine_studio, captured when the engine
    // calls HUD_GetStudioModelInterface; install() itself only registers the
    // per-frame callback, which re-checks that availability on every call, so
    // it's safe to install even if that capture hasn't landed yet.
    anim_fix::install();

    // In-game dodtools_hltv_gunshots_fix / dodtools_hltv_animation_fix
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
