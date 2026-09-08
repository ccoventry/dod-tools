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
//! - **shoot** -- from the spectated player's own body animation. DoD's player
//!   models name every sequence `<stance>_<weapon>_<action>`, so
//!   `stand_bolt_shoot` or `bipod_mg_shoot` says outright that they are firing,
//!   and that is replicated entity state present in an HLTV demo. The
//!   weapon-fire *sound* is kept as a second trigger (`on_weapon_fired`), which
//!   matters only for automatic fire: a held trigger leaves the body sequence
//!   sitting on the same `_shoot` label, so the individual rounds after the
//!   first have no sequence change to key off.
//! - **reload** -- from the spectated player's body animation as well
//!   (`crouch_bar_reload`, `prone_webley_reload`, ...).
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
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicU64, Ordering};
use std::sync::Mutex;

use crate::engine::{self, ClEntityS, ModelSPartial, StudioHdrPartial, StudioSeqDescPartial};

pub static ENABLED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

/// What the player being spectated is doing, read off their own body animation.
///
/// DoD's player models name every sequence `<stance>_<weapon>_<action>` --
/// `stand_bolt_shoot`, `crouch_bar_reload`, `bipod_mg_aim`, `sprint_sten_aim`.
/// Read straight out of `models/player/us-inf/us-inf.mdl`, whose 345 sequences
/// cover every weapon and stance in the game.
///
/// This is the trigger the firing animation hangs off, and the reason it does
/// is that `curstate.sequence` is *replicated*: it survives into an HLTV demo,
/// which almost nothing about another player's weapon does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BodyAction {
    Shoot,
    Reload,
    Other,
}

fn classify_body_sequence(label: &str) -> BodyAction {
    let label = label.to_ascii_lowercase();
    if label.ends_with("_shoot") {
        // Covers every attack, not just gunfire: `stand_gren_shoot` is a
        // grenade throw and `crouch_knife_shoot` a stab, and the viewmodel
        // lookup below has the labels for both.
        BodyAction::Shoot
    } else if label.contains("reload") || label.contains("zoomload") {
        // "zoomload" is the rocket weapons reloading while scoped.
        BodyAction::Reload
    } else {
        BodyAction::Other
    }
}

/// Bipod state read from the player's own body animation.
///
/// Better than the `p_*bu`/`p_*bd` model name it falls back to, which only
/// carries the marker in some stances and so goes unreadable exactly when a
/// machine gunner is prone. The body label carries it in every stance.
fn deploy_state_from_body_sequence(label: &str) -> Option<DeployState> {
    let label = label.to_ascii_lowercase();
    // `sandbag_` is deployed onto cover rather than on the bipod, but it drives
    // the same "down" first-person sequence family.
    if label.starts_with("bipod_") || label.starts_with("sandbag_") {
        Some(DeployState::Down)
    } else if ["stand_", "crouch_", "prone_", "sprint_"].iter().any(|p| label.starts_with(p)) {
        Some(DeployState::Up)
    } else {
        None
    }
}

/// The names DoD's viewmodels give their attack animation, in the order worth
/// trying. Taken from a dump of all 41 `v_*.mdl` sequence lists.
///
/// Plain "shoot" (98k, enfield, luger, sten, webley, m1carbine), numbered
/// "shoot1" (colt, garand, k43, mp40, mp44, tommy, greasegun, spring) and
/// prefixed "up_shoot"/"upshoot" (bar, bren, fg42, mg42, mg34, 30cal) are all
/// reached by the "shoot" entry via the substring fallback. "launch" is the
/// rocket weapons (bazooka, panzerschreck, PIAT), "fire" the mortar, "throw"
/// every grenade, and "slash1" the knife and spade.
const ATTACK_SEQUENCES: &[&str] = &["shoot", "launch", "fire", "throw", "slash1"];

/// `"models/v_98k.mdl"` -> `"98k"`, `"models/p_mg42bd.mdl"` -> `"mg42bd"`.
///
/// DoD uses three model prefixes and all three are stripped: `v_` is the
/// first-person viewmodel (41 files), `p_` the third-person attachment in a
/// player's hands (75), `w_` the world model of a dropped weapon (56).
fn model_stem(name: &str) -> &str {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let file = file.strip_suffix(".mdl").unwrap_or(file);
    for prefix in ["v_", "p_", "w_"] {
        if let Some(rest) = file.strip_prefix(prefix) {
            return rest;
        }
    }
    file
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

/// The third-person model the spectated player was last seen holding.
static LAST_HELD_MODEL: Mutex<Option<String>> = Mutex::new(None);
static HELD_LOGS: AtomicI32 = AtomicI32::new(0);
/// Generous, because this only changes on a weapon or stance change, not per
/// frame -- and the whole point is to catch every one of them.
const MAX_HELD_LOGS: i32 = 200;

/// Logs the held third-person model whenever it changes.
///
/// DoD ships far more `p_` models than weapons because they encode stance as
/// well: `p_mg42bu` / `p_mg42bd` / `p_mg42pr` / `p_mg42sr`, and the 30cal set
/// runs `pr` / `r` / `sr`. Only "bu" and "bd" are known for certain (bipod up
/// and down, and they are the only two carrying a shoot sequence); the rest
/// are inferred -- "pr" looks like prone and "sr" like sprint, but that is a
/// reading of the filenames, not a fact.
///
/// A timestamped trail of the changes can be matched against what the player
/// was visibly doing, which settles it by observation rather than by guessing
/// at abbreviations.
pub static LOG_HELD_MODELS: AtomicBool = AtomicBool::new(false);

fn note_held_model(spectated: &ClEntityS) {
    if !LOG_HELD_MODELS.load(Ordering::Relaxed) {
        // Forget what was last seen, so switching this on mid-session reports
        // the current model straight away rather than waiting for the next
        // change -- which might never come if the player just stands there.
        *LAST_HELD_MODEL.lock().unwrap() = None;
        return;
    }
    let Some(studio) = engine::engine_studio() else { return };
    let held = unsafe { (studio.get_model_by_index)(spectated.curstate.weaponmodel) };
    if held.is_null() {
        return;
    }
    let name = unsafe { (*held).name_str() }.into_owned();

    let mut last = LAST_HELD_MODEL.lock().unwrap();
    if last.as_deref() == Some(name.as_str()) {
        return;
    }
    let previous = last.replace(name.clone());
    drop(last);

    if HELD_LOGS.fetch_add(1, Ordering::Relaxed) >= MAX_HELD_LOGS {
        return;
    }
    unsafe {
        crate::debug::report(&format!(
            "anim_fix: held model changed -- \"{name}\" (was {}) -- what was the player doing?",
            previous.as_deref().unwrap_or("<none>")
        ))
    };
}

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

/// Reads bipod state off the third-person model the spectated player is
/// holding.
///
/// Returns `None` for anything that is neither "bu" nor "bd", which is a real
/// and common case rather than an error: there are far more `p_` models than
/// weapons, because they also vary by stance. The MGs alone ship
/// `p_mg42bu` / `p_mg42bd` / `p_mg42pr` / `p_mg42sr`, and the Bren adds
/// `p_brenbr` / `p_brenpr` / `p_brensr` / `p_bren_l`. A player prone with an
/// MG is on one of those stance variants, so the deploy state is simply not
/// readable from the model name at that moment and the animation falls back to
/// whichever family was last known. Worth knowing before reading "deploy state
/// unknown" in a log as a failure.
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

/// Demo time of the last firing animation played, so the two independent
/// triggers cannot both play one shot.
static LAST_FIRE_PLAYED: AtomicU64 = AtomicU64::new(0);
/// The MG42's ~1200rpm puts 0.05s between rounds, so the window that keeps the
/// body-sequence and sound triggers from doubling up on a single shot has to
/// sit clear of that -- otherwise it would swallow real rounds of automatic
/// fire, which is the one case the sound trigger exists to catch.
const FIRE_DEDUP_SECONDS: f64 = 0.03;

/// Returns whether a firing animation should play now, or whether the other
/// trigger already played one for this same shot.
fn claim_fire(now: f64) -> bool {
    let last = f64::from_bits(LAST_FIRE_PLAYED.load(Ordering::Relaxed));
    // `now < last` means the clock went backwards -- a demo restarting -- so
    // let it through rather than blocking until it catches up again.
    if now >= last && now - last < FIRE_DEDUP_SECONDS {
        return false;
    }
    LAST_FIRE_PLAYED.store(now.to_bits(), Ordering::Relaxed);
    true
}

static CURRENT_SPECTATED: AtomicI32 = AtomicI32::new(-1);
static CURRENT_VIEWMODEL: AtomicPtr<ModelSPartial> = AtomicPtr::new(std::ptr::null_mut());
static CURRENT_DEPLOY_STATE: AtomicI32 = AtomicI32::new(-1);
/// Last bipod state actually read off a "bu"/"bd" model, carried across the
/// stance variants that do not encode one. Cleared on a player switch, since
/// it says nothing about the next person.
static LAST_KNOWN_DEPLOY_STATE: AtomicI32 = AtomicI32::new(-1);

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

/// Plays the firing animation from the weapon-fire *sound*, as a second
/// trigger behind the body-sequence one in `apply()`.
///
/// It exists for one case the body sequence cannot cover: a held trigger. The
/// server sets the player's body to `stand_mg_shoot` once and leaves it there
/// for the whole burst, so every round after the first has no sequence change
/// to detect, while the sound fires per round. Anything the body sequence
/// already caught is filtered out by `claim_fire`.
///
/// Called from `sound_fix`'s `EV_PlaySound` hook, on the engine thread, same as
/// `apply()`.
pub fn on_weapon_fired(entity_index: i32) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }

    // Whether the entity a fire sound names is ever the spectated player has
    // never actually been confirmed, and if it is not then this trigger is
    // inert and only the body-sequence one is doing any work. Log the first few
    // raw so a single session settles it.
    let spectated = CURRENT_SPECTATED.load(Ordering::Relaxed);
    if FIRE_LOGS.fetch_add(1, Ordering::Relaxed) < MAX_FIRE_LOGS {
        unsafe {
            crate::debug::report(&format!(
                "anim_fix: weapon-fire sound from entity {entity_index}, currently spectating {spectated} -- {}",
                if entity_index == spectated { "MATCH" } else { "ignored" }
            ))
        };
    }

    // Only the player actually being watched. Playing a viewmodel animation
    // because somebody else fired would be worse than missing the round.
    if entity_index < 0 || entity_index != spectated {
        return;
    }
    let viewmodel = CURRENT_VIEWMODEL.load(Ordering::Relaxed);
    if viewmodel.is_null() {
        return;
    }
    if !claim_fire(engine::client_time()) {
        return;
    }

    let state = i32_to_deploy_state(CURRENT_DEPLOY_STATE.load(Ordering::Relaxed));
    let sequence = animation_lookup_any(ATTACK_SEQUENCES, state, viewmodel);
    play_viewmodel_animation(sequence, "spectated player fired (sound)", state, viewmodel);
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

    // Before the mismatch filter, so stance changes are still recorded on
    // frames the filter drops.
    note_held_model(spectated);

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
        // Says nothing about the new player.
        LAST_KNOWN_DEPLOY_STATE.store(-1, Ordering::Relaxed);
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

    // The spectated player's own body animation. Replicated, so unlike almost
    // anything else about another player's weapon it survives into an HLTV
    // demo, and its label names both the action and the stance. Everything
    // below is read off it.
    let body_label = if spectated.model.is_null() {
        None
    } else {
        model_sequence_strings(spectated.model)
            .get(spectated.curstate.sequence.max(0) as usize)
            .cloned()
    };

    // Bipod state, preferring the body sequence because it carries the state in
    // every stance. The "bu"/"bd" model name is the fallback: it goes
    // unreadable whenever the player is on a stance variant that carries
    // neither marker (p_mg42pr.mdl, p_mg42sr.mdl), which is most of the time a
    // machine gunner actually matters. Falling back further to the last state
    // actually observed keeps the viewmodel in the right sequence family across
    // any remaining gap, rather than dropping to whichever family a bare lookup
    // happens to find first.
    let observed = deployable.and_then(|weapon| {
        body_label
            .as_deref()
            .and_then(deploy_state_from_body_sequence)
            .or_else(|| get_spectated_deploy_state(weapon, spectated))
    });
    let state = match observed {
        Some(seen) => {
            LAST_KNOWN_DEPLOY_STATE.store(deploy_state_to_i32(Some(seen)), Ordering::Relaxed);
            Some(seen)
        }
        // Only worth remembering for a weapon that has the two families at all.
        None if deployable.is_some() => i32_to_deploy_state(LAST_KNOWN_DEPLOY_STATE.load(Ordering::Relaxed)),
        None => None,
    };

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
        // Classify the action from the spectated player's body animation, not
        // the viewmodel's. Only on a *change* of sequence: the label persists
        // for as long as the animation runs, so acting on its mere presence
        // would restart the viewmodel animation every frame.
        let previous_sequence = PREVIOUS_SEQUENCE.load(Ordering::Relaxed);
        if previous_sequence != spectated.curstate.sequence {
            // Under the same switch as the held-model trail, because it answers
            // the same question and reads better next to it: the body label
            // names the stance outright ("prone_bar_reload"), where the `p_`
            // model name only abbreviates it.
            if LOG_HELD_MODELS.load(Ordering::Relaxed) {
                unsafe {
                    crate::debug::report(&format!(
                        "anim_fix: body sequence -> {} (index {})",
                        body_label.as_deref().unwrap_or("<unreadable>"),
                        spectated.curstate.sequence,
                    ))
                };
            }
            match body_label.as_deref().map(classify_body_sequence) {
                Some(BodyAction::Shoot) if claim_fire(engine::client_time()) => {
                    play_viewmodel_animation(
                        animation_lookup_any(ATTACK_SEQUENCES, state, viewmodel_model),
                        "spectated player fired",
                        state,
                        viewmodel_model,
                    );
                }
                Some(BodyAction::Reload) => {
                    play_viewmodel_animation(
                        animation_lookup_sequence("reload", state, viewmodel_model),
                        "spectated player reloaded",
                        state,
                        viewmodel_model,
                    );
                }
                _ => {}
            }
        }

        if viewmodel_changed {
            play_viewmodel_animation(animation_lookup_sequence("draw", state, viewmodel_model), "viewmodel changed", state, viewmodel_model);
        }
    }

    PREVIOUS_DEPLOY_STATE.store(deploy_state_to_i32(state), Ordering::Relaxed);
    PREVIOUS_SEQUENCE.store(spectated.curstate.sequence, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every label below is copied from a dump of the real
    /// `models/player/us-inf/us-inf.mdl` and `models/v_*.mdl` shipped with
    /// DoD 1.3, not invented -- these functions exist only to read that
    /// naming scheme, so made-up labels would test nothing.
    #[test]
    fn body_sequences_classify_by_action() {
        for label in [
            "stand_bolt_shoot",
            "crouch_rifle_shoot",
            "prone_mg_shoot",
            "bipod_bren_shoot",
            "sandbag_30cal_shoot",
            // Not gunfire, but still an attack, and the viewmodels have
            // "throw" and "slash1" for them.
            "stand_gren_shoot",
            "crouch_knife_shoot",
        ] {
            assert_eq!(classify_body_sequence(label), BodyAction::Shoot, "{label}");
        }

        for label in [
            "stand_garand_reload",
            "crouch_reload_webley",
            "prone_bar_reload",
            "bipod_mg42_reload",
            "stand_pschreck_zoomload",
        ] {
            assert_eq!(classify_body_sequence(label), BodyAction::Reload, "{label}");
        }

        for label in [
            // Aiming is the resting state between shots, and is what makes a
            // repeated shot show up as a sequence *change* at all.
            "stand_bolt_aim",
            "sprint_sten_aim",
            "bipod_mg_aim",
            "dod_idle1",
            "prone_forward",
            "die_headshot",
            // A rifle-butt swing, deliberately left alone: the viewmodels have
            // no matching sequence.
            "stand_rifle_swing",
        ] {
            assert_eq!(classify_body_sequence(label), BodyAction::Other, "{label}");
        }
    }

    #[test]
    fn body_sequences_carry_the_deploy_state_in_every_stance() {
        // The whole point of preferring this over the p_*bu/bd model name: a
        // prone or sprinting machine gunner still reports a state here.
        assert_eq!(deploy_state_from_body_sequence("prone_mg_shoot"), Some(DeployState::Up));
        assert_eq!(deploy_state_from_body_sequence("sprint_bren_aim"), Some(DeployState::Up));
        assert_eq!(deploy_state_from_body_sequence("stand_mg_aim"), Some(DeployState::Up));
        assert_eq!(deploy_state_from_body_sequence("crouch_bar_reload"), Some(DeployState::Up));

        assert_eq!(deploy_state_from_body_sequence("bipod_mg_shoot"), Some(DeployState::Down));
        assert_eq!(deploy_state_from_body_sequence("sandbag_bren_reload"), Some(DeployState::Down));

        // Sequences with no stance prefix say nothing either way, and must not
        // be read as "not deployed".
        assert_eq!(deploy_state_from_body_sequence("dod_idle1"), None);
        assert_eq!(deploy_state_from_body_sequence("hs_gogogo"), None);
    }

    #[test]
    fn model_stem_strips_all_three_prefixes() {
        assert_eq!(model_stem("models/v_98k.mdl"), "98k");
        assert_eq!(model_stem("models/p_mg42bd.mdl"), "mg42bd");
        assert_eq!(model_stem("models\\w_luger.mdl"), "luger");
        assert_eq!(model_stem("models/player/us-inf/us-inf.mdl"), "us-inf");
    }

    #[test]
    fn family_prefix_swaps_both_spellings() {
        assert_eq!(swap_family_prefix("upidle", DeployState::Down), "downidle");
        assert_eq!(swap_family_prefix("down_reload", DeployState::Up), "up_reload");
        // Already in the target family, and unfamilied labels, are untouched.
        assert_eq!(swap_family_prefix("upshoot", DeployState::Up), "upshoot");
        assert_eq!(swap_family_prefix("reload", DeployState::Down), "reload");
    }
}
