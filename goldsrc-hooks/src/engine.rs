//! Captures the engine's `cl_enginefuncs_s` pointer table -- the same table
//! GoldSrc hands to `client.dll` exactly once, at startup, via its exported
//! `Initialize(cl_enginefuncs_t*, int)` -- without needing HLAE's approach of
//! byte-signature-scanning `client.dll`'s data section for a specific engine
//! build. Reused for both fixes.
//!
//! Mechanism, in order:
//! 1. Patch `hw.dll`'s own Import Address Table so its calls to
//!    `KERNEL32!LoadLibraryA` come through us first -- the exact technique
//!    HLAE itself already uses (`AfxHookGoldSrc`'s `CAfxImportDllHook`) to
//!    catch the moment `client.dll` gets loaded.
//! 2. When that happens, patch `client.dll`'s *export table* entry for
//!    `Initialize` to point at our own wrapper, before returning control.
//!    The engine's very next step is `GetProcAddress(hClient, "Initialize")`
//!    followed immediately by calling it -- both PE/ABI-guaranteed steps we
//!    don't need to race against, we just need to have already rewritten
//!    what that lookup resolves to.
//! 3. Our `Initialize` wrapper records the real pointer, then calls through
//!    to the real `Initialize` so the game starts up completely normally.
//!
//! None of this depends on any DoD-specific (or even engine-build-specific)
//! byte pattern -- only on the PE format and the GoldSrc mod ABI, both of
//! which are permanently frozen.

use std::ffi::{c_char, c_void};
use std::sync::atomic::{AtomicPtr, Ordering};

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS};
use windows_sys::Win32::System::Threading::Sleep;

use crate::pe;

/// Partial mirror of GoldSrc's `event_api_s` (see `common/event_api.h` in the
/// Half-Life SDK). Only the fields up to and including `EV_PlaySound` are
/// declared -- we never touch anything past it, and C struct layout is
/// sequential, so the offset of `EV_PlaySound` only depends on what precedes
/// it (just `version`), not on anything declared after it here.
#[repr(C)]
pub struct EventApiPartial {
    pub version: i32,
    pub ev_play_sound: unsafe extern "C" fn(
        ent: i32,
        origin: *mut f32,
        channel: i32,
        sample: *const c_char,
        volume: f32,
        attenuation: f32,
        f_flags: i32,
        pitch: i32,
    ),
}

pub type WeaponAnimFn = unsafe extern "C" fn(sequence: i32, body: i32);
pub type GetGameDirectoryFn = unsafe extern "C" fn(sz_get_game_dir: *mut c_char);
pub type IsSpectateOnlyFn = unsafe extern "C" fn() -> i32;
pub type GetViewModelFn = unsafe extern "C" fn() -> *mut ClEntityS;
pub type GetEntityByIndexFn = unsafe extern "C" fn(index: i32) -> *mut ClEntityS;

/// A 3D vector, matching `vec3_t` (`float[3]`) everywhere it's embedded in a
/// GoldSrc SDK struct below.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Exact field-for-field mirror of `entity_state_s` (`common/entity_state.h`)
/// -- unlike `ClEngineFuncsPartial`'s uniform pointer-sized slots, this
/// struct mixes byte/short/int/float/vec3 fields with real padding between
/// them, so it's transcribed with real types rather than opaque slots:
/// `#[repr(C)]` reproduces MSVC's C struct layout rules exactly, so as long
/// as every field's type and order matches the header, the offsets of
/// `sequence` and `weaponmodel` (what we actually read) come out correct
/// automatically instead of needing hand-computed byte offsets.
#[repr(C)]
pub struct EntityStateS {
    pub entity_type: i32,
    pub number: i32,
    pub msg_time: f32,
    pub messagenum: i32,
    pub origin: Vec3,
    pub angles: Vec3,
    pub modelindex: i32,
    pub sequence: i32,
    pub frame: f32,
    pub colormap: i32,
    pub skin: i16,
    pub solid: i16,
    pub effects: i32,
    pub scale: f32,
    pub eflags: u8,
    pub rendermode: i32,
    pub renderamt: i32,
    pub rendercolor: [u8; 3], // color24 { r, g, b }
    pub renderfx: i32,
    pub movetype: i32,
    pub animtime: f32,
    pub framerate: f32,
    pub body: i32,
    pub controller: [u8; 4],
    pub blending: [u8; 4],
    pub velocity: Vec3,
    pub mins: Vec3,
    pub maxs: Vec3,
    pub aiment: i32,
    pub owner: i32,
    pub friction: f32,
    pub gravity: f32,
    pub team: i32,
    pub playerclass: i32,
    pub health: i32,
    pub spectator: i32, // qboolean
    pub weaponmodel: i32,
    pub gaitsequence: i32,
    pub basevelocity: Vec3,
    pub usehull: i32,
    pub oldbuttons: i32,
    pub onground: i32,
    pub istepleft: i32,
    pub fl_fall_velocity: f32,
    pub fov: f32,
    pub weaponanim: i32,
    pub startpos: Vec3,
    pub endpos: Vec3,
    pub impacttime: f32,
    pub starttime: f32,
    pub iuser1: i32,
    pub iuser2: i32,
    pub iuser3: i32,
    pub iuser4: i32,
    pub fuser1: f32,
    pub fuser2: f32,
    pub fuser3: f32,
    pub fuser4: f32,
    pub vuser1: Vec3,
    pub vuser2: Vec3,
    pub vuser3: Vec3,
    pub vuser4: Vec3,
}

const _: () = assert!(
    size_of::<EntityStateS>() == 340,
    "EntityStateS's layout doesn't match entity_state.h (expected 340 bytes)"
);

#[repr(C)]
struct PositionHistoryT {
    animtime: f32,
    origin: Vec3,
    angles: Vec3,
}

#[repr(C)]
struct MouthT {
    mouthopen: u8,
    sndcount: u8,
    sndavg: i32,
}

#[repr(C)]
struct LatchedVarsT {
    prevanimtime: f32,
    sequencetime: f32,
    prevseqblending: [u8; 2],
    prevorigin: Vec3,
    prevangles: Vec3,
    prevsequence: i32,
    prevframe: f32,
    prevcontroller: [u8; 4],
    prevblending: [u8; 2],
}

/// Partial mirror of `cl_entity_s` (`common/cl_entity.h`), transcribed field
/// for field up through `model` -- everything after that (`efrag`, `topnode`,
/// etc.) is never touched, so it's simply not declared.
#[repr(C)]
pub struct ClEntityS {
    pub index: i32,
    pub player: i32, // qboolean
    pub baseline: EntityStateS,
    pub prevstate: EntityStateS,
    pub curstate: EntityStateS,
    current_position: i32,
    ph: [PositionHistoryT; 64],
    mouth: MouthT,
    latched: LatchedVarsT,
    lastmove: f32,
    origin: Vec3,
    angles: Vec3,
    attachment: [Vec3; 4],
    trivial_accept: i32,
    pub model: *mut ModelSPartial,
}

/// Partial mirror of `model_s` (`common/com_model.h`) -- `name` is its first
/// field, so this is all we need to identify a model by filename substring.
#[repr(C)]
pub struct ModelSPartial {
    pub name: [u8; 64], // MAX_MODEL_NAME
}

impl ModelSPartial {
    pub fn name_str(&self) -> std::borrow::Cow<'_, str> {
        let end = self.name.iter().position(|&b| b == 0).unwrap_or(self.name.len());
        String::from_utf8_lossy(&self.name[..end])
    }
}

/// Exact field-for-field mirror of the studio model file header
/// (`studiohdr_t` in `engine/studio.h`) up through `seqindex` -- confirmed
/// against real DoD 1.3 `.mdl` files during the R&D phase (numseq at byte
/// offset 164, seqindex at 168).
#[repr(C)]
pub struct StudioHdrPartial {
    pub id: i32,
    pub version: i32,
    pub name: [u8; 64],
    pub length: i32,
    pub eyeposition: Vec3,
    pub min: Vec3,
    pub max: Vec3,
    pub bbmin: Vec3,
    pub bbmax: Vec3,
    pub flags: i32,
    pub numbones: i32,
    pub boneindex: i32,
    pub numbonecontrollers: i32,
    pub bonecontrollerindex: i32,
    pub numhitboxes: i32,
    pub hitboxindex: i32,
    pub numseq: i32,
    pub seqindex: i32,
}

const _: () = assert!(
    std::mem::offset_of!(StudioHdrPartial, numseq) == 164
        && std::mem::offset_of!(StudioHdrPartial, seqindex) == 168,
    "StudioHdrPartial's layout doesn't match studio.h / the confirmed .mdl offsets"
);

/// Exact field-for-field mirror of `mstudioseqdesc_t` (`engine/studio.h`) up
/// through `label` -- 176 bytes total per entry, `label` is the first 32.
#[repr(C)]
pub struct StudioSeqDescPartial {
    pub label: [u8; 32],
    _rest: [u8; 176 - 32],
}

const _: () = assert!(size_of::<StudioSeqDescPartial>() == 176);

impl StudioSeqDescPartial {
    pub fn label_str(&self) -> std::borrow::Cow<'_, str> {
        let end = self.label.iter().position(|&b| b == 0).unwrap_or(self.label.len());
        String::from_utf8_lossy(&self.label[..end])
    }
}

/// Partial mirror of `engine_studio_api_s` (`common/r_studioint.h`) up
/// through `GetModelByIndex` -- the only two functions this crate calls.
#[repr(C)]
pub struct EngineStudioApiPartial {
    _slots_before: [*mut c_void; 3], // Mem_Calloc, Cache_Check, LoadCacheFile
    _mod_for_name: *mut c_void,
    pub mod_extradata: unsafe extern "C" fn(model: *mut ModelSPartial) -> *mut c_void,
    pub get_model_by_index: unsafe extern "C" fn(index: i32) -> *mut ModelSPartial,
}

/// Partial, offset-accurate mirror of `cl_enginefuncs_s` (see
/// `advancedfx/halflife`'s `engine/APIProxy.h`, which matches the vanilla
/// Half-Life SDK layout DoD 1.3 was built against). Every field up through
/// `IsSpectateOnly` is present, in order, so the four we actually use
/// (`pfn_weapon_anim`, `pfn_get_game_directory`, `p_event_api`,
/// `is_spectate_only`) land at the correct byte offsets; everything else is
/// kept as an opaque, untyped slot purely to hold the layout together.
#[repr(C)]
pub struct ClEngineFuncsPartial {
    _slots_before_viewmodel: [*mut c_void; 52], // pfnSPR_Load .. GetLocalPlayer
    pub get_view_model: GetViewModelFn,
    pub get_entity_by_index: GetEntityByIndexFn,
    _slots_before_weapon_anim: [*mut c_void; 12], // GetClientTime .. pfnPlaybackEvent
    pub pfn_weapon_anim: WeaponAnimFn,
    _slots_between: [*mut c_void; 4], // pfnRandomFloat, pfnRandomLong, pfnHookEvent, Con_IsVisible
    pub pfn_get_game_directory: GetGameDirectoryFn,
    _slots_after_gamedir: [*mut c_void; 12], // pfnGetCvarPointer .. pEfxAPI
    pub p_event_api: *mut EventApiPartial,
    _slots_after_event_api: [*mut c_void; 3], // pDemoAPI, pNetAPI, pVoiceTweak
    pub is_spectate_only: IsSpectateOnlyFn,
}

// Compile-time cross-check against the field count transcribed from
// APIProxy.h (pfnSPR_Load..IsSpectateOnly inclusive = 89 pointer-sized slots
// on a 32-bit build, i.e. 356 bytes). If this ever fails, a slot count above
// drifted from the reference header.
const _: () = assert!(
    size_of::<ClEngineFuncsPartial>() == 89 * size_of::<usize>(),
    "ClEngineFuncsPartial's layout doesn't match APIProxy.h's field count (expected 89 pointer-sized slots)"
);

static ENGFUNCS: AtomicPtr<ClEngineFuncsPartial> = AtomicPtr::new(std::ptr::null_mut());
static ENGINE_STUDIO: AtomicPtr<EngineStudioApiPartial> = AtomicPtr::new(std::ptr::null_mut());

/// Returns the captured engine function table, once `client.dll` has
/// finished loading and calling its own `Initialize`. `None` before that.
pub fn engfuncs() -> Option<&'static ClEngineFuncsPartial> {
    let ptr = ENGFUNCS.load(Ordering::Acquire);
    if ptr.is_null() {
        None
    } else {
        // Safety: only ever set once, from `hook_initialize`, to a pointer
        // the engine itself handed us and guarantees stays valid for the
        // life of the client.dll instance.
        Some(unsafe { &*ptr })
    }
}

/// Returns the captured studio-model engine interface, once `client.dll` has
/// called its own exported `HUD_GetStudioModelInterface`. `None` before that.
pub fn engine_studio() -> Option<&'static EngineStudioApiPartial> {
    let ptr = ENGINE_STUDIO.load(Ordering::Acquire);
    if ptr.is_null() {
        None
    } else {
        // Safety: same guarantee as `engfuncs()` above.
        Some(unsafe { &*ptr })
    }
}

type InitializeFn = unsafe extern "C" fn(*mut ClEngineFuncsPartial, i32) -> i32;
static REAL_INITIALIZE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

type GetStudioModelInterfaceFn =
    unsafe extern "C" fn(i32, *mut *mut c_void, *mut EngineStudioApiPartial) -> i32;
static REAL_GET_STUDIO_MODEL_INTERFACE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

unsafe extern "C" fn hook_get_studio_model_interface(
    version: i32,
    ppinterface: *mut *mut c_void,
    pstudio: *mut EngineStudioApiPartial,
) -> i32 {
    ENGINE_STUDIO.store(pstudio, Ordering::Release);

    let real = REAL_GET_STUDIO_MODEL_INTERFACE.load(Ordering::Acquire);
    if real.is_null() {
        return 0;
    }
    let real: GetStudioModelInterfaceFn = unsafe { std::mem::transmute(real) };
    unsafe { real(version, ppinterface, pstudio) }
}

type HudFrameFn = unsafe extern "C" fn(f64);
static REAL_HUD_FRAME: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// Registers `callback` to run once per client frame, driven by hooking
/// `client.dll`'s exported `HUD_Frame` (guaranteed to fire every frame,
/// including while passively watching a demo -- the same "runs every frame
/// regardless of anything else" spot HLAE's own `filming.cpp` calls its
/// per-frame fixes from, which we don't have access to from outside HLAE).
static PER_FRAME_CALLBACK: std::sync::OnceLock<fn()> = std::sync::OnceLock::new();

pub fn set_per_frame_callback(callback: fn()) {
    let _ = PER_FRAME_CALLBACK.set(callback);
}

unsafe extern "C" fn hook_hud_frame(time: f64) {
    let real = REAL_HUD_FRAME.load(Ordering::Acquire);
    if !real.is_null() {
        let real: HudFrameFn = unsafe { std::mem::transmute(real) };
        unsafe { real(time) };
    }

    if let Some(callback) = PER_FRAME_CALLBACK.get() {
        callback();
    }
}

type LoadLibraryAFn = unsafe extern "system" fn(*const u8) -> HMODULE;
static REAL_LOAD_LIBRARY_A: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

unsafe extern "C" fn hook_initialize(engfuncs: *mut ClEngineFuncsPartial, version: i32) -> i32 {
    ENGFUNCS.store(engfuncs, Ordering::Release);

    let real = REAL_INITIALIZE.load(Ordering::Acquire);
    if real.is_null() {
        // Should never happen: we only ever install this hook after
        // resolving the real address. Fail closed rather than crash the
        // game's own client.dll load.
        return 0;
    }
    let real: InitializeFn = unsafe { std::mem::transmute(real) };
    unsafe { real(engfuncs, version) }
}

unsafe fn c_str_eq_ignore_ascii_case(ptr: *const u8, other: &str) -> bool {
    unsafe {
        if ptr.is_null() {
            return false;
        }
        std::ffi::CStr::from_ptr(ptr as *const i8)
            .to_string_lossy()
            .eq_ignore_ascii_case(other)
    }
}

unsafe extern "system" fn hook_load_library_a(lp_lib_file_name: *const u8) -> HMODULE {
    let real = REAL_LOAD_LIBRARY_A.load(Ordering::Acquire);
    let real: LoadLibraryAFn = unsafe { std::mem::transmute(real) };
    let result = unsafe { real(lp_lib_file_name) };

    if !result.is_null() && unsafe { c_str_eq_ignore_ascii_case(lp_lib_file_name, "client.dll") } {
        unsafe { hook_client_initialize(result as *mut u8) };
    }

    result
}

unsafe fn hook_client_initialize(client_dll_base: *mut u8) {
    if let Some(real_init) = unsafe { pe::find_export(client_dll_base, "Initialize") } {
        REAL_INITIALIZE.store(real_init, Ordering::Release);
        // Patch client.dll's OWN export-table entry so the engine's imminent
        // GetProcAddress(hClient, "Initialize") resolves to our wrapper
        // instead.
        unsafe { pe::patch_export(client_dll_base, "Initialize", hook_initialize as *mut c_void) };
    } else {
        unsafe { crate::debug::report("goldsrc-hooks: client.dll has no 'Initialize' export -- unexpected, sound/animation fixes can't activate") };
    }

    // Same technique, for the *studio model* interface exchange the
    // animation fix needs (engine_studio_api_s, via a completely separate
    // export -- see EngineStudioApiPartial's docs).
    if let Some(real_get_studio) = unsafe { pe::find_export(client_dll_base, "HUD_GetStudioModelInterface") } {
        REAL_GET_STUDIO_MODEL_INTERFACE.store(real_get_studio, Ordering::Release);
        unsafe {
            pe::patch_export(
                client_dll_base,
                "HUD_GetStudioModelInterface",
                hook_get_studio_model_interface as *mut c_void,
            )
        };
    } else {
        unsafe { crate::debug::report("goldsrc-hooks: client.dll has no 'HUD_GetStudioModelInterface' export -- animation fix can't activate") };
    }

    if let Some(real_hud_frame) = unsafe { pe::find_export(client_dll_base, "HUD_Frame") } {
        REAL_HUD_FRAME.store(real_hud_frame, Ordering::Release);
        unsafe { pe::patch_export(client_dll_base, "HUD_Frame", hook_hud_frame as *mut c_void) };
    } else {
        unsafe { crate::debug::report("goldsrc-hooks: client.dll has no 'HUD_Frame' export -- per-frame fixes (animation) can't activate") };
    }
}

/// Finds `module_name` (e.g. "hw.dll"), patches its IAT so its own calls to
/// `KERNEL32!LoadLibraryA` route through us, and returns whether the patch
/// was applied. Polls briefly since the module may not be loaded yet at the
/// moment we're injected.
fn hook_loadlibrary_in(module_name: &str, timeout_ms: u32) -> bool {
    let module_name_c = format!("{module_name}\0");
    let mut waited = 0u32;
    let module = loop {
        let h = unsafe { GetModuleHandleA(module_name_c.as_ptr()) };
        if !h.is_null() {
            break h;
        }
        if waited >= timeout_ms {
            return false;
        }
        unsafe { Sleep(25) };
        waited += 25;
    };

    let Some(slot) =
        (unsafe { pe::find_iat_slot(module as *mut u8, "KERNEL32.dll", "LoadLibraryA") })
    else {
        return false;
    };

    unsafe {
        REAL_LOAD_LIBRARY_A.store(*slot, Ordering::Release);

        let mut old_protect: PAGE_PROTECTION_FLAGS = 0;
        let ok = VirtualProtect(
            slot as *mut c_void,
            size_of::<*mut c_void>(),
            PAGE_EXECUTE_READWRITE,
            &mut old_protect,
        );
        if ok == 0 {
            return false;
        }
        *slot = hook_load_library_a as *mut c_void;
        VirtualProtect(
            slot as *mut c_void,
            size_of::<*mut c_void>(),
            old_protect,
            &mut old_protect,
        );
    }

    true
}

/// Installs the LoadLibraryA -> Initialize interception chain. Call once,
/// from a background thread (never do this work directly in `DllMain` --
/// see `lib.rs`).
pub fn install() {
    // GoldSrc's OpenGL renderer module is virtually always what's actually
    // loaded in any modern setup (HLAE's own recording pipeline requires
    // it); `sw.dll` (the old software renderer) is not handled here.
    if !hook_loadlibrary_in("hw.dll", 15_000) {
        unsafe { crate::debug::report("goldsrc-hooks: could not hook hw.dll's LoadLibraryA import (module never appeared, or the IAT slot wasn't found) -- sound/animation fixes are inactive this session.") };
    }
}
