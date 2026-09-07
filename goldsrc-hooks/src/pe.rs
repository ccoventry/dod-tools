//! Minimal, dependency-free PE (Portable Executable) helpers: walking a
//! loaded module's Import Address Table (IAT) to find/patch a specific
//! imported function slot, and walking its Export Address Table to find a
//! specific exported function's address.
//!
//! This is deliberately hand-rolled instead of pulling in a PE-parsing crate:
//! we only need two narrow operations, and every offset here is defined by
//! the (frozen, decades-stable) Windows PE/COFF format, not by GoldSrc.

use std::ffi::CStr;
use std::os::raw::{c_char, c_void};

#[repr(C)]
struct ImageDosHeader {
    e_magic: u16,
    _reserved: [u16; 29],
    e_lfanew: i32,
}

#[repr(C)]
struct ImageDataDirectory {
    virtual_address: u32,
    size: u32,
}

#[repr(C)]
struct ImageNtHeaders32 {
    signature: u32,
    file_header: ImageFileHeader,
    optional_header: ImageOptionalHeader32,
}

#[repr(C)]
struct ImageFileHeader {
    machine: u16,
    number_of_sections: u16,
    time_date_stamp: u32,
    pointer_to_symbol_table: u32,
    number_of_symbols: u32,
    size_of_optional_header: u16,
    characteristics: u16,
}

#[repr(C)]
struct ImageOptionalHeader32 {
    magic: u16,
    // MajorLinkerVersion..NumberOfRvaAndSizes -- 94 bytes we don't need
    // individual access to, up to DataDirectory[] at offset 96.
    _skip_to_data_dirs: [u8; 94],
    data_directory: [ImageDataDirectory; 16],
}

const IMAGE_DIRECTORY_ENTRY_EXPORT: usize = 0;
const IMAGE_DIRECTORY_ENTRY_IMPORT: usize = 1;

#[repr(C)]
struct ImageExportDirectory {
    _characteristics: u32,
    _time_date_stamp: u32,
    _major_version: u16,
    _minor_version: u16,
    _name: u32,
    _base: u32,
    number_of_functions: u32,
    number_of_names: u32,
    address_of_functions: u32,
    address_of_names: u32,
    address_of_name_ordinals: u32,
}

#[repr(C)]
struct ImageImportDescriptor {
    original_first_thunk: u32,
    _time_date_stamp: u32,
    _forwarder_chain: u32,
    name: u32,
    first_thunk: u32,
}

#[repr(C)]
struct ImageImportByName {
    _hint: u16,
    name: [c_char; 1],
}

const IMAGE_ORDINAL_FLAG32: u32 = 0x8000_0000;

unsafe fn rva<T>(base: *mut u8, rva: u32) -> *mut T {
    unsafe { base.add(rva as usize) as *mut T }
}

unsafe fn nt_headers(base: *mut u8) -> *mut ImageNtHeaders32 {
    unsafe {
        let dos = base as *const ImageDosHeader;
        rva(base, (*dos).e_lfanew as u32)
    }
}

/// Finds the address of an exported function by name in a loaded module.
///
/// Returns `None` if the module has no export table or the name isn't found.
/// Safety: `module_base` must point to a fully-mapped, valid PE image.
pub unsafe fn find_export(module_base: *mut u8, name: &str) -> Option<*mut c_void> {
    unsafe {
        let nt = nt_headers(module_base);
        let dir = &(*nt).optional_header.data_directory[IMAGE_DIRECTORY_ENTRY_EXPORT];
        if dir.virtual_address == 0 {
            return None;
        }
        let exp: *mut ImageExportDirectory = rva(module_base, dir.virtual_address);
        let names: *mut u32 = rva(module_base, (*exp).address_of_names);
        let funcs: *mut u32 = rva(module_base, (*exp).address_of_functions);
        let ordinals: *mut u16 = rva(module_base, (*exp).address_of_name_ordinals);

        for i in 0..(*exp).number_of_names {
            let name_ptr: *const c_char = rva(module_base, *names.add(i as usize));
            let candidate = CStr::from_ptr(name_ptr).to_string_lossy();
            if candidate == name {
                let ord = *ordinals.add(i as usize) as usize;
                let func_rva = *funcs.add(ord);
                return Some(rva(module_base, func_rva));
            }
        }
        None
    }
}

/// Overwrites an exported function's entry in `module_base`'s Export Address
/// Table so future `GetProcAddress`/export-table lookups for `name` resolve
/// to `new_target` instead. Returns whether the export was found and patched.
///
/// Safety: `module_base` must point to a fully-mapped, valid PE image whose
/// export directory is in writable (or protection-changeable) memory.
pub unsafe fn patch_export(module_base: *mut u8, name: &str, new_target: *mut c_void) -> bool {
    use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS};

    unsafe {
        let nt = nt_headers(module_base);
        let dir = &(*nt).optional_header.data_directory[IMAGE_DIRECTORY_ENTRY_EXPORT];
        if dir.virtual_address == 0 {
            return false;
        }
        let exp: *mut ImageExportDirectory = rva(module_base, dir.virtual_address);
        let names: *mut u32 = rva(module_base, (*exp).address_of_names);
        let funcs: *mut u32 = rva(module_base, (*exp).address_of_functions);
        let ordinals: *mut u16 = rva(module_base, (*exp).address_of_name_ordinals);

        for i in 0..(*exp).number_of_names {
            let name_ptr: *const c_char = rva(module_base, *names.add(i as usize));
            let candidate = CStr::from_ptr(name_ptr).to_string_lossy();
            if candidate == name {
                let ord = *ordinals.add(i as usize) as usize;
                let slot = funcs.add(ord);
                // Export RVAs are meant to point *inside* this module, but
                // nothing enforces that at resolution time -- both our own
                // `rva()` helper and the real Windows loader just compute
                // `module_base + rva` with 32-bit wraparound and hand back
                // whatever that lands on. Since we want `new_target` (a
                // function in a *different* module) to come back out, we
                // store `new_target - module_base` here; wraparound addition
                // later reconstructs `new_target` exactly regardless of
                // where it actually lives.
                let new_rva = (new_target as usize).wrapping_sub(module_base as usize) as u32;

                let mut old_protect: PAGE_PROTECTION_FLAGS = 0;
                if VirtualProtect(slot as *mut c_void, size_of::<u32>(), PAGE_EXECUTE_READWRITE, &mut old_protect) == 0 {
                    return false;
                }
                *slot = new_rva;
                VirtualProtect(slot as *mut c_void, size_of::<u32>(), old_protect, &mut old_protect);
                return true;
            }
        }
        false
    }
}

/// Finds the writable IAT slot (a `*mut *mut c_void`, one pointer-sized cell)
/// that `importing_module` uses to call `import_name` from `from_dll`
/// (case-insensitive DLL name match, e.g. "KERNEL32.dll").
///
/// This is the same technique HLAE itself already uses (see
/// `AfxHookGoldSrc`'s `CAfxImportDllHook`) to intercept a *specific module's*
/// calls to a function, rather than inline-patching the function's own code
/// (which would affect every caller in the process, including the DLL that
/// exports it).
///
/// Safety: `importing_module` must point to a fully-mapped, valid PE image.
pub unsafe fn find_iat_slot(
    importing_module: *mut u8,
    from_dll: &str,
    import_name: &str,
) -> Option<*mut *mut c_void> {
    unsafe {
        let nt = nt_headers(importing_module);
        let dir = &(*nt).optional_header.data_directory[IMAGE_DIRECTORY_ENTRY_IMPORT];
        if dir.virtual_address == 0 {
            return None;
        }

        let mut desc: *mut ImageImportDescriptor = rva(importing_module, dir.virtual_address);
        while (*desc).name != 0 {
            let dll_name_ptr: *const c_char = rva(importing_module, (*desc).name);
            let dll_name = CStr::from_ptr(dll_name_ptr).to_string_lossy();
            if dll_name.eq_ignore_ascii_case(from_dll) {
                // Prefer OriginalFirstThunk (the by-name lookup table) to find
                // *which* slot corresponds to `import_name`; FirstThunk (same
                // index) is the actual IAT the code calls through, and is what
                // we overwrite.
                let lookup_rva = if (*desc).original_first_thunk != 0 {
                    (*desc).original_first_thunk
                } else {
                    (*desc).first_thunk
                };
                let mut lookup: *mut u32 = rva(importing_module, lookup_rva);
                let mut iat: *mut *mut c_void = rva(importing_module, (*desc).first_thunk);

                while *lookup != 0 {
                    if (*lookup & IMAGE_ORDINAL_FLAG32) == 0 {
                        let by_name: *const ImageImportByName = rva(importing_module, *lookup);
                        let name_ptr = (*by_name).name.as_ptr();
                        let candidate = CStr::from_ptr(name_ptr).to_string_lossy();
                        if candidate == import_name {
                            return Some(iat);
                        }
                    }
                    lookup = lookup.add(1);
                    iat = iat.add(1);
                }
                return None;
            }
            desc = desc.add(1);
        }
        None
    }
}
