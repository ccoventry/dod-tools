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
    CustomCommand,
    CommandRelation,
    PatchOptions,
    CaptureStreak,
    PatchJob,
    PatcherConfig,
    HighlightRules,
};

#[cfg(not(target_arch = "wasm32"))]
pub use types::{PatchEvent, CaptureWorker};

#[cfg(not(target_arch = "wasm32"))]
pub use highlevel::patch_demo_highlights;

#[cfg(not(target_arch = "wasm32"))]
pub use engine::StreamPatcher;

#[cfg(not(target_arch = "wasm32"))]
pub use builder::{build_batch_queue, spawn_patch_batch, WorkspaceGuard, build_director_message, build_director_stufftext};

#[cfg(not(target_arch = "wasm32"))]
pub use scanner::{is_hltv_demo, scan_demo_for_highlights};
