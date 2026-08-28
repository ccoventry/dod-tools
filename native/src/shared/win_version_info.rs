//! Reads the version strings Windows embeds in an executable's resources.
//!
//! Used to tell whether a chosen path is the program it is supposed to be, in
//! the one case where the executable cannot simply be run and asked: launching
//! HLAE.exe starts the application. FFmpeg gets asked directly instead, in
//! `shared::hlae_ffmpeg::verify_is_ffmpeg` — and it has to, because the FFmpeg
//! builds people actually download carry **no version metadata at all**
//! (measured: `ffmpeg.exe` and `ffplay.exe` from a working install both return
//! empty for every field). A metadata check there would reject a perfectly good
//! FFmpeg; a metadata check here is the only option available.
//!
//! Advisory by design. Nothing in this module should ever reject a path outright
//! — see `hlae_original_filename` for why the values are less dependable than
//! they look.

#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

#[cfg(windows)]
mod sys {
    #[link(name = "version")]
    unsafe extern "system" {
        pub fn GetFileVersionInfoSizeW(filename: *const u16, handle: *mut u32) -> u32;
        pub fn GetFileVersionInfoW(
            filename: *const u16,
            handle: u32,
            len: u32,
            data: *mut u8,
        ) -> i32;
        pub fn VerQueryValueW(
            block: *const u8,
            sub_block: *const u16,
            buffer: *mut *mut core::ffi::c_void,
            len: *mut u32,
        ) -> i32;
    }
}

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// One `StringFileInfo` value — `OriginalFilename`, `ProductName`, and so on —
/// or `None` when the file carries no version resource, or none in a language
/// this build of it uses.
///
/// The translation block is read first rather than assuming `040904b0` (US
/// English + Unicode). That constant is the usual shortcut and it is wrong the
/// moment a binary is compiled under a different locale: the query returns
/// nothing and a perfectly legitimate file reads as unidentifiable. Every
/// translation the file declares is tried, in order.
#[cfg(windows)]
pub fn string_value(path: &Path, name: &str) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let file = wide(&path.to_string_lossy());

    // SAFETY: `file` is a NUL-terminated UTF-16 string that outlives the call,
    // and `handle` is a live u32 the API only writes to.
    let mut handle: u32 = 0;
    let size = unsafe { sys::GetFileVersionInfoSizeW(file.as_ptr(), &mut handle) };
    if size == 0 {
        return None;
    }

    let mut data = vec![0u8; size as usize];
    // SAFETY: `data` is exactly `size` bytes, which is what the call above said
    // to allocate, and is the length passed here.
    let ok = unsafe { sys::GetFileVersionInfoW(file.as_ptr(), 0, size, data.as_mut_ptr()) };
    if ok == 0 {
        return None;
    }

    let translations = translations(&data)?;
    for (language, codepage) in translations {
        let sub_block = wide(&format!(
            "\\StringFileInfo\\{:04x}{:04x}\\{}",
            language, codepage, name
        ));
        let mut buffer: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut len: u32 = 0;
        // SAFETY: `data` is the block returned by GetFileVersionInfoW and is
        // still alive; `sub_block` is NUL-terminated and outlives the call. On
        // success the API points `buffer` into `data`, so the slice below
        // borrows memory this function still owns.
        let found = unsafe {
            sys::VerQueryValueW(data.as_ptr(), sub_block.as_ptr(), &mut buffer, &mut len)
        };
        if found == 0 || buffer.is_null() || len == 0 {
            continue;
        }
        // `len` counts UTF-16 characters including the terminator.
        let chars = unsafe { std::slice::from_raw_parts(buffer as *const u16, len as usize) };
        let value = String::from_utf16_lossy(chars)
            .trim_end_matches('\0')
            .trim()
            .to_string();
        if !value.is_empty() {
            return Some(value);
        }
    }
    None
}

/// The `(language, codepage)` pairs the file declares.
#[cfg(windows)]
fn translations(data: &[u8]) -> Option<Vec<(u16, u16)>> {
    let sub_block = wide("\\VarFileInfo\\Translation");
    let mut buffer: *mut core::ffi::c_void = std::ptr::null_mut();
    let mut len: u32 = 0;
    // SAFETY: as above — `data` outlives the call and the result points into it.
    let found =
        unsafe { sys::VerQueryValueW(data.as_ptr(), sub_block.as_ptr(), &mut buffer, &mut len) };
    if found == 0 || buffer.is_null() || len < 4 {
        return None;
    }
    // `len` is a byte count here, and each entry is two u16s.
    let pairs = unsafe { std::slice::from_raw_parts(buffer as *const u16, (len / 2) as usize) };
    Some(pairs.chunks_exact(2).map(|c| (c[0], c[1])).collect())
}

#[cfg(not(windows))]
pub fn string_value(_path: &Path, _name: &str) -> Option<String> {
    None
}

/// Whether a path looks like HLAE's own executable, as far as can be told.
///
/// `None` means "no opinion" and is returned both for a file with no version
/// resource and for one that looks right. Only a file that carries metadata
/// *and* names itself something else produces a complaint, and even that is
/// advisory — the caller reports it and carries on.
///
/// **Only `OriginalFilename` is consulted, deliberately.** Measured on HLAE
/// 2.191.1.0: `FileDescription` and `ProductName` are both `hlae`, not the
/// project's full name, and `OriginalFilename` is `hlae.exe`. A widely-repeated
/// answer claims all three hold "Half-Life Advanced Effects", which would reject
/// a genuine install outright. Since only one build has actually been measured
/// here, testing the fewest fields that identify it keeps a build nobody has
/// seen from being called a fake.
pub fn hlae_mismatch(exe: &Path) -> Option<String> {
    let declared = string_value(exe, "OriginalFilename")?;
    if declared.eq_ignore_ascii_case("hlae.exe") {
        return None;
    }
    Some(declared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dod_verinfo_{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    #[test]
    fn a_file_with_no_version_resource_has_nothing_to_say() {
        // Not an error and not a complaint: this is the state a real FFmpeg
        // build is in, so "no metadata" must read as "no opinion".
        let dir = scratch("bare");
        let bare = dir.join("thing.exe");
        std::fs::write(&bare, b"not a PE file").expect("write");
        assert_eq!(string_value(&bare, "OriginalFilename"), None);
        assert_eq!(hlae_mismatch(&bare), None, "silence, not an accusation");
    }

    #[test]
    fn a_path_that_is_not_there_has_nothing_to_say() {
        let dir = scratch("absent");
        assert_eq!(string_value(&dir.join("nope.exe"), "OriginalFilename"), None);
        assert_eq!(hlae_mismatch(&dir.join("nope.exe")), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_windows_binary_yields_its_declared_name() {
        // Proves the translation lookup and the UTF-16 read actually work
        // against a real resource, without depending on HLAE being installed.
        let notepad = PathBuf::from(r"C:\Windows\System32\notepad.exe");
        if !notepad.is_file() {
            eprintln!("no notepad.exe; skipping");
            return;
        }
        let name = string_value(&notepad, "OriginalFilename")
            .expect("a system binary must carry version info");
        assert!(
            name.to_ascii_lowercase().starts_with("notepad"),
            "unexpected OriginalFilename: {}",
            name
        );
    }

    #[cfg(windows)]
    #[test]
    fn something_that_is_not_hlae_is_reported_by_the_name_it_gives() {
        // Naming what it actually is beats "invalid" — the usual cause is
        // picking the wrong executable, and saying which one points at the fix.
        let notepad = PathBuf::from(r"C:\Windows\System32\notepad.exe");
        if !notepad.is_file() {
            eprintln!("no notepad.exe; skipping");
            return;
        }
        let declared = hlae_mismatch(&notepad).expect("notepad is not HLAE");
        assert!(declared.to_ascii_lowercase().starts_with("notepad"), "{}", declared);
    }
}
