//! Drives the first-person viewmodel's animations while spectating a player
//! in-eye, which the engine otherwise leaves largely static.
//!
//! ## Why the animations are missing
//!
//! Structural, not a bug. GoldSrc's weapon event scripts animate the viewmodel
//! only for the **local** player -- the `EV_IsLocal` check in every
//! `events/weapons/*.sc`. Every other player gets the sound and the muzzle
//! flash but no first-person animation, because normally nobody is looking
//! down their sights. Spectating in-eye, and HLTV playback in particular, is
//! exactly the case that assumption does not hold for: the viewmodel on screen
//! belongs to someone who is not the local player, so firing, reloading and
//! drawing never reach it.
//!
//! ## What this drives, and from where
//!
//! - **shoot** -- from the weapon-fire sound, which names the entity that
//!   fired (`sound_fix`'s `EV_PlaySound` hook, see `on_weapon_fired`). This is
//!   the most visible omission, and the fire events are present in HLTV demos
//!   even though some rounds of automatic fire are dropped.
//! - **reload** -- from the spectated player's own third-person sequence
//!   label, which *is* replicated.
//! - **draw** -- from the viewmodel's model changing.
//! - **idle** -- on switching to a different spectated player, so the new
//!   viewmodel does not inherit whatever sequence the last one was left on.
//!
//! ## Bipod weapons, additionally
//!
//! MG42, MG34, BAR and Bren keep two parallel sequence families in one model
//! -- "up" (hip-fire) and "down" (deployed), e.g. `upidle`/`downidle` -- and
//! nothing tells a puppeted in-eye viewmodel which to use. Unlike Counter-
//! Strike's silenced/unsilenced M4A1 fix this was modeled on, DoD's deploy
//! state is not hidden in a fire-event side channel: it drives a real,
//! always-replicated `entity_state_t::weaponmodel` swap between two
//! third-person models (`p_mg42bu.mdl` <-> `p_mg42bd.mdl`), so it can be read
//! directly each frame. Every animation above is then looked up within
//! whichever family is current. Confirmed against the `.mdl` files shipped
//! with DoD 1.3.
//!
//! This part matters far less in practice than the plain animations: league
//! configs generally limit the MGs to zero, and deploying the BAR's bipod is
//! rare. It is a refinement on top, not the point.
//!
//! Originally ported from a prototype written against HLAE's own source
//! (`AfxHookGoldSrc/hooks/client/dod/ViewmodelAnimationFix.cpp`), adapted to
//! the engine interfaces this crate captures itself.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, Ordering};
use std::sync::Mutex;

use crate::engine::{self, ClEntityS, ModelSPartial, StudioHdrPartial, StudioSeqDescPartial};

pub static ENABLED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq, Eq)]
enum DeployState {
    Up,
    Down,
}

struct DeployableWeapon {
    viewmodel_match: &'static str,
    deployed_marker: &'static str,
    undeployed_marker: &'static str,
}

// Confirmed against the actual .mdl files shipped with DoD 1.3 (see the R&D
// write-up for the sequence-label dump). All four use the same "up"/"down"
// (or "up_"/"down_") first-person sequence-family split as the third-person
// p_*bu.mdl / p_*bd.mdl model swap.
const DEPLOYABLE_WEAPONS: &[DeployableWeapon] = &[
    DeployableWeapon { viewmodel_match: "mg42", deployed_marker: "bd.mdl", undeployed_marker: "bu.mdl" },
    DeployableWeapon { viewmodel_match: "mg34", deployed_marker: "bd.mdl", undeployed_marker: "bu.mdl" },
    DeployableWeapon { viewmodel_match: "bar", deployed_marker: "bd.mdl", undeployed_marker: "bu.mdl" },
    DeployableWeapon { viewmodel_match: "bren", deployed_marker: "bd.mdl", undeployed_marker: "bu.mdl" },
    // Also matches v_scopedfg42.mdl, which is correct: it has the same
    // up_*/down_* sequence set. It ships only p_scopedfg42bu.mdl with no "bd"
    // counterpart, so its deploy state simply always reads as up, which is
    // what a scoped FG42 does.
    DeployableWeapon { viewmodel_match: "fg42", deployed_marker: "bd.mdl", undeployed_marker: "bu.mdl" },
    // v_30cal.mdl has the same upidle/downidle first-person split, but its
    // p_30cal*.mdl set (p_30cal / p_30calpr / p_30calr / p_30calsr) has no
    // matching bd/bu third-person pair -- DoD 1.3's 30cal is normally a
    // fixed, already-mounted tripod gun rather than carried and
    // bipod-deployed the way the other four are, so its "up"/"down"
    // viewmodel sequences likely key off something other than a weaponmodel
    // swap. Left out until confirmed live; see the R&D write-up.
];

fn find_deployable_weapon(viewmodel_name: &str) -> Option<&'static DeployableWeapon> {
    DEPLOYABLE_WEAPONS.iter().find(|w| viewmodel_name.contains(w.viewmodel_match))
}

/// `"models/v_98k.mdl"` -> `"98k"`, `"models/p_mg42bd.mdl"` -> `"mg42bd"`.
fn model_stem(name: &str) -> &str {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let file = file.strip_suffix(".mdl").unwrap_or(file);
    file.strip_prefix("v_").or_else(|| file.strip_prefix("p_")).unwrap_or(file)
}

/// Whether the viewmodel on screen is the weapon the spectated player is
/// actually holding.
///
/// The viewmodel pointer alone cannot be trusted: live logging shows it
/// alternating between two of a player's weapons (v_98k and v_luger) every few
/// milliseconds, which fires a spurious "draw" on every flip. The spectated
/// player's `curstate.weaponmodel` is replicated entity state and does not
/// flap, so it is the authority on what is held; the viewmodel is only
/// consulted for its sequence list once the two agree.
///
/// Matching is viewmodel-stem inside third-person name rather than the
/// reverse, because the bipod weapons append a deploy suffix on the
/// third-person side only (`v_mg42.mdl` vs `p_mg42bu.mdl` / `p_mg42bd.mdl`).
fn viewmodel_matches_held_weapon(viewmodel_name: &str, spectated: &ClEntityS) -> Option<bool> {
    let verdict = viewmodel_match_inner(viewmodel_name, spectated);

    // The filter is still letting a spurious draw through, alternating between
    // v_bar and v_colt, so log how the decision is actually being reached for
    // the first few. `None` means the held weapon could not be resolved at all,
    // which is currently treated as "allow" and would explain it.
    if MATCH_LOGS.fetch_add(1, Ordering::Relaxed) < MAX_MATCH_LOGS {
        let held = engine::engine_studio()
            .map(|studio| unsafe { (studio.get_model_by_index)(spectated.curstate.weaponmodel) })
            .filter(|m| !m.is_null())
            .map(|m| unsafe { (*m).name_str() }.into_owned())
            .unwrap_or_else(|| "<unresolved>".into());
        unsafe {
            crate::debug::report(&format!(
                "anim_fix: match check -- viewmodel \"{viewmodel_name}\" vs held \"{held}\" (weaponmodel index {}) -> {}",
                spectated.curstate.weaponmodel,
                match verdict {
                    Some(true) => "match",
                    Some(false) => "MISMATCH, frame skipped",
                    None => "UNRESOLVED, frame allowed through",
                }
            ))
        };
    }

    verdict
}

static MATCH_LOGS: AtomicI32 = AtomicI32::new(0);
const MAX_MATCH_LOGS: i32 = 30;

fn viewmodel_match_inner(viewmodel_name: &str, spectated: &ClEntityS) -> Option<bool> {
    let studio = engine::engine_studio()?;
    let held = unsafe { (studio.get_model_by_index)(spectated.curstate.weaponmodel) };
    if held.is_null() {
        return None;
    }
    let held_name = unsafe { (*held).name_str() }.into_owned();
    let stem = model_stem(viewmodel_name);
    if stem.is_empty() {
        return None;
    }
    Some(model_stem(&held_name).contains(stem))
}

static SEQUENCE_CACHE: Mutex<Option<HashMap<usize, Vec<String>>>> = Mutex::new(None);

/// Returns every sequence label baked into `model`, cached by the model
/// pointer's address (stable for the life of a precached model).
fn model_sequence_strings(model: *mut ModelSPartial) -> Vec<String> {
    let key = model as usize;
    let mut cache = SEQUENCE_CACHE.lock().unwrap();
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(cached) = cache.get(&key) {
        return cached.clone();
    }

    let Some(studio) = engine::engine_studio() else { return Vec::new() };
    let extradata = unsafe { (studio.mod_extradata)(model) };
    if extradata.is_null() {
        return Vec::new();
    }

    let header = extradata as *const StudioHdrPartial;
    // Validate before trusting anything in here. `mod_extradata` is happy to
    // hand back a pointer for a model that is not a studio model at all, and
    // the loop below walks `numseq` entries at `seqindex` with no bound of its
    // own -- a garbage header would read arbitrary memory until it faulted.
    const STUDIO_MAGIC: i32 = 0x5453_4449; // "IDST"
    const MAX_SEQUENCES: i32 = 512;
    let (id, numseq, seqindex) = unsafe { ((*header).id, (*header).numseq, (*header).seqindex) };
    if id != STUDIO_MAGIC || !(0..=MAX_SEQUENCES).contains(&numseq) || seqindex <= 0 {
        unsafe {
            crate::debug::report(&format!(
                "anim_fix: refusing to read sequences from {model:p} -- header id {id:#x}, numseq {numseq}, seqindex {seqindex}"
            ))
        };
        cache.insert(key, Vec::new());
        return Vec::new();
    }
    let base = extradata as *const u8;

    let mut labels = Vec::with_capacity(numseq.max(0) as usize);
    for i in 0..numseq {
        let entry = unsafe { base.add(seqindex as usize + i as usize * size_of::<StudioSeqDescPartial>()) }
            as *const StudioSeqDescPartial;
        labels.push(unsafe { (*entry).label_str() }.into_owned());
    }

    cache.insert(key, labels.clone());
    labels
}

fn sequence_family(label: &str) -> Option<DeployState> {
    if label.len() >= 4 && label[..4].eq_ignore_ascii_case("down") {
        Some(DeployState::Down)
    } else if label.len() >= 2 && label[..2].eq_ignore_ascii_case("up") {
        Some(DeployState::Up)
    } else {
        None
    }
}

/// "upidle" <-> "downidle", "up_idle" <-> "down_idle" -- keeps whatever
/// followed the prefix (including a leading underscore, if any) intact.
fn swap_family_prefix(label: &str, target: DeployState) -> String {
    match sequence_family(label) {
        Some(current) if current != target => {
            let prefix_len = if current == DeployState::Up { 2 } else { 4 };
            let rest = &label[prefix_len..];
            let new_prefix = if target == DeployState::Up { "up" } else { "down" };
            format!("{new_prefix}{rest}")
        }
        _ => label.to_string(),
    }
}

fn apply_deploy_state_to_sequence(sequence: i32, state: Option<DeployState>, viewmodel: *mut ModelSPartial) -> i32 {
    let Some(state) = state else { return sequence };
    let labels = model_sequence_strings(viewmodel);
    let Some(current_label) = labels.get(sequence.max(0) as usize) else { return sequence };

    let wanted = swap_family_prefix(current_label, state);
    labels
        .iter()
        .position(|l| l.eq_ignore_ascii_case(&wanted))
        .map(|i| i as i32)
        // No matching sequence in the other family (e.g. mg42/mg34's single
        // shared "reload") -- keep the caller's original index.
        .unwrap_or(sequence)
}

fn animation_lookup_sequence(label: &str, state: Option<DeployState>, viewmodel: *mut ModelSPartial) -> i32 {
    animation_lookup_any(&[label], state, viewmodel)
}

/// Finds the first sequence matching any of `candidates`, in order.
///
/// DoD's models are not consistent about what they call things -- a firing
/// animation is `shoot` on some weapons and `fire` on others -- so the caller
/// gives the names worth trying rather than assuming one.
fn animation_lookup_any(candidates: &[&str], state: Option<DeployState>, viewmodel: *mut ModelSPartial) -> i32 {
    let labels = model_sequence_strings(viewmodel);

    // Exact match first, substring only as a fallback. Several models list a
    // qualified variant *before* the plain one -- v_luger.mdl is
    // [.., 5:reload_empty, 6:reload, ..] -- so a substring-first search picks
    // the wrong animation, which is exactly what made a luger reload play as
    // reload_empty in testing. The fallback still matters, because the bipod
    // weapons have no bare label at all: v_bar.mdl is up_reload / down_reload,
    // v_mg42.mdl is upshoot / downshoot.
    for candidate in candidates {
        let needle = candidate.to_lowercase();
        if let Some(i) = labels.iter().position(|l| l.to_lowercase() == needle) {
            return apply_deploy_state_to_sequence(i as i32, state, viewmodel);
        }
    }
    for candidate in candidates {
        let needle = candidate.to_lowercase();
        if let Some(i) = labels.iter().position(|l| l.to_lowercase().contains(&needle)) {
            return apply_deploy_state_to_sequence(i as i32, state, viewmodel);
        }
    }
    -1
}

fn get_spectated_deploy_state(weapon: &DeployableWeapon, entity: &ClEntityS) -> Option<DeployState> {
    let studio = engine::engine_studio()?;
    let weapon_model = unsafe { (studio.get_model_by_index)(entity.curstate.weaponmodel) };
    if weapon_model.is_null() {
        return None;
    }
    let name = unsafe { (*weapon_model).name_str() };
    if name.contains(weapon.deployed_marker) {
        Some(DeployState::Down)
    } else if name.contains(weapon.undeployed_marker) {
        Some(DeployState::Up)
    } else {
        None
    }
}

/// Counts the animations this fix has actually forced, so a session can be
/// judged without trusting scrollback -- reaching "running" says the
/// preconditions held, not that anything was corrected.
static ANIMATIONS_PLAYED: AtomicI32 = AtomicI32::new(0);
static ANIMATION_LOGS: AtomicI32 = AtomicI32::new(0);
const MAX_ANIMATION_LOGS: i32 = 40;

fn play_viewmodel_animation(sequence: i32, reason: &str, state: Option<DeployState>, viewmodel: *mut ModelSPartial) {
    if sequence < 0 {
        // Worth seeing: it means the model had no sequence matching what the
        // deploy state asked for, which is a gap in the up/down mapping rather
        // than a no-op.
        if ANIMATION_LOGS.fetch_add(1, Ordering::Relaxed) < MAX_ANIMATION_LOGS {
            unsafe { crate::debug::report(&format!("anim_fix: {reason} -- no matching sequence found, nothing played")) };
        }
        return;
    }
    let Some(engfuncs) = engine::engfuncs() else { return };

    ANIMATIONS_PLAYED.fetch_add(1, Ordering::Relaxed);
    if ANIMATION_LOGS.fetch_add(1, Ordering::Relaxed) < MAX_ANIMATION_LOGS {
        let label = model_sequence_strings(viewmodel)
            .get(sequence as usize)
            .cloned()
            .unwrap_or_else(|| "<unknown>".into());
        let family = match state {
            Some(DeployState::Up) => "bipod up",
            Some(DeployState::Down) => "bipod down",
            None => "deploy state unknown",
        };
        unsafe {
            crate::debug::report(&format!(
                "anim_fix: {reason} -- {family}, playing sequence {sequence} (\"{label}\")"
            ))
        };
    }

    unsafe { (engfuncs.pfn_weapon_anim)(sequence, 0) };
}

/// What `apply()` last saw, published for `on_weapon_fired`, which runs from
/// the sound hook on the same thread but has none of this context.
static FIRE_LOGS: AtomicI32 = AtomicI32::new(0);
const MAX_FIRE_LOGS: i32 = 25;

static CURRENT_SPECTATED: AtomicI32 = AtomicI32::new(-1);
static CURRENT_VIEWMODEL: AtomicPtr<ModelSPartial> = AtomicPtr::new(std::ptr::null_mut());
static CURRENT_DEPLOY_STATE: AtomicI32 = AtomicI32::new(-1);

static PREVIOUS_SPECTATED_ENTITY: AtomicI32 = AtomicI32::new(-1);
static PREVIOUS_SEQUENCE: AtomicI32 = AtomicI32::new(-1);
static PREVIOUS_DEPLOY_STATE: AtomicI32 = AtomicI32::new(-1); // -1 none, 0 up, 1 down
static PREVIOUS_VIEWMODEL: AtomicPtr<ModelSPartial> = AtomicPtr::new(std::ptr::null_mut());

fn deploy_state_to_i32(state: Option<DeployState>) -> i32 {
    match state {
        None => -1,
        Some(DeployState::Up) => 0,
        Some(DeployState::Down) => 1,
    }
}

fn i32_to_deploy_state(v: i32) -> Option<DeployState> {
    match v {
        0 => Some(DeployState::Up),
        1 => Some(DeployState::Down),
        _ => None,
    }
}

/// Registers `apply()` to run every client frame. Safe to call regardless of
/// whether `ENABLED` is set -- `apply()` checks that itself, so this can be
/// installed unconditionally and toggled purely via the env var at any time
/// during the session (there's no per-install teardown needed).
pub fn install() {
    engine::set_per_frame_callback(apply);
}

/// How far `apply()` got on the most recent frame. Reported only when it
/// *changes*, never per frame -- this runs 60+ times a second, so a line per
/// call would flood the log and slow a capture. Logged as a trace, it answers
/// the only question a take that looks unchanged actually raises: which of the
/// preconditions is the one not being met.
static STAGE: AtomicI32 = AtomicI32::new(-1);

const STAGE_DISABLED: i32 = 0;
const STAGE_NO_ENGFUNCS: i32 = 1;
const STAGE_NOT_SPECTATING: i32 = 2;
const STAGE_NO_VIEWMODEL_ENTITY: i32 = 3;
const STAGE_NO_VIEWMODEL_MODEL: i32 = 4;
const STAGE_NOT_A_DEPLOYABLE_WEAPON: i32 = 5;
const STAGE_NO_SPECTATED_PLAYER: i32 = 6;
const STAGE_RUNNING: i32 = 7;
const STAGE_VIEWMODEL_MISMATCH: i32 = 8;

fn stage_name(stage: i32) -> &'static str {
    match stage {
        STAGE_DISABLED => "disabled (dodtools_hltv_animation_fix is 0)",
        STAGE_NO_ENGFUNCS => "waiting for engfuncs",
        STAGE_NOT_SPECTATING => "not spectating (IsSpectateOnly() is false) -- the fix only acts in a spectated view",
        STAGE_NO_VIEWMODEL_ENTITY => "no viewmodel entity",
        STAGE_NO_VIEWMODEL_MODEL => "viewmodel entity has no model",
        STAGE_NOT_A_DEPLOYABLE_WEAPON => "viewmodel is not one of the deployable weapons (MG42/MG34/BAR/Bren)",
        STAGE_NO_SPECTATED_PLAYER => "spectated entity is missing or is not a player",
        STAGE_RUNNING => "running -- all preconditions met",
        STAGE_VIEWMODEL_MISMATCH => "viewmodel is not the weapon the spectated player is holding (ignored this frame)",
        _ => "unknown",
    }
}

/// The stage plus the viewmodel pointers behind it, as of the last frame that
/// changed either. Logging keys off all three, so an alternation between two
/// different viewmodels and one viewmodel flickering to null look different in
/// the log instead of both reading as "stage changed".
static LAST_TRACE: Mutex<Option<(i32, usize, usize, i32)>> = Mutex::new(None);
static TRACE_LINES: AtomicI32 = AtomicI32::new(0);
/// The flicker being investigated is per-frame, so this has to be capped or a
/// single session would write tens of thousands of lines.
const MAX_TRACE_LINES: i32 = 80;

fn stage(stage: i32) {
    stage_with(stage, std::ptr::null_mut::<u8>(), std::ptr::null_mut::<u8>(), -1);
}

/// Records how far this frame got, logging only when the stage, either
/// pointer, or the entity index changes.
///
/// `index` is the viewmodel entity's own `index` field, which `apply()` uses
/// as the spectated player. Logging it answers the question the model pointer
/// alone leaves open: whether the weapon changing means the *spectated player*
/// changed (the director moving on) or the same player swapped weapons.
fn stage_with<A, B>(stage: i32, entity: *mut A, model: *mut B, index: i32) {
    STAGE.store(stage, Ordering::Relaxed);

    let key = (stage, entity as usize, model as usize, index);
    let mut last = LAST_TRACE.lock().unwrap();
    if *last == Some(key) {
        return;
    }
    *last = Some(key);
    drop(last);

    if TRACE_LINES.fetch_add(1, Ordering::Relaxed) >= MAX_TRACE_LINES {
        return;
    }
    unsafe {
        crate::debug::report(&format!(
            "anim_fix: {} | entity {entity:p} idx {index}, model {model:p}",
            stage_name(stage)
        ))
    };
}

/// One-line summary for the `dodtools_hltv_animation_fix` status reply.
pub fn status() -> String {
    let seen = SEEN_VIEWMODELS.lock().unwrap();
    let count = seen.as_ref().map(|s| s.len()).unwrap_or(0);
    format!(
        "state: {} (distinct viewmodels seen: {count}, animations corrected: {})",
        stage_name(STAGE.load(Ordering::Relaxed)),
        ANIMATIONS_PLAYED.load(Ordering::Relaxed),
    )
}

/// Every distinct viewmodel this session, logged once each.
///
/// `apply()` keys entirely off the viewmodel's model name, so when it reports
/// "not one of the deployable weapons" the only useful follow-up is *which*
/// model it actually saw. Capped, and one line per distinct name rather than
/// per frame.
static SEEN_VIEWMODELS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn note_viewmodel(name: &str, deployable: bool, model: *mut ModelSPartial) {
    const LIMIT: usize = 24;
    let mut guard = SEEN_VIEWMODELS.lock().unwrap();
    let seen = guard.get_or_insert_with(HashSet::new);
    if seen.len() >= LIMIT || !seen.insert(name.to_string()) {
        return;
    }
    drop(guard);

    // Dump the model's whole sequence list the first time it is seen. Every
    // animation this fix plays is found by matching a label ("shoot", "draw",
    // "reload"), so when a lookup comes back empty the only thing worth
    // knowing is what the model actually calls its animations -- and DoD is
    // not consistent about it. One line per weapon, not per frame.
    let labels = model_sequence_strings(model);
    unsafe {
        crate::debug::report(&format!(
            "anim_fix: viewmodel seen -- \"{name}\" (deployable weapon: {}), {} sequences: [{}]",
            if deployable { "yes" } else { "no" },
            labels.len(),
            labels
                .iter()
                .enumerate()
                .map(|(i, l)| format!("{i}:{l}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    };
}

/// Plays the firing animation when the player being spectated in-eye shoots.
///
/// This is the animation most obviously missing in an HLTV demo, and the
/// reason is structural rather than a bug: GoldSrc's weapon event scripts only
/// drive the viewmodel for the *local* player (`EV_IsLocal`). Everyone else
/// gets the sound and the muzzle flash but no first-person animation, because
/// normally nobody is looking down their sights. Spectating in-eye is exactly
/// the case that assumption doesn't hold for.
///
/// Driven from `sound_fix`'s `EV_PlaySound` hook rather than from the frame
/// callback: the fire sound carries the entity that fired, which is precisely
/// the signal needed and is already being intercepted. Called on the engine
/// thread, same as `apply()`.
pub fn on_weapon_fired(entity_index: i32) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }

    // Every weapon-fire sound in the match arrives here, and so far *none* has
    // matched, so log the first few raw: if the entity a fire sound reports is
    // never the one being spectated, the sound's `ent` is not the shooter and
    // this whole trigger needs a different signal.
    let spectated = CURRENT_SPECTATED.load(Ordering::Relaxed);
    if FIRE_LOGS.fetch_add(1, Ordering::Relaxed) < MAX_FIRE_LOGS {
        unsafe {
            crate::debug::report(&format!(
                "anim_fix: weapon-fire sound from entity {entity_index}, currently spectating {spectated} -- {}",
                if entity_index == spectated { "MATCH" } else { "ignored" }
            ))
        };
    }

    // Only the player actually being watched.
    if entity_index < 0 || entity_index != spectated {
        return;
    }
    let viewmodel = CURRENT_VIEWMODEL.load(Ordering::Relaxed);
    if viewmodel.is_null() {
        return;
    }

    let state = i32_to_deploy_state(CURRENT_DEPLOY_STATE.load(Ordering::Relaxed));
    // Every DoD viewmodel names its firing animation one of these, confirmed
    // by dumping the sequence list of all 41 v_*.mdl files: plain "shoot"
    // (98k, enfield, luger, sten, webley, m1carbine), numbered "shoot1"
    // (colt, garand, k43, mp40, mp44, tommy, greasegun, spring), prefixed
    // "up_shoot"/"upshoot" (bar, bren, fg42, mg42, mg34, 30cal), "launch"
    // (bazooka, panzerschreck, piat) or "fire" (mortar).
    let sequence = animation_lookup_any(&["shoot", "launch", "fire"], state, viewmodel);
    play_viewmodel_animation(sequence, "spectated player fired", state, viewmodel);
}

/// `"idx 6"`. Names would be nicer, but reaching them needs an engine slot
/// that cannot be verified against any call site in client.dll -- see the note
/// on `ClEngineFuncsPartial`. The index is stable within a match and can be
/// matched against the scoreboard.
fn describe_player(index: i32) -> String {
    format!("idx {index}")
}

/// Runs once per client frame (see `engine::set_per_frame_callback`).
pub fn apply() {
    if !ENABLED.load(Ordering::Relaxed) {
        stage(STAGE_DISABLED);
        return;
    }
    let Some(engfuncs) = engine::engfuncs() else {
        stage(STAGE_NO_ENGFUNCS);
        return;
    };
    if unsafe { (engfuncs.is_spectate_only)() } == 0 {
        stage(STAGE_NOT_SPECTATING);
        return;
    }

    let viewmodel_entity = unsafe { (engfuncs.get_view_model)() };
    if viewmodel_entity.is_null() {
        stage_with(STAGE_NO_VIEWMODEL_ENTITY, viewmodel_entity, std::ptr::null_mut::<u8>(), -1);
        return;
    }
    let viewmodel_model = unsafe { (*viewmodel_entity).model };
    if viewmodel_model.is_null() {
        stage_with(STAGE_NO_VIEWMODEL_MODEL, viewmodel_entity, viewmodel_model, unsafe { (*viewmodel_entity).index });
        return;
    }
    let viewmodel_name = unsafe { (*viewmodel_model).name_str() }.into_owned();
    // Only the four bipod weapons have an up/down sequence split; every other
    // weapon still needs draw/reload/shoot driven, so this is no longer a
    // reason to bail out -- it just means there is no deploy state to track.
    let deployable = find_deployable_weapon(&viewmodel_name);
    note_viewmodel(&viewmodel_name, deployable.is_some(), viewmodel_model);

    let viewmodel_index = unsafe { (*viewmodel_entity).index };
    let spectated = unsafe { (engfuncs.get_entity_by_index)(viewmodel_index) };
    if spectated.is_null() || unsafe { (*spectated).player } == 0 {
        stage_with(STAGE_NO_SPECTATED_PLAYER, viewmodel_entity, viewmodel_model, viewmodel_index);
        return;
    }
    let spectated = unsafe { &*spectated };

    // The viewmodel flaps between a player's weapons faster than they could
    // possibly be switching. Acting on a frame where it disagrees with what
    // the player actually holds is what made "draw" fire ~30 times a second,
    // and it also meant the entity index published for the fire trigger was
    // whichever weapon happened to be showing.
    //
    // This has to come before any of the previous-state trackers are touched,
    // or the flap still registers as a change on the next agreeing frame.
    if viewmodel_matches_held_weapon(&viewmodel_name, spectated) == Some(false) {
        stage_with(STAGE_VIEWMODEL_MISMATCH, viewmodel_entity, viewmodel_model, viewmodel_index);
        return;
    }

    let previous_entity = PREVIOUS_SPECTATED_ENTITY.swap(viewmodel_index, Ordering::Relaxed);
    let switched_players = previous_entity != viewmodel_index;
    if switched_players {
        // The single most useful line for reading a session back: which player
        // the camera moved to, and what they are holding according to their own
        // replicated state rather than the viewmodel.
        let held = engine::engine_studio()
            .map(|studio| unsafe { (studio.get_model_by_index)(spectated.curstate.weaponmodel) })
            .filter(|m| !m.is_null())
            .map(|m| unsafe { (*m).name_str() }.into_owned())
            .unwrap_or_else(|| "<unknown>".into());
        unsafe {
            crate::debug::report(&format!(
                "anim_fix: now spectating {} (was {}), holding {held}, viewmodel \"{viewmodel_name}\"",
                describe_player(viewmodel_index),
                describe_player(previous_entity),
            ))
        };
    }

    stage_with(STAGE_RUNNING, viewmodel_entity, viewmodel_model, viewmodel_index);

    let state = deployable.and_then(|weapon| get_spectated_deploy_state(weapon, spectated));

    // Published for the fire-event path, which runs from the sound hook rather
    // than from here and so has no view of any of this.
    CURRENT_SPECTATED.store(viewmodel_index, Ordering::Relaxed);
    CURRENT_VIEWMODEL.store(viewmodel_model, Ordering::Relaxed);
    CURRENT_DEPLOY_STATE.store(deploy_state_to_i32(state), Ordering::Relaxed);
    let previous_state = i32_to_deploy_state(PREVIOUS_DEPLOY_STATE.load(Ordering::Relaxed));
    let deploy_state_changed = previous_state.is_some() && state.is_some() && previous_state != state;

    let previous_viewmodel = PREVIOUS_VIEWMODEL.swap(viewmodel_model, Ordering::Relaxed);
    let viewmodel_changed = previous_viewmodel != viewmodel_model && !previous_viewmodel.is_null();

    if switched_players {
        // Snap the new viewmodel straight to the right family's idle so it
        // doesn't sit on whatever sequence the previously-spectated player
        // left it on.
        play_viewmodel_animation(animation_lookup_sequence("idle", state, viewmodel_model), "spectated player changed", state, viewmodel_model);
    } else if deploy_state_changed {
        // TODO(R&D, unverified live): play the "uptodown"/"downtoup"-style
        // transition sequence here instead of snapping straight to idle.
        play_viewmodel_animation(animation_lookup_sequence("idle", state, viewmodel_model), "bipod deploy state changed", state, viewmodel_model);
    } else {
        let previous_sequence = PREVIOUS_SEQUENCE.load(Ordering::Relaxed);
        // Use the spectated player's own body-model sequence table to
        // classify their current action, not the viewmodel's.
        if previous_sequence != spectated.curstate.sequence && !spectated.model.is_null() {
            let labels = model_sequence_strings(spectated.model);
            let seq = spectated.curstate.sequence.max(0) as usize;
            if labels.get(seq).is_some_and(|label| label.to_lowercase().contains("reload")) {
                play_viewmodel_animation(animation_lookup_sequence("reload", state, viewmodel_model), "spectated player reloaded", state, viewmodel_model);
            }
        }

        if viewmodel_changed {
            play_viewmodel_animation(animation_lookup_sequence("draw", state, viewmodel_model), "viewmodel changed", state, viewmodel_model);
        }
    }

    PREVIOUS_DEPLOY_STATE.store(deploy_state_to_i32(state), Ordering::Relaxed);
    PREVIOUS_SEQUENCE.store(spectated.curstate.sequence, Ordering::Relaxed);
}
