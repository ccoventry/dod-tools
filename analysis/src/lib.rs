mod clan_match;
mod chat;
mod kill;
mod localization;
mod mortality;
mod player;
mod round;
mod scoreboard;
mod time;

use crate::{
    clan_match::{ClanMatchDetection, use_clan_match_detection_updates},
    chat::use_chat_updates,
    kill::{use_kill_streak_updates, use_weapon_breakdown_updates},
    mortality::with_mortality_detection,
    player::use_player_updates,
    round::use_rounds_updates,
    scoreboard::{TeamScores, use_scoreboard_updates, use_team_score_updates},
    time::{GameTime, use_timing_updates},
};
use dem::{
    open_demo_from_bytes,
    types::{Demo, EngineMessage, Frame, FrameData, MessageData, NetMessage},
};
use dod::UserMessage;
use std::time::Duration;

pub use crate::{
    chat::{ChatMessage, ChatType, translate_system_message},
    mortality::MortalityState,
    player::{Connection, Player, PlayerGlobalId, SteamId},
    round::Round,
    localization::{set_active_language, get_active_language, translate_key},
};
pub use dod::{Team, Weapon};

#[derive(Debug)]
pub enum AnalyzerEvent<'a> {
    Initialization,
    Finalization,

    Frame(&'a Frame),
    EngineMessage(&'a EngineMessage),
    UserMessage(UserMessage),
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct WeaponPovStats {
    pub bullets_fired: u32,
    pub reloads: u32,
    pub kills: u32,
    pub noscopes: u32,
    pub scoped_kills: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PovStats {
    pub is_scoped: bool,
    pub hits_taken: u32,
    pub total_damage_taken: u32,
    pub suicides: u32,
    pub teamkills_committed: u32,
    pub teamkills_suffered: u32,
    pub weapon_stats: std::collections::HashMap<Weapon, WeaponPovStats>,
    
    // Tracking state
    pub current_weapon: Option<Weapon>,
    pub prev_clip_ammo: u32,
    pub prev_health: u32,
    pub has_received_health: bool,
}

#[derive(Debug, Default)]
pub struct AnalyzerState {
    clan_match_detection: ClanMatchDetection,
    current_time: GameTime,

    pub frame_index: usize,
    pub players: Vec<Player>,
    pub rounds: Vec<Round>,
    pub team_scores: TeamScores,
    pub chat_messages: Vec<ChatMessage>,
    pub pov_player_index: Option<u8>,
    pub pov_stats: PovStats,
}

#[derive(Default)]
pub struct DemoInfo {
    /// Version of the demo protocol used to encode the demo.
    pub demo_protocol: i32,

    /// Name of the map the demo was recorded on.
    pub map_name: String,

    /// Version of the network protocol used during the game.
    pub network_protocol: i32,

    /// Playback time of the demo in seconds.
    pub playback_time: f32,

    /// Game directory / mod name.
    pub game_directory: String,

    /// Type of demo: "HLTV" or "POV"
    pub demo_type: String,
}

impl From<Demo> for DemoInfo {
    fn from(value: Demo) -> Self {
        let map_name = value
            .header
            .map_name
            .to_str()
            .map(|s| s.trim_end_matches('\x00'))
            .unwrap_or("unknown")
            .to_string();

        let game_directory = value
            .header
            .game_directory
            .to_str()
            .map(|s| s.trim_end_matches('\x00'))
            .unwrap_or("unknown")
            .to_string();

        let playback_time = value
            .directory
            .entries
            .first()
            .map(|e| e.track_time)
            .unwrap_or(0.0);

        let mut is_hltv = false;
        'outer: for entry in &value.directory.entries {
            for frame in &entry.frames {
                if let FrameData::NetworkMessage(box_type) = &frame.frame_data {
                    if let MessageData::Parsed(msgs) = &box_type.1.messages {
                        for msg in msgs {
                            if let NetMessage::EngineMessage(eng_msg) = msg {
                                if matches!(**eng_msg, EngineMessage::SvcHltv(_) | EngineMessage::SvcDirector(_)) {
                                    is_hltv = true;
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
            }
        }
        let demo_type = if is_hltv { "HLTV".to_string() } else { "POV".to_string() };

        Self {
            demo_protocol: value.header.demo_protocol,
            map_name,
            network_protocol: value.header.network_protocol,
            playback_time,
            game_directory,
            demo_type,
        }
    }
}

#[derive(Default)]
pub struct Analysis {
    pub demo_info: DemoInfo,
    pub state: AnalyzerState,
}

fn is_relevant_message(name_bytes: &[u8]) -> bool {
    let mut len = name_bytes.len();
    while len > 0 && name_bytes[len - 1] == 0 {
        len -= 1;
    }
    let trimmed = &name_bytes[..len];
    
    matches!(
        trimmed,
        b"RoundState"
            | b"ClanTimer"
            | b"TeamScore"
            | b"ScoreShort"
            | b"ObjScore"
            | b"Frags"
            | b"PClass"
            | b"PTeam"
            | b"ScoreInfo"
            | b"ScoreInfoLong"
            | b"SayText"
            | b"TextMsg"
            | b"DeathMsg"
            | b"PStatus"
            | b"Scope"
            | b"CurWeapon"
            | b"ReloadDone"
            | b"ResetHUD"
            | b"Health"
    )
}

pub fn use_pov_stats_updates(state: &mut AnalyzerState, event: &AnalyzerEvent) {
    if let AnalyzerEvent::EngineMessage(EngineMessage::SvcServerInfo(msg)) = event {
        state.pov_player_index = Some(msg.player_index);
    }

    if let AnalyzerEvent::UserMessage(user_msg) = event {
        match user_msg {
            UserMessage::Scope(_) => {
                state.pov_stats.is_scoped = !state.pov_stats.is_scoped;
            }
            UserMessage::CurWeapon(msg) => {
                if let Some(ref prev_weapon) = state.pov_stats.current_weapon {
                    if prev_weapon == &msg.weapon {
                        if (msg.clip_ammo as u32) < state.pov_stats.prev_clip_ammo {
                            let entry = state.pov_stats.weapon_stats.entry(msg.weapon.clone()).or_default();
                            entry.bullets_fired += 1;
                        }
                    } else {
                        state.pov_stats.is_scoped = false;
                    }
                } else {
                    state.pov_stats.is_scoped = false;
                }
                state.pov_stats.current_weapon = Some(msg.weapon.clone());
                state.pov_stats.prev_clip_ammo = msg.clip_ammo as u32;
            }
            UserMessage::ReloadDone(_) => {
                if let Some(ref active_weapon) = state.pov_stats.current_weapon {
                    let entry = state.pov_stats.weapon_stats.entry(active_weapon.clone()).or_default();
                    entry.reloads += 1;
                }
                state.pov_stats.is_scoped = false;
            }
            UserMessage::ResetHUD(_) => {
                state.pov_stats.is_scoped = false;
                state.pov_stats.has_received_health = false;
            }
            UserMessage::Health(msg) => {
                let health_val = msg.0 as u32;
                if state.pov_stats.has_received_health {
                    if health_val < state.pov_stats.prev_health {
                        state.pov_stats.hits_taken += 1;
                        state.pov_stats.total_damage_taken += state.pov_stats.prev_health - health_val;
                    }
                }
                state.pov_stats.prev_health = health_val;
                state.pov_stats.has_received_health = true;
            }
            UserMessage::DeathMsg(msg) => {
                if let Some(pov_idx) = state.pov_player_index {
                    let is_killer_pov = msg.killer_client_index > 0 && msg.killer_client_index - 1 == pov_idx;
                    let is_victim_pov = msg.victim_client_index > 0 && msg.victim_client_index - 1 == pov_idx;

                    if is_killer_pov {
                        if msg.killer_client_index == msg.victim_client_index {
                            state.pov_stats.suicides += 1;
                        } else {
                            let killer_team = state.find_player_by_client_index(msg.killer_client_index - 1).and_then(|p| p.team.clone());
                            let victim_team = state.find_player_by_client_index(msg.victim_client_index - 1).and_then(|p| p.team.clone());
                            if killer_team.is_some() && victim_team.is_some() && killer_team == victim_team {
                                state.pov_stats.teamkills_committed += 1;
                            } else {
                                let entry = state.pov_stats.weapon_stats.entry(msg.weapon.clone()).or_default();
                                entry.kills += 1;
                                if matches!(
                                    msg.weapon,
                                    Weapon::Springfield
                                        | Weapon::ScopedK98
                                        | Weapon::ScopedFg42
                                        | Weapon::ScopedLeeEnfield
                                ) {
                                    if state.pov_stats.is_scoped {
                                        entry.scoped_kills += 1;
                                    } else {
                                        entry.noscopes += 1;
                                    }
                                }
                            }
                        }
                    } else if msg.killer_client_index == 0 && is_victim_pov {
                        state.pov_stats.suicides += 1;
                    }

                    if is_victim_pov {
                        state.pov_stats.is_scoped = false;
                        if msg.killer_client_index > 0 && !is_killer_pov {
                            let killer_team = state.find_player_by_client_index(msg.killer_client_index - 1).and_then(|p| p.team.clone());
                            let victim_team = state.find_player_by_client_index(msg.victim_client_index - 1).and_then(|p| p.team.clone());
                            if killer_team.is_some() && victim_team.is_some() && killer_team == victim_team {
                                state.pov_stats.teamkills_suffered += 1;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl Analysis {
    fn new(demo_info: DemoInfo, state: AnalyzerState) -> Self {
        Self { demo_info, state }
    }

    pub fn try_from_bytes(value: &[u8]) -> Result<Self, String> {
        Self::try_from_bytes_with_progress(value, |_, _| {})
    }

    pub fn try_from_bytes_with_progress<F>(value: &[u8], mut progress_cb: F) -> Result<Self, String>
    where
        F: FnMut(usize, usize),
    {
        let demo_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            open_demo_from_bytes(value)
        }));
        let demo = match demo_res {
            Ok(Ok(d)) => d,
            Ok(Err(e)) => return Err(format!("Parse error: {}", e)),
            Err(_) => return Err("Parser panicked during demo structural decoding".to_string()),
        };

        let mut state = AnalyzerState::default();

        let process_event = |state: &mut AnalyzerState, event: &AnalyzerEvent| {
            use_timing_updates(state, event);
            use_player_updates(state, event);
            with_mortality_detection(state, event);
            use_scoreboard_updates(state, event);
            use_kill_streak_updates(state, event);
            use_weapon_breakdown_updates(state, event);
            use_team_score_updates(state, event);
            use_rounds_updates(state, event);
            use_chat_updates(state, event);
            use_clan_match_detection_updates(Duration::from_secs(10), state, event);
            use_pov_stats_updates(state, event);
        };

        process_event(&mut state, &AnalyzerEvent::Initialization);

        let total_frames: usize = demo.directory.entries.iter().map(|entry| entry.frames.len()).sum();
        let mut processed_frames = 0;

        for entry in &demo.directory.entries {
            for frame in &entry.frames {
                process_event(&mut state, &AnalyzerEvent::Frame(frame));
                if let FrameData::NetworkMessage(box_type) = &frame.frame_data {
                    if let MessageData::Parsed(msgs) = &box_type.1.messages {
                        for net_msg in msgs {
                            match net_msg {
                                NetMessage::EngineMessage(engine_msg) => {
                                    process_event(&mut state, &AnalyzerEvent::EngineMessage(engine_msg));
                                }
                                NetMessage::UserMessage(user_msg) => {
                                    if is_relevant_message(user_msg.name.as_ref()) {
                                        if let Ok(msg) = UserMessage::new(&user_msg.name, &user_msg.data) {
                                            process_event(&mut state, &AnalyzerEvent::UserMessage(msg));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                processed_frames += 1;
                if processed_frames % 500 == 0 || processed_frames == total_frames {
                    progress_cb(processed_frames, total_frames);
                }
            }
        }

        process_event(&mut state, &AnalyzerEvent::Finalization);

        Ok(Analysis::new(demo.into(), state))
    }

    pub fn parse_with_diagnostics(value: &[u8]) -> Result<(Self, ParseDiagnostics), String> {
        let demo_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            open_demo_from_bytes(value)
        }));
        let demo = match demo_res {
            Ok(Ok(d)) => d,
            Ok(Err(e)) => return Err(format!("Parse error: {}", e)),
            Err(_) => return Err("Parser panicked during demo structural decoding".to_string()),
        };
        
        let process_event = |state: &mut AnalyzerState, event: &AnalyzerEvent| {
            use_timing_updates(state, event);
            use_player_updates(state, event);
            with_mortality_detection(state, event);
            use_scoreboard_updates(state, event);
            use_kill_streak_updates(state, event);
            use_weapon_breakdown_updates(state, event);
            use_team_score_updates(state, event);
            use_rounds_updates(state, event);
            use_chat_updates(state, event);
            use_clan_match_detection_updates(Duration::from_secs(10), state, event);
            use_pov_stats_updates(state, event);
        };

        // 1. Unoptimized Parse (no filtering, processes every single user message)
        let start_unopt = std::time::Instant::now();
        let mut _last_live_frame_unopt = None;
        let mut state_unopt = AnalyzerState::default();
        process_event(&mut state_unopt, &AnalyzerEvent::Initialization);
        let total_frames: usize = demo.directory.entries.iter().map(|entry| entry.frames.len()).sum();
        let mut processed_frames = 0;

        for entry in &demo.directory.entries {
            for frame in &entry.frames {
                process_event(&mut state_unopt, &AnalyzerEvent::Frame(frame));
                if let FrameData::NetworkMessage(box_type) = &frame.frame_data {
                    if let MessageData::Parsed(msgs) = &box_type.1.messages {
                        for net_msg in msgs {
                            match net_msg {
                                NetMessage::EngineMessage(engine_msg) => {
                                    process_event(&mut state_unopt, &AnalyzerEvent::EngineMessage(engine_msg));
                                }
                                NetMessage::UserMessage(user_msg) => {
                                    if let Ok(msg) = UserMessage::new(&user_msg.name, &user_msg.data) {
                                        let old_live = matches!(state_unopt.clan_match_detection, ClanMatchDetection::MatchIsLive);
                                        process_event(&mut state_unopt, &AnalyzerEvent::UserMessage(msg));
                                        let new_live = matches!(state_unopt.clan_match_detection, ClanMatchDetection::MatchIsLive);
                                        if !old_live && new_live {
                                            _last_live_frame_unopt = Some(processed_frames);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                processed_frames += 1;
            }
        }
        process_event(&mut state_unopt, &AnalyzerEvent::Finalization);
        let unopt_duration = start_unopt.elapsed();

        // 2. Optimized Parse (with filtering, processes only the 14 relevant messages)
        let start_opt = std::time::Instant::now();
        let mut last_live_frame_opt = None;
        let mut state_opt = AnalyzerState::default();
        process_event(&mut state_opt, &AnalyzerEvent::Initialization);
        processed_frames = 0;

        for entry in &demo.directory.entries {
            for frame in &entry.frames {
                process_event(&mut state_opt, &AnalyzerEvent::Frame(frame));
                if let FrameData::NetworkMessage(box_type) = &frame.frame_data {
                    if let MessageData::Parsed(msgs) = &box_type.1.messages {
                        for net_msg in msgs {
                            match net_msg {
                                NetMessage::EngineMessage(engine_msg) => {
                                    process_event(&mut state_opt, &AnalyzerEvent::EngineMessage(engine_msg));
                                }
                                NetMessage::UserMessage(user_msg) => {
                                    if is_relevant_message(user_msg.name.as_ref()) {
                                        if let Ok(msg) = UserMessage::new(&user_msg.name, &user_msg.data) {
                                            let old_live = matches!(state_opt.clan_match_detection, ClanMatchDetection::MatchIsLive);
                                            process_event(&mut state_opt, &AnalyzerEvent::UserMessage(msg));
                                            let new_live = matches!(state_opt.clan_match_detection, ClanMatchDetection::MatchIsLive);
                                            if !old_live && new_live {
                                                last_live_frame_opt = Some(processed_frames);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                processed_frames += 1;
            }
        }
        process_event(&mut state_opt, &AnalyzerEvent::Finalization);
        let opt_duration = start_opt.elapsed();

        // 3. Comparison check
        let compare_result = check_states_equal(&state_unopt, &state_opt);
        let (states_matched, mismatch_reason) = match compare_result {
            Ok(_) => (true, None),
            Err(reason) => (false, Some(reason)),
        };

        let diagnostics = ParseDiagnostics {
            total_frames,
            live_frame_index: last_live_frame_opt,
            unopt_duration,
            opt_duration,
            states_matched,
            mismatch_reason,
        };

        let analysis_opt = Analysis::new(demo.into(), state_opt);
        Ok((analysis_opt, diagnostics))
    }
}

#[derive(Debug, Clone)]
pub struct ParseDiagnostics {
    pub total_frames: usize,
    pub live_frame_index: Option<usize>,
    pub unopt_duration: std::time::Duration,
    pub opt_duration: std::time::Duration,
    pub states_matched: bool,
    pub mismatch_reason: Option<String>,
}

fn check_states_equal(left: &AnalyzerState, right: &AnalyzerState) -> Result<(), String> {
    if format!("{:?}", left.clan_match_detection) != format!("{:?}", right.clan_match_detection) {
        return Err(format!("clan_match_detection mismatch: {:?} vs {:?}", left.clan_match_detection, right.clan_match_detection));
    }
    if left.current_time.real_offset != right.current_time.real_offset || left.current_time.viewdemo_offset != right.current_time.viewdemo_offset {
        return Err(format!("current_time mismatch"));
    }
    if format!("{:?}", left.team_scores) != format!("{:?}", right.team_scores) {
        return Err(format!("team_scores mismatch"));
    }
    if left.pov_player_index != right.pov_player_index {
        return Err(format!("pov_player_index mismatch: {:?} vs {:?}", left.pov_player_index, right.pov_player_index));
    }
    if left.pov_stats != right.pov_stats {
        return Err(format!("pov_stats mismatch: {:?} vs {:?}", left.pov_stats, right.pov_stats));
    }
    if left.rounds.len() != right.rounds.len() {
        return Err(format!("rounds count mismatch: {} vs {}", left.rounds.len(), right.rounds.len()));
    }
    for (i, (r_l, r_r)) in left.rounds.iter().zip(right.rounds.iter()).enumerate() {
        if format!("{:?}", r_l) != format!("{:?}", r_r) {
            return Err(format!("round {} mismatched", i));
        }
    }
    if left.players.len() != right.players.len() {
        return Err(format!("players count mismatch: {} vs {}", left.players.len(), right.players.len()));
    }
    for (i, (p_l, p_r)) in left.players.iter().zip(right.players.iter()).enumerate() {
        if p_l.id != p_r.id {
            return Err(format!("player {} ID mismatched", i));
        }
        if format!("{:?}", p_l.connection) != format!("{:?}", p_r.connection) {
            return Err(format!("player {} connection mismatched", i));
        }
        if p_l.name != p_r.name {
            return Err(format!("player {} name mismatched", i));
        }
        if p_l.team != p_r.team {
            return Err(format!("player {} team mismatched", i));
        }
        if format!("{:?}", p_l.class) != format!("{:?}", p_r.class) {
            return Err(format!("player {} class mismatched", i));
        }
        if p_l.stats != p_r.stats {
            return Err(format!("player {} stats mismatched", i));
        }
        if p_l.kill_streaks.len() != p_r.kill_streaks.len() {
            return Err(format!("player {} kill_streaks len mismatched", i));
        }
        for (j, (k_l, k_r)) in p_l.kill_streaks.iter().zip(p_r.kill_streaks.iter()).enumerate() {
            if format!("{:?}", k_l.kills) != format!("{:?}", k_r.kills) {
                return Err(format!("player {} kill_streak {} mismatched", i, j));
            }
        }
        if p_l.weapon_breakdown != p_r.weapon_breakdown {
            return Err(format!("player {} weapon_breakdown mismatched", i));
        }
        if p_l.mortality.len() != p_r.mortality.len() {
            return Err(format!("player {} mortality len mismatched", i));
        }
        for (j, (m_l, m_r)) in p_l.mortality.iter().zip(p_r.mortality.iter()).enumerate() {
            if m_l.time().real_offset != m_r.time().real_offset || 
               m_l.time().viewdemo_offset != m_r.time().viewdemo_offset || 
               m_l.mortality() != m_r.mortality() {
                return Err(format!("player {} mortality {} mismatched", i, j));
            }
        }
    }
    Ok(())
}

impl<'a> From<&'a [u8]> for Analysis {
    fn from(value: &'a [u8]) -> Self {
        Self::try_from_bytes(value).expect("Could not parse the file")
    }
}

impl AnalyzerState {
    fn find_player_by_client_index(&self, client_index: u8) -> Option<&Player> {
        self.players.iter().find(|player| match player.connection {
            Connection::Connected { client_id } => client_id == client_index,
            _ => false,
        })
    }

    fn find_player_by_client_index_mut(&mut self, client_index: u8) -> Option<&mut Player> {
        self.players
            .iter_mut()
            .find(|player| match player.connection {
                Connection::Connected { client_id } => client_id == client_index,
                _ => false,
            })
    }

    fn find_player_by_id(&self, id: &PlayerGlobalId) -> Option<&Player> {
        self.players.iter().find(|player| player.id == *id)
    }

    fn find_player_by_id_mut(&mut self, id: &PlayerGlobalId) -> Option<&mut Player> {
        self.players.iter_mut().find(|player| player.id == *id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    #[ignore]
    fn test_optimized_vs_unoptimized() {
        let paths = [
            "../demos/bb-scrim-harr-h1.dem",
            "demos/bb-scrim-harr-h1.dem",
            "../demos/bb-scrim-harr-h2.dem",
            "demos/bb-scrim-harr-h2.dem",
            "../demos/bb-scrim-railyard-h1.dem",
            "demos/bb-scrim-railyard-h1.dem",
            "../demos/bewton-playoffs-round1-armory-allied.dem",
            "demos/bewton-playoffs-round1-armory-allied.dem",
        ];
        
        let mut tested_any = false;
        
        for path in paths {
            if !std::path::Path::new(path).exists() {
                continue;
            }
            
            println!("Testing optimized vs unoptimized parsing on: {}", path);
            let file_bytes = fs::read(path).expect("failed to read demo");
            
            // Unoptimized parse
            let demo = open_demo_from_bytes(&file_bytes).unwrap();
            let mut state_unopt = AnalyzerState::default();
            let mut last_live_frame = None;
            let mut processed_frames = 0;
            let process_event = |state: &mut AnalyzerState, event: &AnalyzerEvent| {
                use_timing_updates(state, event);
                use_player_updates(state, event);
                with_mortality_detection(state, event);
                use_scoreboard_updates(state, event);
                use_kill_streak_updates(state, event);
                use_weapon_breakdown_updates(state, event);
                use_team_score_updates(state, event);
                use_rounds_updates(state, event);
                use_chat_updates(state, event);
                use_clan_match_detection_updates(Duration::from_secs(10), state, event);
                use_pov_stats_updates(state, event);
            };
            process_event(&mut state_unopt, &AnalyzerEvent::Initialization);
            for entry in &demo.directory.entries {
                for frame in &entry.frames {
                    let old_live = matches!(state_unopt.clan_match_detection, ClanMatchDetection::MatchIsLive);
                    process_event(&mut state_unopt, &AnalyzerEvent::Frame(frame));
                    if let FrameData::NetworkMessage(box_type) = &frame.frame_data {
                        if let MessageData::Parsed(msgs) = &box_type.1.messages {
                            for net_msg in msgs {
                                match net_msg {
                                    NetMessage::EngineMessage(engine_msg) => {
                                        process_event(&mut state_unopt, &AnalyzerEvent::EngineMessage(engine_msg));
                                    }
                                    NetMessage::UserMessage(user_msg) => {
                                        if let Ok(msg) = UserMessage::new(&user_msg.name, &user_msg.data) {
                                            process_event(&mut state_unopt, &AnalyzerEvent::UserMessage(msg));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let new_live = matches!(state_unopt.clan_match_detection, ClanMatchDetection::MatchIsLive);
                    if !old_live && new_live {
                        last_live_frame = Some(processed_frames);
                    }
                    processed_frames += 1;
                }
            }
            process_event(&mut state_unopt, &AnalyzerEvent::Finalization);

            // Optimized parse
            let opt_analysis = Analysis::try_from_bytes(&file_bytes).unwrap();
            let state_opt = opt_analysis.state;

            // Compare debug print representation
            let total_frames: usize = demo.directory.entries.iter().map(|entry| entry.frames.len()).sum();
            println!("  -> Total frames: {}, Live frame index: {:?} (Warmup frames skipped: {:?})", 
                total_frames, 
                last_live_frame,
                last_live_frame.unwrap_or(0)
            );
            
            assert_states_eq(&state_unopt, &state_opt);
            tested_any = true;
        }
        
        assert!(tested_any, "No demo files were found to run the comparison test!");
        println!("All existing demos match perfectly!");
    }

    fn assert_states_eq(left: &AnalyzerState, right: &AnalyzerState) {
        // Compare clan_match_detection
        assert_eq!(format!("{:?}", left.clan_match_detection), format!("{:?}", right.clan_match_detection));
        
        // Compare current_time
        assert_eq!(left.current_time.real_offset, right.current_time.real_offset);
        assert_eq!(left.current_time.viewdemo_offset, right.current_time.viewdemo_offset);
        
        // Compare team_scores
        assert_eq!(format!("{:?}", left.team_scores), format!("{:?}", right.team_scores));
        
        // Compare POV stats
        assert_eq!(left.pov_player_index, right.pov_player_index);
        assert_eq!(left.pov_stats, right.pov_stats);
        
        // Compare rounds
        assert_eq!(left.rounds.len(), right.rounds.len());
        for (i, (r_l, r_r)) in left.rounds.iter().zip(right.rounds.iter()).enumerate() {
            assert_eq!(format!("{:?}", r_l), format!("{:?}", r_r), "Round {} mismatched", i);
        }
        
        // Compare players
        assert_eq!(left.players.len(), right.players.len());
        for (i, (p_l, p_r)) in left.players.iter().zip(right.players.iter()).enumerate() {
            assert_eq!(p_l.id, p_r.id, "Player {} ID mismatched", i);
            assert_eq!(format!("{:?}", p_l.connection), format!("{:?}", p_r.connection), "Player {} connection mismatched", i);
            assert_eq!(p_l.name, p_r.name, "Player {} name mismatched", i);
            assert_eq!(p_l.team, p_r.team, "Player {} team mismatched", i);
            assert_eq!(format!("{:?}", p_l.class), format!("{:?}", p_r.class), "Player {} class mismatched", i);
            assert_eq!(p_l.stats, p_r.stats, "Player {} stats mismatched", i);
            
            // kill_streaks
            assert_eq!(p_l.kill_streaks.len(), p_r.kill_streaks.len(), "Player {} kill_streaks len mismatched", i);
            for (j, (k_l, k_r)) in p_l.kill_streaks.iter().zip(p_r.kill_streaks.iter()).enumerate() {
                assert_eq!(format!("{:?}", k_l.kills), format!("{:?}", k_r.kills), "Player {} kill_streak {} mismatched", i, j);
            }
            
            // weapon_breakdown (HashMap == is order-independent)
            assert_eq!(p_l.weapon_breakdown, p_r.weapon_breakdown, "Player {} weapon_breakdown mismatched", i);
            
            // mortality
            assert_eq!(p_l.mortality.len(), p_r.mortality.len(), "Player {} mortality len mismatched", i);
            for (j, (m_l, m_r)) in p_l.mortality.iter().zip(p_r.mortality.iter()).enumerate() {
                assert_eq!(m_l.time().real_offset, m_r.time().real_offset, "Player {} mortality {} real_offset mismatched", i, j);
                assert_eq!(m_l.time().viewdemo_offset, m_r.time().viewdemo_offset, "Player {} mortality {} viewdemo_offset mismatched", i, j);
                assert_eq!(m_l.mortality(), m_r.mortality(), "Player {} mortality {} status mismatched", i, j);
            }
        }
        
        // Compare chat messages
        assert_eq!(left.chat_messages.len(), right.chat_messages.len(), "Chat messages length mismatched");
        for (i, (c_l, c_r)) in left.chat_messages.iter().zip(right.chat_messages.iter()).enumerate() {
            assert_eq!(c_l.chat_type, c_r.chat_type, "Chat message {} type mismatched", i);
            assert_eq!(c_l.sender_name, c_r.sender_name, "Chat message {} sender mismatched", i);
            assert_eq!(c_l.sender_team, c_r.sender_team, "Chat message {} team mismatched", i);
            assert_eq!(c_l.sender_dead, c_r.sender_dead, "Chat message {} dead flag mismatched", i);
            assert_eq!(c_l.text, c_r.text, "Chat message {} text mismatched", i);
            assert_eq!(c_l.system_token, c_r.system_token, "Chat message {} system_token mismatched", i);
            assert_eq!(c_l.system_args, c_r.system_args, "Chat message {} system_args mismatched", i);
        }
    }
}
