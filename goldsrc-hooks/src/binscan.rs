//! Minimal port of HLAE's own `shared/binutils.h`/`.cpp` (from
//! `advancedfx/advancedfx`, MIT-ish/permissive engine tooling, fetched
//! 2026-09-07) -- byte/string/pattern search helpers over a loaded module's
//! memory, plus a PE section-table walker. Faithfully translated (same
//! algorithms, same semantics) so the byte patterns and offsets copied into
//! `addresses.rs` behave identically to the C++ originals they were taken
//! from.

use std::os::raw::c_void;

#[derive(Clone, Copy, Debug)]
pub struct MemRange {
    /// Inclusive.
    pub start: usize,
    /// Exclusive.
    pub end: usize,
}

impl MemRange {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    pub fn and(&self, other: MemRange) -> MemRange {
        if self.is_empty() {
            return *self;
        }
        if other.is_empty() {
            return other;
        }
        MemRange::new(self.start.max(other.start), self.end.min(other.end))
    }
}

/// Forward byte-for-byte search, matching HLAE's `FindBytes`. Returns an
/// empty range at `range.start` if `pattern` isn't found.
///
/// Safety: every byte in `range` must be readable.
pub unsafe fn find_bytes(range: MemRange, pattern: &[u8]) -> MemRange {
    if pattern.is_empty() {
        return MemRange::new(range.start, range.start.min(range.end));
    }

    let mut pos = range.start;
    let mut match_depth = 0usize;
    while pos < range.end {
        let cur = unsafe { *(pos as *const u8) };
        if cur == pattern[match_depth] {
            match_depth += 1;
        } else {
            pos -= match_depth;
            match_depth = 0;
            pos += 1;
            continue;
        }

        if match_depth == pattern.len() {
            return MemRange::new(pos + 1 - match_depth, pos + 1);
        }
        pos += 1;
    }

    MemRange::new(range.start, range.start.min(range.end))
}

/// Safety: every byte in `range` must be readable.
pub unsafe fn find_cstring(range: MemRange, s: &str) -> MemRange {
    let mut pattern = s.as_bytes().to_vec();
    pattern.push(0);
    unsafe { find_bytes(range, &pattern) }
}

/// Forward hex-byte-pattern search with `??` wildcards, matching HLAE's
/// `FindPatternString` (e.g. `"6A 07 68 ?? ?? ?? ?? FF 15"`).
///
/// Safety: every byte in `range` must be readable.
pub unsafe fn find_pattern_string(range: MemRange, hex_pattern: &str) -> MemRange {
    let tokens: Vec<&str> = hex_pattern.split_whitespace().collect();
    if tokens.is_empty() {
        return MemRange::new(range.start, range.start.min(range.end));
    }

    let mut pos = range.start;
    let mut match_depth = 0usize;
    while pos < range.end {
        let cur = unsafe { *(pos as *const u8) };
        let tok = tokens[match_depth];
        let matches = if tok == "??" {
            true
        } else {
            u8::from_str_radix(tok, 16).map(|b| b == cur).unwrap_or(false)
        };

        if matches {
            match_depth += 1;
        } else {
            pos -= match_depth;
            match_depth = 0;
            pos += 1;
            continue;
        }

        if match_depth == tokens.len() {
            return MemRange::new(pos + 1 - match_depth, pos + 1);
        }
        pos += 1;
    }

    MemRange::new(range.start, range.start.min(range.end))
}

/// Reads a little-endian u32 out of `range.start` -- used to pull the
/// immediate/address operand HLAE's patterns land on.
///
/// Safety: `range.start..range.start+4` must be readable.
pub unsafe fn read_u32(addr: usize) -> u32 {
    unsafe { (addr as *const u32).read_unaligned() }
}

#[repr(C)]
struct ImageDosHeader {
    e_magic: u16,
    _reserved: [u16; 29],
    e_lfanew: i32,
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
struct ImageSectionHeader {
    _name: [u8; 8],
    _misc: u32, // virtual_size (union with physical_address, same offset)
    virtual_address: u32,
    _size_of_raw_data: u32,
    _pointer_to_raw_data: u32,
    _pointer_to_relocations: u32,
    _pointer_to_linenumbers: u32,
    _number_of_relocations: u16,
    _number_of_linenumbers: u16,
    _characteristics: u32,
}

/// Returns the memory ranges of a loaded module's PE sections, in file
/// order -- e.g. for a typical MSVC build, `.text`, `.rdata`, `.data`, ...
/// Matches HLAE's `ImageSectionsReader` (which iterates unconditionally,
/// not by section name).
///
/// Safety: `module_base` must point to a fully-mapped, valid PE image.
pub unsafe fn image_sections(module_base: *mut u8) -> Vec<MemRange> {
    unsafe {
        let dos = module_base as *const ImageDosHeader;
        let nt_base = module_base.add((*dos).e_lfanew as usize);
        // signature: u32, then ImageFileHeader, then optional header (size
        // given by file_header.size_of_optional_header) -- section table
        // follows immediately after the optional header.
        let file_header = nt_base.add(4) as *const ImageFileHeader;
        let num_sections = (*file_header).number_of_sections as usize;
        let opt_header_size = (*file_header).size_of_optional_header as usize;

        let mut section = nt_base.add(4 + std::mem::size_of::<ImageFileHeader>() + opt_header_size)
            as *const ImageSectionHeader;

        let mut ranges = Vec::with_capacity(num_sections);
        for _ in 0..num_sections {
            let start = module_base.add((*section).virtual_address as usize) as usize;
            let size = (*section)._misc as usize;
            ranges.push(MemRange::new(start, start + size));
            section = (section as *const u8).add(std::mem::size_of::<ImageSectionHeader>()) as *const ImageSectionHeader;
        }
        ranges
    }
}

#[allow(dead_code)]
pub type OpaquePtr = *mut c_void;
