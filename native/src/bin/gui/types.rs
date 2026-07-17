use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use analysis::{Analysis, PlayerGlobalId, Weapon};
use native::FileInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    Name,
    Type,
    Map,
    Date,
}

#[derive(Default)]
pub struct ScoreboardCache {
    pub path: Option<String>,
    pub allies_players: Vec<usize>, // indices in analysis.state.players
    pub axis_players: Vec<usize>,
    pub spec_players: Vec<usize>,
    pub unassigned_players: Vec<usize>,
    pub allies_totals: (i32, i32, i32), // score, kills, deaths
    pub axis_totals: (i32, i32, i32),
    pub spec_totals: (i32, i32, i32),
    pub unassigned_totals: (i32, i32, i32),
    pub player_steam_ids: HashMap<PlayerGlobalId, String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ChatFilterState {
    pub show_mm1: bool,
    pub show_mm2: bool,
    pub show_status: crate::views::chat::PlayerStatusFilter,
    pub show_team_filter: crate::views::chat::PlayerTeamFilter,
    pub show_joins: bool,
    pub show_teams: bool,
    pub show_gameplay: bool,
    pub show_other_sys: bool,
    pub filter_text: String,
}

#[derive(Default)]
pub struct ChatCache {
    pub path: Option<String>,
    pub filter_state: Option<ChatFilterState>,
    pub filtered_indices: Vec<usize>, // indices in analysis.state.chat_messages
}

#[derive(Clone, Debug)]
pub struct ExportRequest {
    pub start_time: f32,
    pub stop_time: f32,
}

#[derive(Clone, Debug)]
pub struct AddToQueueRequest {
    pub start_time: f32,
    pub stop_time: f32,
    pub streak_idx: usize,
    pub kills_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CapturePhase {
    ReviewQueue,
    Patching,
    HlaeCapture,
    HlcrRendering,
    Complete,
    Failed,
}

pub fn default_capture_phase() -> CapturePhase {
    CapturePhase::ReviewQueue
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CaptureStudioState {
    Scan,
    Select,
    Capture,
    Render,
    Finish,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct QueuedStreakExport {
    pub id: String,
    pub input_path: PathBuf,
    pub player_id: PlayerGlobalId,
    pub player_name: String,
    pub start_time: f32,
    pub stop_time: f32,
    pub streak_idx: usize,
    pub kills_count: usize,
    pub output_name: String,
    pub enabled: bool,
    pub exit_on_finish: bool,
    pub init_commands: Vec<String>,
    pub custom_commands: Vec<native::patch::CustomCommand>,
    pub fast_forward_speed: f32,
    pub hltv_spec_player: Option<String>,
    pub initial_delay: f32,
    pub pre_record_buffer: f32,
    pub record_start_lead: f32,
    pub record_stop_trail: f32,
    pub post_record_buffer: f32,
    #[serde(default = "default_capture_phase")]
    pub status: CapturePhase,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub sub_status: Option<String>,
    #[serde(default)]
    pub debug_command: Option<String>,
    #[serde(skip)]
    pub started_at: Option<std::time::Instant>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
pub struct PendingStreakExport {
    pub input_path: PathBuf,
    pub player_id: PlayerGlobalId,
    pub start_time: f32,
    pub stop_time: f32,
}

#[derive(Default)]
pub struct PlayerDetailsCache {
    pub path: Option<String>,
    pub player_id: Option<PlayerGlobalId>,
    pub disabled_weapons: HashSet<Weapon>,
    pub sorted_weapons: Vec<Weapon>,
    pub sorted_weapon_breakdown: Vec<(Weapon, (u32, u32))>,
    pub filtered_streaks: Vec<(usize, Vec<usize>)>, // (streak_index, Vec<kill_index>)
    pub export_request: Option<ExportRequest>,
    pub add_to_queue_request: Option<AddToQueueRequest>,
    pub batch_export_request: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    Analyzer,
    #[cfg(not(target_arch = "wasm32"))]
    Auditor,
    CaptureStudio,
    Settings,
}

#[cfg(not(target_arch = "wasm32"))]
pub enum AuditorState {
    Idle,
    Scanning {
        rx: std::sync::mpsc::Receiver<hl_demo_auditor::AuditProgress>,
        cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
        progress_text: String,
        files_found: usize,
        last_update: std::time::Instant,
    },
    Complete {
        groups: Vec<hl_demo_auditor::DuplicateGroup>,
        total: usize,
        wasted: u64,
        expanded: HashSet<usize>,
    },
    Failed(String),
}

#[allow(dead_code)]
pub enum GuiMessage {
    Idle,
    AnalyzerStart {
        _files: usize,
    },
    AnalyzerProgress {
        file_info: FileInfo,
        _progress: (usize, usize),
        analysis: Box<Analysis>,
    },
    AnalyzerError {
        path: String,
        error: String,
    },
    DemoParsingProgress {
        path: String,
        progress: f32,
        elapsed_sec: f32,
        eta_sec: Option<f32>,
    },
    PatchingComplete,
    PreviewPatchingComplete,
    #[cfg(not(target_arch = "wasm32"))]
    DirScanComplete {
        dir: PathBuf,
        files: Vec<crate::tree::DemoListItem>,
    },
    #[cfg(not(target_arch = "wasm32"))]
    DemoFoldersScanComplete {
        scan_id: usize,
        folders: Vec<(PathBuf, usize)>,
    },
    #[cfg(target_arch = "wasm32")]
    WebFolderLoaded(Vec<crate::tree::WebFile>),
    #[cfg(target_arch = "wasm32")]
    WebFileParsed {
        path: String,
        file_info: FileInfo,
        analysis: Box<Analysis>,
    },
    #[cfg(not(target_arch = "wasm32"))]
    CapturePipelineUpdate {
        item_id: String,
        phase: CapturePhase,
        sub_status: Option<String>,
        debug_command: Option<String>,
        error: Option<String>,
    },
    #[cfg(not(target_arch = "wasm32"))]
    CaptureStudioFinished,
    CaptureEngineEvent(EngineEvent),
    IngestionFinished,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserView {
    Flat,
    GroupByMatch,
    GroupByPlayer,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct HighlightStreak {
    pub start_tick: i32,
    pub end_tick: i32,
    pub kill_count: usize,
    pub target_player: String,
    pub is_selected: bool,
    pub display_text: String,
    pub timeline_string: String,
    pub duration_string: String,
    pub player_index: usize,
    /// Raw kill events mirrored from CaptureStreak: (tick, abs_time_secs, weapon).
    pub kills: Vec<(i32, f32, String)>,
    pub start_index: usize,
    pub end_index: usize,
    #[serde(default)]
    pub viewdemo_times: Vec<f32>,
    #[serde(skip, default)]
    pub frame_times: std::sync::Arc<Vec<f32>>,
    #[serde(default)]
    pub status: native::patch::HighlightStatus,
    #[serde(default)]
    pub notes: Option<String>,
}

impl HighlightStreak {
    /// Rebuilds `timeline_string`, `duration_string`, `kill_count`, `start_tick`,
    /// and `end_tick` from `kills[start_index..=end_index]`.
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


#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DemoData {
    pub demo_name: String,
    pub path: PathBuf,
    pub streaks: Vec<HighlightStreak>,
    pub tickrate: f32,
    pub is_pov: bool,
    pub local_player_index: Option<usize>,
    pub playback_frames: i32,
    #[serde(default)]
    pub match_start_tick: Option<i32>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CaptureJob {
    pub patched_demo_path: std::path::PathBuf,
    pub expected_take_folder: std::path::PathBuf,
}

#[derive(Clone, Debug)]
pub enum EngineEvent {
    Starting(usize),
    Launching(String),
    Finished(String),
    Verified(String),
    Error(String),
    AllCompleted,
    /// Posted when the cancellation token is raised mid-batch.
    /// Signals the GUI to reset the running flag and show a cancelled message.
    Cancelled,
}

/// Controls whether a recovery modal is shown on startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupState {
    /// Normal startup — no autosave lockfile detected.
    Normal,
    /// An `.autosave.json` lockfile was found; prompt the user before loading.
    PendingRecovery,
    /// A `.render_autosave.json` lockfile was found (render batch interrupted).
    PendingRenderRecovery,
}
