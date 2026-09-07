# Day of Defeat 1.3 `client.dll` Internals

Static reverse-engineering notes for DoD 1.3's client library, produced to unblock
[issue #204](https://github.com/ccoventry/dod-tools/issues/204) (the `goldsrc-hooks`
companion DLL could not capture `pEngfuncs`). Everything below was derived offline from
the shipped binary — no running game, no debugger, no patched engine — and each claim
names the evidence it rests on.

**Subject analysed:** `dod/cl_dlls/client.dll` from the *Half-Life - Pre-Anniversary for
Movies* depot (the SteamLegacy/beta branch this project captures against), 977,816 bytes,
32-bit PE, `ImageBase 0x1900000`, four sections (`.text 0x1901000`, `.rdata 0x19a4000`,
`.data 0x19bf000`, `.reloc 0x1a9f000`), 125 named exports.

---

## 1. The engine does not resolve `client.dll`'s exports by name

This is the finding that explains months of confusing symptoms, and it inverts the
documented GoldSrc mod ABI.

`hw.dll` loads the client library and then asks for **one** export:

```asm
; hw.dll @ 0x1d61cb9  (ImageBase 0x1d00000)
call dword ptr [0x1e1c24c]   ; KERNEL32!LoadLibraryA  -> eax = hClient
mov  ecx, [ebp+0xc]
mov  edi, eax
test edi, edi
mov  [ecx], edi
je   <fail>
push 0x1e4d428               ; the string "F"
push edi                     ; hClient
call dword ptr [0x1e1c114]   ; KERNEL32!GetProcAddress
test eax, eax
je   0x1d61cf1               ; -> FreeLibrary, return 0
mov  edx, [ebp+0x10]
push edx
call eax                     ; F(table)
add  esp, 4
ret
```

If `F` is present the engine calls it and is **done**. If `F` is absent it calls
`FreeLibrary` and fails — there is no per-name fallback on this path. DoD 1.3's
`client.dll` exports `F`, so `Initialize`, `HUD_Frame`,
`HUD_GetStudioModelInterface` and the other classic names are exported but **never
looked up**.

That is the complete explanation for the failure recorded in #204: patching
`client.dll`'s export table was mechanically perfect (the real Win32 `GetProcAddress`
read our patched value straight back) and yet nothing ever called through it, because
the engine never asks for those names.

**Independent corroboration.** Xash3D's GoldSrc-compatible client loader implements the
same convention and names it explicitly
([`FWGS/xash3d-fwgs`, `engine/client/dll_int/cl_game.c`](https://github.com/FWGS/xash3d-fwgs/blob/master/engine/client/dll_int/cl_game.c)):

```c
else if(( GetClientAPI = COM_GetProcAddress( clgame.hInstance, "F" )) != NULL )
{
        Con_Reportf( "%s: found single callback export (secured client dlls)\n", __func__ );
        CL_GetSecuredClientAPI( GetClientAPI );
}
```

"Secured client dlls" is the community term for this shape. It also matches HLAE's own
dead, commented-out detection code, which treats a single-letter `F` export as the
marker of a VAC-"secure" build.

> Practical consequence for this project: **the `-insecure` launch flag is irrelevant
> here.** It controls VAC on a *server*; whether `client.dll` was built with the `F`
> entry point is a compile-time property of the shipped file. `-insecure` was already
> present in every test in #204 and changed nothing, which is consistent.

---

## 2. `F` and the `cldll_func_t` table

`F` is `__cdecl`, takes one pointer, returns void. Disassembling it
(`client.dll` RVA `0x2a700`):

```asm
mov  eax, [esp+0xb0]     ; the caller's buffer
mov  esi, [eax]          ; buffer->[0] : pointer to 29 engine-supplied dwords
mov  edi, 0x1a08038
mov  ecx, 0x1d           ; 29
rep  movsd               ; ...copied into a client.dll global
mov  ecx, [eax+8]
mov  [0x1a08030], ecx    ; buffer->[8] : the internal dispatch object
...
lea  esi, [esp+8]        ; 43 function pointers built on the stack
mov  edi, eax            ; destination = the caller's buffer, from offset 0
mov  ecx, 0x2b           ; 43
rep  movsd
ret
```

So `F` **overwrites the buffer it was given** with exactly 43 function pointers, starting
at offset 0. Resolving all 43 addresses back through the export table yields a perfect,
in-order match with Xash3D's `cldll_func_src_t` — which is how the slot indices below are
known to be right rather than guessed:

| Slot | Export | Slot | Export |
| --- | --- | --- | --- |
| 0 | `Initialize` | 22 | `HUD_DrawNormalTriangles` |
| 1 | `HUD_Init` | 23 | `HUD_DrawTransparentTriangles` |
| 2 | `HUD_VidInit` | 24 | `HUD_StudioEvent` |
| 3 | `HUD_Redraw` | 25 | `HUD_PostRunCmd` |
| 4 | `HUD_UpdateClientData` | 26 | `HUD_Shutdown` |
| 5 | `HUD_Reset` | 27 | `HUD_TxferLocalOverrides` |
| 6 | `HUD_PlayerMove` | 28 | `HUD_ProcessPlayerState` |
| 7 | `HUD_PlayerMoveInit` | 29 | `HUD_TxferPredictionData` |
| 8 | `HUD_PlayerMoveTexture` | 30 | `Demo_ReadBuffer` |
| 9 | `IN_ActivateMouse` | 31 | `HUD_ConnectionlessPacket` |
| 10 | `IN_DeactivateMouse` | 32 | `HUD_GetHullBounds` |
| 11 | `IN_MouseEvent` | **33** | **`HUD_Frame`** |
| 12 | `IN_ClearStates` | 34 | `HUD_Key_Event` |
| 13 | `IN_Accumulate` | 35 | `HUD_TempEntUpdate` |
| 14 | `CL_CreateMove` | 36 | `HUD_GetUserEntity` |
| 15 | `CL_IsThirdPerson` | 37 | `HUD_VoiceStatus` |
| 16 | `CL_CameraOffset` | 38 | `HUD_DirectorMessage` |
| 17 | `KB_Find` | **39** | **`HUD_GetStudioModelInterface`** |
| 18 | `CAM_Think` | 40 | `HUD_ChatInputPosition` |
| 19 | `V_CalcRefdef` | 41 | `HUD_GetPlayerTeam` |
| 20 | `HUD_AddEntity` | 42 | `ClientFactory` (not a named export) |
| 21 | `HUD_CreateEntities` | | |

The 43 exported entries are thin thunks: each forwards to the real implementation behind
`[0x1a08030] + slot*4`, converting the normal argument convention into a by-pointer one
(Xash calls these two shapes `cldll_func_src_t` and `cldll_func_dst_t`). For hooking
purposes this is irrelevant — **the slots in the table `F` returns take ordinary
arguments**, e.g. `Initialize(cl_enginefunc_t*, int)`.

### How `goldsrc-hooks` uses this

`goldsrc-hooks` hooks **`hw.dll`'s IAT entry for `KERNEL32!GetProcAddress`** — the exact
call site above — next to the `LoadLibraryA` entry it already hooked to spot `client.dll`
loading. Both slots live in the same import table (`0x1e1c114` and `0x1e1c24c`), and the
`LoadLibraryA` half was already proven reliable, so this is an extension of working
machinery rather than a new mechanism. It is also the technique HLAE's own
`CAfxImportDllHook` uses, and it affects only `hw.dll`'s calls rather than the whole
process.

When the engine asks `client.dll` for `"F"`, we return our own wrapper: it calls the real
`F` so the table gets filled exactly as normal, then swaps slots 0, 33 and 39 for
trampolines before the engine ever reads them. The trampolines receive what we need as
plain arguments — `Initialize` hands us `pEnginefuncs`, `HUD_GetStudioModelInterface`
hands us `pstudio`, and `HUD_Frame` is a genuine per-frame tick (replacing the ~60Hz
timer thread that stood in for it). Lookups of `"Initialize"`, `"HUD_Frame"` and
`"HUD_GetStudioModelInterface"` are handled the same way, so a non-secured `client.dll`
works through the identical code path with no byte patterns and no hardcoded offsets.

---

## 3. Why HLAE's `pEngfuncs` signature scan cannot work on DoD

HLAE's `hl_addresses.cpp` finds `pEngfuncs` by locating the string `"ScreenFade"` in
`client.dll`, finding the code that references it, and reading a hardcoded operand
nearby. That string **does not exist anywhere in DoD's `client.dll`** — confirmed both
in-process (searched every section, exact and substring) and by offline string
extraction from the file.

The reason is now clear, and it is not a build quirk: in the stock Half-Life SDK the
client hooks the `ScreenFade` user message itself (`HOOK_MESSAGE(ScreenFade)`), so the
literal is present. **DoD's client never hooks `ScreenFade`** — it is absent from the
71 messages DoD registers (§5) and is left to the engine. HLAE's anchor is therefore
structurally unavailable in DoD, not merely missing from one build.

## 4. `gEngfuncs` — the client's own copy of the engine table

Useful as a cross-check even though the hook above makes it unnecessary. The exported
`Initialize` thunk copies the engine's table into a `client.dll` global before
dispatching:

```asm
; client.dll Initialize, RVA 0x2a3b0
mov  ecx, 0x87           ; 135 dwords = 540 bytes = sizeof(cl_enginefunc_t)
mov  esi, [esp+0x310]    ; arg1 = pEnginefuncs
mov  edi, 0x1a76388      ; gEngfuncs
rep  movsd
...
cmp  eax, 7              ; CLDLL_INTERFACE_VERSION
```

`gEngfuncs` is therefore at **RVA `0x176388`** (`.data`) in this build, and the struct
this build expects is **135 pointer-sized fields**. Note it is *the struct itself*, not a
pointer to one, and it is all zeroes until `Initialize` runs — reading it earlier yields
a table of null function pointers, so anything derived from it must be validated before
use.

This address is what makes §5 possible, and its correctness is confirmed by the fact that
decoding calls through it produces 111 real, correctly-spelled DoD cvar names.

---

## 5. Extracted client API surface

With `gEngfuncs` known, every call of the form `call dword ptr [gEngfuncs + idx*4]` can
be found in `.text` and its string argument recovered, giving an authoritative inventory
of what DoD 1.3's client registers. Indices are `cl_enginefunc_t` declaration order.

### Console commands — `pfnAddCommand` (index 17), 107 registered

Beyond the expected `+`/`-` movement and `slot0..slot10` bindings:

```
cancelselect  centerview  credits  firstperson  force_centerview  hud_saytext  impulse
invnext  invprev  joyadvancedupdate  prone  snapto  spec_autodirector  spec_decal
spec_drawcone  spec_drawnames  spec_drawstatus  spec_help  spec_menu  spec_mode
thirdperson  togglescores  voice_showbanned
```

DoD-specific movement verbs worth knowing exist: `+prone`-equivalent `prone`, `+special`,
`+alt1`, `+break`, `+map` / `+mapzoom` (overview), `+commandmenu`.

### Client cvars — `pfnRegisterVariable` (index 14), 111 registered

Grouped by what they are likely to be worth to this project:

- **Spectator / HLTV:** `spec_pip`, `spec_scoreboard`, and the `_internal` companions
  `spec_autodirector_internal`, `spec_drawcone_internal`, `spec_drawnames_internal`,
  `spec_drawstatus_internal`. The `spec_*` console commands above are the user-facing
  front ends; the `_internal` cvars hold the state.
- **Granular HUD suppression:** `cl_hud_ammo`, `cl_hud_health`, `cl_hud_msgs`,
  `cl_hud_objectives`, `cl_hud_objtimer`, `cl_hud_reinforcements`, plus `hud_draw`,
  `hud_centerid`, `hud_deathnotice_time`, `hud_fastswitch`, `hud_takesshots`,
  `cl_hudfont`, `cl_identiconmode`. Finer-grained than the all-or-nothing `hud_draw 0`
  the pipeline currently relies on.
- **Cinematic / visual:** `cl_fog`, `cl_fog_density`, `cl_fog_start`, `cl_fog_end`,
  `cl_fog_red`, `cl_fog_green`, `cl_fog_blue`, `cl_particlefx`, `cl_dynamiclights`,
  `cl_bulletejects`, `cl_corpsestay`, `cl_numshotrubble`, `max_rubble`,
  `cl_drawmodels`, `cl_drawplayermodels`, `cl_weatherdis`, `cl_cutscenes`,
  `cl_xhair_style`, `cl_dynamic_xhair`.
- **View / camera:** `cl_bob`, `cl_bobcycle`, `cl_bobup`, `cl_vsmoothing`,
  `cl_chasedist`, `cl_waterdist`, `scr_ofsy`, `scr_ofsz`, `v_centermove`,
  `v_centerspeed`, and the `cam_*` / `c_min*` / `c_max*` third-person camera family.
- **Overview map:** `_cl_minimap`, `_cl_minimapzoom`, `cl_dmsmallmap`,
  `cl_dmshowplayers`, `cl_dmshowobjects`, `cl_dmshowflags`, `cl_dmshowgrenades`,
  `cl_dmshowmarkers`.

The remainder are stock Half-Life input/joystick/mouse cvars (`m_*`, `joy*`,
`cl_*speed`, `sensitivity`, `lookspring`, `lookstrafe`, `zoom_sensitivity_ratio`).

`spec_decal` (a command, not a cvar) is worth flagging against the decal R&D in
`goldsrc_dod_quirks.md` — it is a spectator-side decal action DoD registers itself,
independent of `r_decals`.

### Server commands the client sends — `pfnServerCmd` (index 19)

```
VModEnable %d          drc_spray %.2f %.2f %.2f %i    follow "%s"
follownext %i          ignoremsg %i                   maim %i
specmode %i            sprone
```

`follow "%s"`, `follownext`, and `specmode %i` are how DoD's own spectator UI drives
target selection — directly relevant to any future work that wants to steer an HLTV
demo's spectated player.

### Console commands the client injects into itself — `pfnClientCmd` (index 20)

```
+attack;wait;-attack   cl_lw 1        cl_pitchdown 89   cl_pitchup 89
crosshair 0            r_drawentities 1   vguicancel    voice_modenable 1
```

Worth noting for `native::patch::cfg_scan`: **the client itself writes `crosshair 0`,
`r_drawentities 1`, `cl_pitchup/down 89` and `cl_lw 1` at runtime**, so a user config
setting any of those can be silently reverted by the game regardless of what the pipeline
does.

### User messages — `pfnHookUserMsg` (index 18), 71 registered

```
AllowSpec  AmmoPickup  AmmoShort  AmmoX  BloodPuff  CameraView  CancelProg  CapMsg
ClCorpse  ClanTimer  ClientAreas  CurMarker  CurWeapon  DeathMsg  Frags  GameRules
GameTitle  HLTV  HandSignal  Health  HideWeapon  HudText  InitHUD  InitObj  ItemPickup
Logo  MOTD  MapMarker  ObjScore  Object  PClass  PShoot  PStatus  PTeam  PlayersIn
ProgUpdate  RandomPC  ReloadDone  ReqState  ResetHUD  ResetSens  RoundState  SayText
Scope  ScoreInfo  ScoreInfoLong  ScoreShort  ServerName  SetFOV  SetObj  ShowMenu
Spectator  StartProg  StartProgF  StatusIcon  StatusValue  TeamNames  TeamScore  TextMsg
TimeLeft  TimerStatus  Train  UseSound  VGUIMenu  VoiceMask  WaveStatus  WaveTime
WeapPickup  WeaponList  WideScreen  YouDied
```

This is the authoritative set for the `dod` crate's parser, and it settles two things:

1. **`dod::UserMessage::new` covers 61 of the 71.** Unhandled: `AllowSpec`, `GameTitle`,
   `ItemPickup`, `Logo`, `ProgUpdate`, `RandomPC`, `StatusIcon`, `TeamNames`, `Train`,
   `WideScreen`. This is *not* currently a defect — `analysis::is_relevant_message`
   filters to a curated subset first and none of the ten are in it, and the call site
   swallows parse failures with `if let Ok`. It is a completeness note for anyone
   extending the analyzer, not a bug to chase.
2. **`ScreenFade`, `ScreenShake` and `Weather` are parsed by the `dod` crate but are not
   in this list** — they are engine-level messages, not client-hooked ones. That is the
   same fact that breaks HLAE's scan anchor (§3), arrived at from the other direction.

---

## 6. Structure: the binary is not obfuscated

Worth stating plainly, because it changes how expensive any future work here is. Despite
being a "secured" build, `client.dll` is an ordinary release binary:

- **Full MSVC RTTI is present** — 209 type descriptors, giving real C++ class names.
- **2,996 printable strings**, including every sound path, sprite name, VGUI resource
  file and localisation token.
- No packing, no encrypted sections, no anti-debug, no import obfuscation.

The `F` export is a calling-convention difference, not a protection measure. Everything in
this document was recovered with ~200 lines of Python; a proper Ghidra pass would go
considerably further.

The RTTI names map the client's architecture directly. Selected non-VGUI classes:

- **Weapons** (each with a matching `*AmmoClip`): `C30CAL`, `CBAR`, `CBREN`, `CBazooka`,
  `CCOLT`, `CENFIELD`, `CFG42`, `CGarand`, `CGreaseGun`, `CM1Carbine`, `CMG34`, `CMG42`,
  `CMP40`, `CMP44`, `CPIAT`, `CPistol`, `CPschreck`, `CSPRING`, `CSTEN`, `CScopedKar`,
  `CThompson`, `CWEBLEY`; melee `CAmerKnife`, `CGerKnife`, `CSpade`, `CMeleeWeapon`;
  thrown `CHandGrenade(Ex)`, `CStickGrenade(Ex)`, `CDoDGrenade(Ex)`; and the base classes
  `CBipodWeapon` (the MG deploy behaviour `anim_fix` cares about) and `CRocketWeapon`.
- **HUD:** `CHudAmmo`, `CHudBase`, `CHudDeathNotice`, `CHudDoDCommon`,
  `CHudDoDCrossHair`, `CHudDoDMap`, `CMortarHud`, `CObjectiveIcons`, `CHudSpectator`.
- **Spectator / viewport:** `CDoDSpectatorGUI`, `CSpectatorGUI`, `ISpectatorInterface`,
  `DoDViewport`, `IDoDViewPort`, `TeamFortressViewport`, `CSpecHelpWindow`.
- **Rendering:** `CStudioModelRenderer`, `CGameStudioModelRenderer` (the studio renderer
  `anim_fix` works alongside), `CWeatherManager`, `CDoDRainDrop`, `CDoDSnowFlake`,
  `CDoDRocketTrail`, `CDoDDirtExploDust`, `CParticleShooter`, `TriangleWallPuff`.

Entity classnames are present in full: 76 `weapon_*` / `ammo_*` names in `client.dll`,
and `dod/dlls/dod.dll` (the server library, same techniques apply) carries those plus the
stock Half-Life leftovers (`weapon_crossbow`, `weapon_snark`, `weapon_tripmine`, …) that
DoD never uses.

### The unmapped weapon IDs — settled, and not worth chasing

`dod::Weapon` leaves IDs 15, 16, 33, 34 and 41 unmapped, with source comments guessing at
grenades. Two attempts to resolve them statically did **not** pan out, recorded so nobody
repeats them:

- The 42 `d_*` death-notice sprite names exist in `client.dll`, but there is **no array of
  pointers to them** and only two are referenced from `.text` — the death-notice icon is
  resolved by name at runtime, not through a static ID-indexed table.
- In `dod.dll`, `weapon_garand` and friends appear only once each in `.text`, inside a
  flat precache list — not next to an `ItemInfo` initialiser, so the HL SDK's
  `p->iId` field-offset trick finds nothing.

Rather than escalate to Ghidra, the question was answered empirically.
`analysis/examples/weapon_id_probe.rs` tallies the raw `DeathMsg` weapon byte across a
folder of demos and flags any ID the enum doesn't map. Across the full local library —
**447 demos, 148,109 `DeathMsg` records** — only 24 distinct IDs occur, and **none of
15, 16, 33, 34 or 41 appears even once**. The gaps are not a defect worth reverse
engineering.

The same run is a useful profile of what competitive DoD actually shoots with:

| id | weapon | share | | id | weapon | share |
| --- | --- | --- | --- | --- | --- | --- |
| 10 | K98 | 21.6% | | 13 | Mk2Grenade | 7.0% |
| 5 | Garand | 15.0% | | 14 | StickGrenade | 6.4% |
| 11 | Bar | 15.0% | | 8 | Stg44 | 6.0% |
| 6 | ScopedK98 | 8.2% | | 24 | K43 | 3.8% |
| 9 | Springfield | 8.0% | | 12 | Mp40 | 3.0% |

The remaining 14 IDs are all under 1.5% combined (`M1911`, `Luger`, `Thompson`, `Kabar`,
`Spade`, `M1Carbine`, `K98Bayonet`, `ButtStock`, `Bazooka`, `Panzerschreck`,
`M1A1Carbine`, `GermanKnife`, `Mg42`, plus 317 records at id 0 / `Unknown` — world and
non-weapon deaths). Notably **`Mg42` appears 6 times and `Browning30Cal`, `Mg34`,
`GreaseGun`, `Fg42`, `LeeEnfield`, `Sten`, `Bren`, `Webley`, `Piat` and `Mortar` never at
all** — consistent with competitive class restrictions, and worth knowing before
optimising analyzer paths for weapons that never fire in this corpus.

## 7. Reproducing this analysis

No IDA or Ghidra required; everything above came from `pefile` + `capstone`
(`pip install capstone pefile`). The core of it:

```python
import pefile, capstone

pe   = pefile.PE(r"...\dod\cl_dlls\client.dll")
base = pe.OPTIONAL_HEADER.ImageBase
img  = pe.get_memory_mapped_image()
md   = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)

exports = {e.name.decode(): e.address
           for e in pe.DIRECTORY_ENTRY_EXPORT.symbols if e.name}

# 1. Read any export's disassembly (start with "F" and "Initialize").
for i in md.disasm(img[exports["F"]:exports["F"]+0x200], base + exports["F"]):
    print(hex(i.address), i.mnemonic, i.op_str)

# 2. gEngfuncs = the `mov edi, imm32` immediately before Initialize's `rep movsd`.
# 3. For an engine-func index N, find `FF 15 <gEngfuncs + N*4>` in .text and walk
#    backwards for the nearest `68 <imm32>` that points at a printable C string:
#    that is the name argument (cdecl pushes right-to-left, so the name lands last).
```

Re-running §5 against a different depot (for example the 25th Anniversary build) is the
way to check whether these offsets and inventories still hold; only `gEngfuncs`'s address
and the `F` table's contents are build-specific, and the hook in §2 depends on neither.
