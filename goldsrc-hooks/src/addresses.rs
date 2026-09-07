//! Locates `pEngfuncs` (and `pstudio`) directly inside a loaded `client.dll`,
//! by byte-signature-scanning its own compiled code/data -- the same
//! technique HLAE's `AfxHookGoldSrc.dll` uses (`hl_addresses.cpp`, fetched
//! from `advancedfx/advancedfx` 2026-09-07), ported faithfully (same string
//! anchors, same hex patterns, same byte offsets).
//!
//! Why this exists instead of hooking `client.dll`'s exported `Initialize`:
//! live-tested against a real DoD 1.3 session and confirmed the engine never
//! calls `Initialize`/`HUD_Frame`/`HUD_GetStudioModelInterface` through a
//! live `GetProcAddress` lookup the way the GoldSrc mod ABI docs imply --
//! export-table patching those three names is provably correct (verified
//! with the real Win32 `GetProcAddress` immediately after patching) yet
//! nothing ever calls through to the patched entries, even over 30+ seconds
//! of active gameplay. HLAE's own source never hooks `Initialize` either --
//! it always reads `client.dll`'s own already-populated copy of the pointer
//! directly out of memory, which is what this module replicates.
//!
//! `client.dll`'s `Initialize` function, whenever the engine does end up
//! calling it (by whatever mechanism), immediately stores the engfuncs
//! pointer it's given into a fixed spot referenced by its own compiled code
//! -- these patterns locate that reference and read the value next to it.
//! Two variants exist because HLAE itself has to support two different
//! `client.dll` builds: the older ("SteamLegacy") branch and the current
//! (25th Anniversary-era) branch, distinguished by whether `client.dll`
//! still references the ancient `A3D.DLL` (Aureal 3D audio) string.

use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;

use crate::binscan::{find_bytes, find_cstring, find_pattern_string, image_sections, read_u32, MemRange};

/// HLAE computes this ONCE, from **`hw.dll`'s** own memory (not
/// `client.dll`'s) -- `g_bHas_A3D_DLL_String`, set inside their
/// `Addresses_InitHwDll` and then reused as the branch selector for every
/// later scan, `client.dll` included. `hw.dll` is guaranteed already loaded
/// by the time this runs (we only get here via its own LoadLibraryA hook).
unsafe fn is_steam_legacy() -> bool {
    let hw_dll = unsafe { GetModuleHandleA(c"hw.dll".as_ptr() as *const u8) };
    if hw_dll.is_null() {
        return false;
    }
    let sections = unsafe { image_sections(hw_dll as *mut u8) };
    let Some(&data2_range) = sections.get(2) else {
        return false;
    };
    unsafe { !find_cstring(data2_range, "A3D.DLL").is_empty() }
}

/// Result of scanning a loaded `client.dll`: raw addresses, not yet cast to
/// our own struct types (the caller does that).
pub struct ScannedAddresses {
    pub engfuncs: usize,
    pub pstudio: Option<usize>,
}

/// Scans `client_dll_base` (a loaded, fully-mapped `client.dll`) for
/// `pEngfuncs` and `pstudio`, trying both the SteamLegacy and current
/// `client.dll` build patterns (selected the same way HLAE selects them --
/// by whether `hw.dll` still references the `A3D.DLL` string).
///
/// Safety: `client_dll_base` must point to a fully-mapped, valid PE image.
pub unsafe fn scan_client_dll(client_dll_base: *mut u8) -> Option<ScannedAddresses> {
    let sections = unsafe { image_sections(client_dll_base) };
    unsafe {
        crate::debug::report(&format!(
            "scan_client_dll: {} PE sections found ({})",
            sections.len(),
            sections.iter().map(|r| format!("{:#x}..{:#x}", r.start, r.end)).collect::<Vec<_>>().join(", ")
        ))
    };
    if sections.len() < 3 {
        unsafe { crate::debug::report("scan_client_dll: fewer than 3 sections, can't proceed") };
        return None;
    }
    let text_range = sections[0];
    let data1_range = sections[1];
    let data2_range = sections[2];

    let legacy = unsafe { is_steam_legacy() };
    unsafe { crate::debug::report(&format!("scan_client_dll: is_steam_legacy() = {legacy}, trying BOTH branches regardless (diagnostic)")) };

    // Diagnostic: the fixed "data1 = section[1], data2 = section[2]"
    // assumption (from HLAE's own ImageSectionsReader-based code) may not
    // hold if this build's linker orders/merges sections differently --
    // search every section directly for our anchor strings so we know
    // exactly where (if anywhere) they actually live.
    for (i, &range) in sections.iter().enumerate() {
        for anchor in ["ScreenFade", "HUD_GetStudioModelInterface", "A3D.DLL"] {
            let found = unsafe { find_cstring(range, anchor) };
            if !found.is_empty() {
                unsafe { crate::debug::report(&format!("scan_client_dll: \"{anchor}\" found (NUL-terminated) in section[{i}] at {:#x}", found.start)) };
            }
        }
        // Also check as a bare substring (no trailing NUL required) -- MSVC
        // string-pooling/COMDAT-folding can merge "ScreenFade" into a longer
        // literal like "ScreenFadeXYZ" that shares the same prefix bytes,
        // which the exact NUL-terminated match above would miss entirely.
        let raw = unsafe { find_bytes(range, b"ScreenFade") };
        if !raw.is_empty() {
            unsafe { crate::debug::report(&format!("scan_client_dll: \"ScreenFade\" found as a bare substring (no NUL) in section[{i}] at {:#x}", raw.start)) };
        }
    }

    // Diagnostic: try both branches unconditionally rather than trusting the
    // selector alone, so a wrong selection doesn't masquerade as "the
    // patterns just don't match this build" -- whichever succeeds wins.
    if let Some(found) = unsafe { scan_legacy(text_range, data2_range) } {
        unsafe { crate::debug::report("scan_client_dll: SteamLegacy branch succeeded") };
        return Some(found);
    }
    if let Some(found) = unsafe { scan_current(text_range, data1_range) } {
        unsafe { crate::debug::report("scan_client_dll: Current branch succeeded") };
        return Some(found);
    }
    None
}

/// SteamLegacy branch -- HLAE's `hl_addresses.cpp`, "Checked: 2018-10-06" /
/// "Checked: 2018-09-01" comments.
unsafe fn scan_legacy(text_range: MemRange, data2_range: MemRange) -> Option<ScannedAddresses> {
    let screenfade = unsafe { find_cstring(data2_range, "ScreenFade") };
    if screenfade.is_empty() {
        unsafe { crate::debug::report("scan_legacy: \"ScreenFade\" string not found in data2 section") };
        return None;
    }
    unsafe { crate::debug::report(&format!("scan_legacy: \"ScreenFade\" found at {:#x}", screenfade.start)) };

    let engfuncs = unsafe {
        let r1 = find_bytes(text_range, &(screenfade.start as u32).to_le_bytes());
        if r1.is_empty() {
            crate::debug::report("scan_legacy: no code reference to ScreenFade's address found in .text");
            return None;
        }
        crate::debug::report(&format!("scan_legacy: code reference found at {:#x}", r1.start));
        let r2 = find_pattern_string(
            text_range.and(MemRange::new(r1.start + 0x09, r1.start + 0x09 + 23)),
            "6A 07 68 ?? ?? ?? ?? FF 15 ?? ?? ?? ?? 68 ?? ?? ?? ?? E8 ?? ?? ?? ??",
        );
        if r2.is_empty() {
            crate::debug::report("scan_legacy: byte pattern after the code reference didn't match");
            return None;
        }
        read_u32(r2.start + 3) as usize
    };

    // pstudio: continues the SAME `s1` search position from the ScreenFade
    // match (HLAE's code literally reuses the variable), searching forward
    // for the SECOND occurrence of "HUD_GetStudioModelInterface" after it.
    let mut s1 = screenfade;
    for _ in 0..2 {
        s1 = unsafe { find_cstring(MemRange::new(s1.end, data2_range.end), "HUD_GetStudioModelInterface") };
    }

    let pstudio = if s1.is_empty() {
        None
    } else {
        unsafe {
            let r1 = find_bytes(text_range, &(s1.start as u32).to_le_bytes());
            if r1.is_empty() {
                None
            } else {
                let r2 = find_pattern_string(
                    text_range.and(MemRange::new(r1.start + 0x14, r1.start + 0x14 + 14)),
                    "68 ?? ?? ?? ?? 68 ?? ?? ?? ?? 6A 01 FF D0",
                );
                if r2.is_empty() { None } else { Some(read_u32(r2.start + 1) as usize) }
            }
        }
    };

    Some(ScannedAddresses { engfuncs, pstudio })
}

/// Current (25th Anniversary-era) branch -- HLAE's `hl_addresses.cpp`,
/// "Checked: 2023-12-16" / "Checked: 2023-12-20" comments.
unsafe fn scan_current(text_range: MemRange, data1_range: MemRange) -> Option<ScannedAddresses> {
    let screenfade = unsafe { find_cstring(data1_range, "ScreenFade") };
    if screenfade.is_empty() {
        unsafe { crate::debug::report("scan_current: \"ScreenFade\" string not found in data1 section") };
        return None;
    }
    unsafe { crate::debug::report(&format!("scan_current: \"ScreenFade\" found at {:#x}", screenfade.start)) };

    let engfuncs = unsafe {
        let r1 = find_bytes(text_range, &(screenfade.start as u32).to_le_bytes());
        if r1.is_empty() {
            crate::debug::report("scan_current: no code reference to ScreenFade's address found in .text");
            return None;
        }
        crate::debug::report(&format!("scan_current: code reference found at {:#x}", r1.start));
        let r2 = find_pattern_string(
            text_range.and(MemRange::new(r1.start + 0x09, r1.start + 0x09 + 30)),
            "6A 07 68 ?? ?? ?? ?? FF 15 ?? ?? ?? ?? a1 ?? ?? ?? ?? 83 c4 18 85 c0 74 ?? 68 ?? ?? ?? ??",
        );
        if r2.is_empty() {
            crate::debug::report("scan_current: byte pattern after the code reference didn't match");
            return None;
        }
        read_u32(r2.start + 3) as usize
    };

    let s1 = unsafe { find_cstring(MemRange::new(data1_range.start, data1_range.end), "HUD_GetStudioModelInterface") };

    let pstudio = if s1.is_empty() {
        None
    } else {
        unsafe {
            let r1 = find_bytes(text_range, &(s1.start as u32).to_le_bytes());
            if r1.is_empty() {
                None
            } else {
                let r2 = find_pattern_string(
                    text_range.and(MemRange::new(r1.start + 0x4, r1.start + 0x4 + 26)),
                    "56 ff 15 ?? ?? ?? ?? a1 ?? ?? ?? ?? 85 c0 74 ?? 68 ?? ?? ?? ?? 68 ?? ?? ?? ??",
                );
                if r2.is_empty() { None } else { Some(read_u32(r2.start + 17) as usize) }
            }
        }
    };

    Some(ScannedAddresses { engfuncs, pstudio })
}
