// patch/mod.rs
// Public surface of the patch module.
//
// Declares all sub-modules and re-exports every public item that was previously
// at the flat `native::patch::*` path. All existing call sites remain unchanged.
//
// Sub-module creation order (Phase 10 sequence):
//   Step 1 (current): types.rs, mod.rs  ← foundation
//   Step 2 (pending): highlevel.rs      ← dem-crate high-level API
//   Step 3 (pending): engine.rs         ← StreamPatcher binary I/O
//   Step 4 (pending): builder.rs        ← build_batch_queue, spawn_patch_batch
//   Step 5 (pending): scanner.rs        ← scan_demo_for_highlights, is_hltv_demo

// Engine & Memory Limits
pub const MAX_CONSOLE_CMD_LEN: usize = 64;
pub const MAX_CONSOLE_CMD_SAFE_LEN: usize = 63;
pub const MAX_DIRECTOR_STUFFTEXT_LEN: usize = 253;
pub const IO_BUFFER_CAPACITY: usize = 262_144;
pub const MAX_PAYLOAD_LIMIT_BYTES: usize = 2_097_152;

// Binary Frame & Header Sizes
pub const HLTV_HEADER_SIZE: usize = 512;
pub const DEMO_HEADER_SIZE: usize = 544;
pub const DIRECTORY_OFFSET_POS: usize = 540;
pub const FRAME_HEADER_SIZE: usize = 9;
pub const NETMSG_INFO_SIZE: usize = 464;
pub const NETWORK_HEADER_ALIGNMENT: usize = 468;
pub const DIR_ENTRY_SIZE: usize = 92;
pub const SCANNER_SECTION_BOUNDARY: u8 = 5;

// Frame Type Payload Sizes
pub const CMD_FRAME_SIZE: usize = 64;
pub const CLIENT_DATA_FRAME_SIZE: usize = 32;
pub const EVENT_FRAME_SIZE: usize = 84;

// Command Injection Logic
pub const MAX_ECHO_CHUNK_SIZE: usize = 55;
pub const CUSTOM_CMD_WARN_LIMIT: usize = 60;
pub const PRIMER_DELAY_TICKS: i32 = 500;
pub const BREADCRUMB_INTERVAL_TICKS: i32 = 5000;

pub mod types;

#[cfg(not(target_arch = "wasm32"))]
pub mod highlevel;
#[cfg(not(target_arch = "wasm32"))]
pub mod engine;
#[cfg(not(target_arch = "wasm32"))]
pub mod builder;
#[cfg(not(target_arch = "wasm32"))]
pub mod scanner;

// ── Re-export wall ────────────────────────────────────────────────────────────
// All items below were previously at the top level of patch.rs.
// Every existing `native::patch::*` call site resolves here unchanged.

pub use types::{
    MAX_PAYLOAD_SIZE,
    CustomCommand,
    CommandRelation,
    PatchOptions,
    CaptureStreak,
    PatchJob,
    PatcherConfig,
    HighlightRules,
    DriveAllocationStrategy,
    HighlightStatus,
};

#[cfg(not(target_arch = "wasm32"))]
pub use types::{PatchEvent, CaptureWorker};

#[cfg(not(target_arch = "wasm32"))]
pub use highlevel::patch_demo_highlights;

#[cfg(not(target_arch = "wasm32"))]
pub use engine::StreamPatcher;

#[cfg(not(target_arch = "wasm32"))]
pub use builder::{build_batch_queue, spawn_patch_batch, WorkspaceGuard, build_director_message, build_director_stufftext, build_preview_patch_jobs};

#[cfg(not(target_arch = "wasm32"))]
pub use scanner::{is_hltv_demo, scan_demo_for_highlights};
