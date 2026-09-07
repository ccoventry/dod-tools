//! Standalone companion DLL for DoD 1.3 GoldSrc capture sessions, injected
//! into `hl.exe` alongside (not instead of) HLAE's own `AfxHookGoldSrc.dll`.
//!
//! Unlike patching HLAE's own source, this doesn't need HLAE's build
//! toolchain, and doesn't touch any function HLAE itself already hooks (see
//! `engine.rs` for exactly which functions this DLL depends on and why
//! there's no HLAE-internal signature-scanning involved). It's built as a
//! separate workspace crate specifically so it doesn't have to live inside
//! a forked copy of HLAE's own repository.
//!
//! Implements two fixes, each independently toggled and each safe to inject
//! without the other:
//! - `sound_fix`: force full-volume weapon-fire audio while spectating.
//! - `anim_fix`: correct MG42/MG34/BAR/Bren viewmodel deploy animations
//!   while spectating in-eye.
//!
//! See each module's docs for the full R&D reasoning.
//!
//! Toggled via environment variables set on the `hl.exe` process before
//! launch (dod-tools already controls that launch, so this is simpler than
//! wiring up our own console-command parser):
//! `GOLDSRC_HOOKS_FORCE_WEAPON_VOLUME=1`, `GOLDSRC_HOOKS_ANIM_FIX=1`.

mod anim_fix;
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

    unsafe { debug::report("goldsrc-hooks worker thread started") };

    // Hooks client.dll's Initialize export (via hw.dll's LoadLibraryA IAT --
    // see engine.rs's module docs for the full chain and why this needs no
    // DoD-specific byte pattern).
    engine::install();

    // engine::install() only arranges for `hook_initialize` to fire the next
    // time client.dll's real Initialize runs; wait for that to actually
    // happen (map load can take a few seconds) before wiring up fixes that
    // need the captured engfuncs table.
    let mut waited = 0u32;
    while engine::engfuncs().is_none() {
        if waited >= 30_000 {
            unsafe { debug::report("goldsrc-hooks: timed out waiting for client.dll's Initialize to fire; fixes not installed this session") };
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
