//! Forces DoD 1.3 weapon-fire sounds to play at full volume with no distance
//! attenuation while spectating (HLTV or a regular demo's in-eye/free
//! camera). See the R&D write-up: the fire *events* are almost always
//! present in a demo -- what makes gunfire sound "missing" is normal
//! distance-based volume falloff relative to wherever the camera happens to
//! be, which is exactly what a director-camera / movie-capture use case
//! doesn't want.
//!
//! Every DoD 1.3 weapon's firing sample is named `<weapon>_shoot.wav`
//! (confirmed against the game's own installed sound files -- garand_shoot,
//! kar_shoot, bar_shoot, mp44_shoot, luger_shoot, mg42_shoot, ... every one
//! of them, no exceptions found), which is enough on its own to distinguish
//! a gunshot from reloads, footsteps, voice lines, etc. -- no need to also
//! track weapon-fire *events* the way the animation fix does.

use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};

use crate::engine::{self, EventApiPartial};

pub static ENABLED: AtomicBool = AtomicBool::new(false);

const ATTN_NONE: f32 = 0.0;

// Counters, not per-call logging: this runs for every sound the engine plays,
// so a log line per call would flood the file and cost real time in a capture.
// They answer the question a silent take actually raises -- did the hook run
// at all, did it ever see a gunshot, and did it boost one.
static CALLS: AtomicU32 = AtomicU32::new(0);
static SHOOT_SAMPLES: AtomicU32 = AtomicU32::new(0);
static BOOSTED: AtomicU32 = AtomicU32::new(0);
static SKIPPED_NOT_SPECTATING: AtomicU32 = AtomicU32::new(0);

/// One-line summary of what the hook has actually done this session, for the
/// `dodtools_hltv_gunshots_fix` status reply.
pub fn status() -> String {
    format!(
        "sounds seen: {}, weapon-fire samples: {}, boosted: {}, skipped (not spectating): {}",
        CALLS.load(Ordering::Relaxed),
        SHOOT_SAMPLES.load(Ordering::Relaxed),
        BOOSTED.load(Ordering::Relaxed),
        SKIPPED_NOT_SPECTATING.load(Ordering::Relaxed),
    )
}

/// Substring test without allocating -- the previous `to_string_lossy()` built
/// a `String` for every sound the engine played.
fn is_weapon_fire(sample: *const c_char) -> bool {
    if sample.is_null() {
        return false;
    }
    unsafe { CStr::from_ptr(sample) }.to_bytes().windows(6).any(|w| w == b"_shoot")
}

type EvPlaySoundFn = unsafe extern "C" fn(i32, *mut f32, i32, *const c_char, f32, f32, i32, i32);
static REAL_EV_PLAY_SOUND: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

unsafe extern "C" fn hook_ev_play_sound(
    ent: i32,
    origin: *mut f32,
    channel: i32,
    sample: *const c_char,
    volume: f32,
    attenuation: f32,
    f_flags: i32,
    pitch: i32,
) {
    let real = REAL_EV_PLAY_SOUND.load(Ordering::Acquire);
    let real: EvPlaySoundFn = unsafe { std::mem::transmute(real) };

    if CALLS.fetch_add(1, Ordering::Relaxed) == 0 {
        unsafe { crate::debug::report("sound_fix: EV_PlaySound hook is live (first sound played through it)") };
    }

    let is_fire = is_weapon_fire(sample);
    if is_fire {
        SHOOT_SAMPLES.fetch_add(1, Ordering::Relaxed);
    }
    let spectating = engine::engfuncs().map(|e| unsafe { (e.is_spectate_only)() } != 0).unwrap_or(false);
    let should_boost = ENABLED.load(Ordering::Relaxed) && is_fire && spectating;

    if should_boost {
        if BOOSTED.fetch_add(1, Ordering::Relaxed) == 0 {
            let name = unsafe { CStr::from_ptr(sample) }.to_string_lossy().into_owned();
            unsafe {
                crate::debug::report(&format!(
                    "sound_fix: first boosted gunshot -- \"{name}\" (was volume {volume}, attenuation {attenuation}; now 1.0 / {ATTN_NONE})"
                ))
            };
        }
        unsafe { real(ent, origin, channel, sample, 1.0, ATTN_NONE, f_flags, pitch) };
    } else {
        // The most likely reason a take sounds unchanged with the fix on: the
        // gunshots are there, but IsSpectateOnly() is false so nothing boosts.
        if is_fire && ENABLED.load(Ordering::Relaxed) && !spectating && SKIPPED_NOT_SPECTATING.fetch_add(1, Ordering::Relaxed) == 0 {
            unsafe { crate::debug::report("sound_fix: saw a weapon-fire sample but IsSpectateOnly() is false, so it was left alone -- the fix only acts while spectating") };
        }
        unsafe { real(ent, origin, channel, sample, volume, attenuation, f_flags, pitch) };
    }
}

/// Installs the `EV_PlaySound` hook. Must be called after `engine::engfuncs()`
/// returns `Some` (i.e. after `client.dll` has finished loading), since it
/// needs a valid `p_event_api` pointer to patch.
pub fn install() {
    let Some(engfuncs) = engine::engfuncs() else {
        unsafe { crate::debug::report("sound_fix::install called before engfuncs were captured -- this is a bug in install ordering") };
        return;
    };

    let event_api: *mut EventApiPartial = engfuncs.p_event_api;
    if event_api.is_null() {
        unsafe { crate::debug::report("sound_fix: pEventAPI is null, cannot install") };
        return;
    }

    unsafe {
        let real = (*event_api).ev_play_sound;
        REAL_EV_PLAY_SOUND.store(real as *mut c_void, Ordering::Release);
        (*event_api).ev_play_sound = hook_ev_play_sound;
    }
}
