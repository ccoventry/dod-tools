// ============================================================
// views/capture/capture.rs
// Placeholder sub-module for CaptureStudioState::Capture.
//
// The active Capture step UI (HLAE paths, Launch button, progress bar)
// is rendered directly in capture_studio.rs, which has full access to
// Gui fields (hlae_path, game_path, capture_engine_running, etc.) that
// are not passed through the render_patch_ui dispatcher.
//
// This module is reserved for any future extraction of that logic into
// a standalone stateless render function.
// ============================================================

/// Render placeholder — currently unused by the dispatcher.
/// The Capture step is handled by `capture_studio.rs` directly.
#[allow(dead_code)]
pub fn render() {}
