// patch/types.rs
// Pure data layer: structs, enums, and their direct impl blocks.
// No file I/O, no thread spawning. All types except PatchEvent and
// CaptureWorker are WASM-safe.

use std::sync::{Arc, atomic::AtomicBool};

// ── Command scheduling ────────────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CustomCommand {
    pub command: String,
    pub offset: f32,
    pub relation: CommandRelation,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum CommandRelation {
    Before,
    After,
}

// ── High-level patch options (used by the dem-crate API path) ─────────────────

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PatchOptions {
    pub exit_on_finish: bool,
    pub init_commands: Vec<String>,
    pub custom_commands: Vec<CustomCommand>,
    pub fast_forward_speed: Option<f32>,
    pub hltv_spec_player: Option<String>,
    pub initial_delay: Option<f32>,
    pub pre_record_buffer: Option<f32>,
    pub record_start_lead: Option<f32>,
    pub record_stop_trail: Option<f32>,
    pub post_record_buffer: Option<f32>,
    pub player_deaths: Option<Vec<f32>>,
}

// ── Capture result types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CaptureStreak {
    pub start_tick: i32,
    pub end_tick: i32,
    pub source_demo: String,
    pub target_player: Option<String>,
    pub kill_count: usize,
    pub timeline_string: String,
    pub duration_string: String,
    pub player_index: usize,
    /// Raw kill events: (tick, abs_time_secs, weapon). Stored so update_visuals
    /// can rebuild timeline_string from any sub-slice without needing frame_times.
    pub kills: Vec<(i32, f32, String)>,
    pub start_index: usize,
    pub end_index: usize,
    pub total_demo_frames: i32,
    pub demo_fps: f32,
    #[serde(skip, default)]
    pub frame_times: Arc<Vec<f32>>,
}

impl CaptureStreak {
    /// Rebuilds `timeline_string`, `duration_string`, `kill_count`, `start_tick`,
    /// and `end_tick` from `kills[start_index..=end_index]`. Must be called after
    /// any mutation of `start_index` or `end_index`.
    pub fn update_visuals(&mut self) {
        if self.kills.is_empty() {
            return;
        }
        let end = self.end_index.min(self.kills.len().saturating_sub(1));
        let start = self.start_index.min(end);
        let slice = &self.kills[start..=end];

        self.start_tick = slice[0].0;
        self.end_tick = slice[slice.len() - 1].0;
        self.kill_count = slice.len();

        let total_secs = (slice.last().unwrap().1 - slice[0].1).max(0.0).round() as i32;
        self.duration_string = format!("{}:{:02}", total_secs / 60, total_secs % 60);

        let mut parts: Vec<String> = Vec::with_capacity(slice.len());
        for (i, (_, abs_time, weapon)) in slice.iter().enumerate() {
            let weapon_clean = weapon.trim_start_matches("Weapon::").to_string();
            if i == 0 {
                parts.push(weapon_clean);
            } else {
                let gap_sec = (abs_time - slice[i - 1].1).max(0.0).round() as i32;
                parts.push(format!("(+{}:{:02}) {}", gap_sec / 60, gap_sec % 60, weapon_clean));
            }
        }
        self.timeline_string = parts.join(", ");
    }
}

#[derive(Debug, Clone)]
pub struct PatchJob {
    pub source_demo: String,
    pub output_demo: std::path::PathBuf,
    pub streaks: Vec<CaptureStreak>,
    pub target_player: Option<String>,
    pub init_commands: Vec<String>,
    pub scheduled_commands: Vec<(i32, String)>,
}

// ── Patcher configuration ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PatcherConfig {
    pub pre_roll_ticks: i32,
    pub post_roll_ticks: i32,
    pub capture_fps: i32,
    pub exit_on_finish: bool,
    pub init_commands: Vec<String>,
    pub custom_commands: Vec<CustomCommand>,
    pub pre_roll_seconds: f32,
    pub post_roll_seconds: f32,
    pub record_start_lead: f32,
    pub record_stop_trail: f32,
    pub initial_delay: f32,
    pub fast_forward_speed: f32,
    pub tickrate: f32,
    pub output_dir: Option<std::path::PathBuf>,
    pub separate_hud: bool,
    pub resolution_width: i32,
    pub resolution_height: i32,
    pub primary_media_dir: Option<std::path::PathBuf>,
    pub backup_media_dir: Option<std::path::PathBuf>,
    pub movie_config: String,
    pub save_local_patched_copy: bool,
    pub add_condebug: bool,
    pub session_id: String,
}

impl Default for PatcherConfig {
    fn default() -> Self {
        Self {
            pre_roll_ticks: 200,
            post_roll_ticks: 60,
            capture_fps: 300,
            exit_on_finish: true,
            init_commands: Vec::new(),
            custom_commands: Vec::new(),
            pre_roll_seconds: 2.0,
            post_roll_seconds: 0.6,
            record_start_lead: 0.0,
            record_stop_trail: 0.0,
            initial_delay: 3.0,
            fast_forward_speed: 0.2,
            tickrate: 100.0,
            output_dir: None,
            separate_hud: false,
            resolution_width: 1280,
            resolution_height: 720,
            primary_media_dir: None,
            backup_media_dir: None,
            movie_config: String::new(),
            save_local_patched_copy: false,
            add_condebug: false,
            session_id: String::new(),
        }
    }
}

// ── Scanner filter rules ──────────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct HighlightRules {
    pub min_kills: Option<usize>,
    pub max_time_gap: Option<f32>,
    pub target_players: Vec<String>,
}

// ── Worker event channel types (native-only: require std threading) ───────────

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub enum PatchEvent {
    Starting(usize),
    Progress(String, f32),
    Completed,
    Cancelled,
    Error(String),
}

#[cfg(not(target_arch = "wasm32"))]
pub struct CaptureWorker {
    pub receiver: std::sync::mpsc::Receiver<PatchEvent>,
    pub is_running: bool,
    pub cancel_token: Arc<AtomicBool>,
    pub handle: Option<std::thread::JoinHandle<()>>,
}
