//! DoD 1.3 viewmodel deploy-animation fix for HLAE's in-eye spectating of
//! MG42, MG34, BAR, and Bren -- see the R&D write-up for the full mechanism.
//! Ported from the prototype patch written against HLAE's own source
//! (`AfxHookGoldSrc/hooks/client/dod/ViewmodelAnimationFix.cpp`); this is the
//! same logic, adapted to read the engine interfaces this crate captures
//! itself instead of ones HLAE already has resolved.
//!
//! Short version: these weapons' first-person viewmodels keep two parallel
//! sequence families baked into one model file -- "up" (hip-fire) and "down"
//! (bipod-deployed), e.g. `upidle`/`downidle`, `up_shoot`/`down_shoot` -- and
//! nothing tells HLAE's puppeted in-eye viewmodel which family to use, since
//! that's normally decided by client-side prediction logic HLAE bypasses.
//! Confirmed against the actual `.mdl` files shipped with DoD 1.3.
//!
//! Unlike Counter-Strike's silenced/unsilenced M4A1 fix this is modeled on,
//! DoD's deploy state isn't hidden in a fire-event side channel: it drives a
//! real, always-replicated `entity_state_t::weaponmodel` swap between two
//! separate third-person attachment models (e.g. `p_mg42bu.mdl` <->
//! `p_mg42bd.mdl`), so this fix reads the spectated player's current weapon
//! model name directly every frame instead of tracking fire events.

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
    let (numseq, seqindex) = unsafe { ((*header).numseq, (*header).seqindex) };
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
    let labels = model_sequence_strings(viewmodel);
    match labels.iter().position(|l| l.to_lowercase().contains(&label.to_lowercase())) {
        Some(i) => apply_deploy_state_to_sequence(i as i32, state, viewmodel),
        None => -1,
    }
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

fn play_viewmodel_animation(sequence: i32) {
    if sequence >= 0
        && let Some(engfuncs) = engine::engfuncs()
    {
        unsafe { (engfuncs.pfn_weapon_anim)(sequence, 0) };
    }
}

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
        _ => "unknown",
    }
}

/// The stage plus the viewmodel pointers behind it, as of the last frame that
/// changed either. Logging keys off all three, so an alternation between two
/// different viewmodels and one viewmodel flickering to null look different in
/// the log instead of both reading as "stage changed".
static LAST_TRACE: Mutex<Option<(i32, usize, usize)>> = Mutex::new(None);
static TRACE_LINES: AtomicI32 = AtomicI32::new(0);
/// The flicker being investigated is per-frame, so this has to be capped or a
/// single session would write tens of thousands of lines.
const MAX_TRACE_LINES: i32 = 80;

fn stage(stage: i32) {
    stage_with(stage, std::ptr::null_mut::<u8>(), std::ptr::null_mut::<u8>());
}

/// Records how far this frame got, logging only when the stage or either
/// pointer changes.
fn stage_with<A, B>(stage: i32, entity: *mut A, model: *mut B) {
    STAGE.store(stage, Ordering::Relaxed);

    let key = (stage, entity as usize, model as usize);
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
            "anim_fix: {} | viewmodel entity {entity:p}, model {model:p}",
            stage_name(stage)
        ))
    };
}

/// One-line summary for the `dodtools_hltv_animation_fix` status reply.
pub fn status() -> String {
    let seen = SEEN_VIEWMODELS.lock().unwrap();
    let count = seen.as_ref().map(|s| s.len()).unwrap_or(0);
    format!(
        "state: {} (distinct viewmodels seen: {count})",
        stage_name(STAGE.load(Ordering::Relaxed))
    )
}

/// Every distinct viewmodel this session, logged once each.
///
/// `apply()` keys entirely off the viewmodel's model name, so when it reports
/// "not one of the deployable weapons" the only useful follow-up is *which*
/// model it actually saw. Capped, and one line per distinct name rather than
/// per frame.
static SEEN_VIEWMODELS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn note_viewmodel(name: &str, deployable: bool) {
    const LIMIT: usize = 24;
    let mut guard = SEEN_VIEWMODELS.lock().unwrap();
    let seen = guard.get_or_insert_with(HashSet::new);
    if seen.len() < LIMIT && seen.insert(name.to_string()) {
        unsafe {
            crate::debug::report(&format!(
                "anim_fix: viewmodel seen -- \"{name}\" (deployable weapon: {})",
                if deployable { "yes" } else { "no" }
            ))
        };
    }
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
        stage_with(STAGE_NO_VIEWMODEL_ENTITY, viewmodel_entity, std::ptr::null_mut::<u8>());
        return;
    }
    let viewmodel_model = unsafe { (*viewmodel_entity).model };
    if viewmodel_model.is_null() {
        stage_with(STAGE_NO_VIEWMODEL_MODEL, viewmodel_entity, viewmodel_model);
        return;
    }
    let viewmodel_name = unsafe { (*viewmodel_model).name_str() }.into_owned();
    let deployable = find_deployable_weapon(&viewmodel_name);
    note_viewmodel(&viewmodel_name, deployable.is_some());

    let Some(weapon) = deployable else {
        stage_with(STAGE_NOT_A_DEPLOYABLE_WEAPON, viewmodel_entity, viewmodel_model);
        PREVIOUS_DEPLOY_STATE.store(-1, Ordering::Relaxed);
        return;
    };

    let viewmodel_index = unsafe { (*viewmodel_entity).index };
    let previous_entity = PREVIOUS_SPECTATED_ENTITY.swap(viewmodel_index, Ordering::Relaxed);
    let switched_players = previous_entity != viewmodel_index;

    let spectated = unsafe { (engfuncs.get_entity_by_index)(viewmodel_index) };
    if spectated.is_null() || unsafe { (*spectated).player } == 0 {
        stage_with(STAGE_NO_SPECTATED_PLAYER, viewmodel_entity, viewmodel_model);
        return;
    }
    let spectated = unsafe { &*spectated };
    stage_with(STAGE_RUNNING, viewmodel_entity, viewmodel_model);

    let state = get_spectated_deploy_state(weapon, spectated);
    let previous_state = i32_to_deploy_state(PREVIOUS_DEPLOY_STATE.load(Ordering::Relaxed));
    let deploy_state_changed = previous_state.is_some() && state.is_some() && previous_state != state;

    let previous_viewmodel = PREVIOUS_VIEWMODEL.swap(viewmodel_model, Ordering::Relaxed);
    let viewmodel_changed = previous_viewmodel != viewmodel_model && !previous_viewmodel.is_null();

    if switched_players {
        // Snap the new viewmodel straight to the right family's idle so it
        // doesn't sit on whatever sequence the previously-spectated player
        // left it on.
        play_viewmodel_animation(animation_lookup_sequence("idle", state, viewmodel_model));
    } else if deploy_state_changed {
        // TODO(R&D, unverified live): play the "uptodown"/"downtoup"-style
        // transition sequence here instead of snapping straight to idle.
        play_viewmodel_animation(animation_lookup_sequence("idle", state, viewmodel_model));
    } else {
        let previous_sequence = PREVIOUS_SEQUENCE.load(Ordering::Relaxed);
        // Use the spectated player's own body-model sequence table to
        // classify their current action, not the viewmodel's.
        if previous_sequence != spectated.curstate.sequence && !spectated.model.is_null() {
            let labels = model_sequence_strings(spectated.model);
            let seq = spectated.curstate.sequence.max(0) as usize;
            if labels.get(seq).is_some_and(|label| label.to_lowercase().contains("reload")) {
                play_viewmodel_animation(animation_lookup_sequence("reload", state, viewmodel_model));
            }
        }

        if viewmodel_changed {
            play_viewmodel_animation(animation_lookup_sequence("draw", state, viewmodel_model));
        }
    }

    PREVIOUS_DEPLOY_STATE.store(deploy_state_to_i32(state), Ordering::Relaxed);
    PREVIOUS_SEQUENCE.store(spectated.curstate.sequence, Ordering::Relaxed);
}
