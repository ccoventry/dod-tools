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
    ReviewingQueue,
    Capturing,
    Rendering,
    Complete,
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
    pub init_commands: String,
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
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserView {
    Flat,
    GroupByMatch,
    GroupByPlayer,
}
