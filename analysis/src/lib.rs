mod chat;
mod clan_match;
mod kill;
mod localization;
mod mortality;
mod player;
mod round;
mod scoreboard;
mod time;

use crate::{
    chat::use_chat_updates,
    clan_match::{ClanMatchDetection, use_clan_match_detection_updates},
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
    localization::{get_active_language, set_active_language, translate_key},
    mortality::{MortalityState, Mortality, MortalityChange},
    player::{Connection, Player, PlayerGlobalId, SteamId},
    round::Round,
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

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WeaponPovStats {
    pub bullets_fired: u32,
    pub reloads: u32,
    pub kills: u32,
    pub noscopes: u32,
    pub scoped_kills: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct AnalyzerState {
    clan_match_detection: ClanMatchDetection,
    /// Set to `true` as soon as any definitive clan-match signal is observed
    /// (ClanTimer message, WaveTime > 0, or the Reset→Start scoreboard zeroing
    /// sequence). Unlike `clan_match_detection`, this flag is never cleared, so
    /// demos recorded *after* the match went live still report correctly.
    pub clan_match_detected: bool,
    pub match_start_witnessed: bool,
    pub started_late: bool,
    pub ended_early: bool,
    pub first_time_left: Option<std::time::Duration>,
    pub last_time_left: Option<std::time::Duration>,
    pub map_changed: bool,
    pub initial_map_name: Option<String>,
    pub current_time: GameTime,

    pub frame_index: usize,
    pub players: Vec<Player>,
    pub rounds: Vec<Round>,
    pub team_scores: TeamScores,
    pub chat_messages: Vec<ChatMessage>,
    pub pov_player_index: Option<u8>,
    pub pov_stats: PovStats,
    pub hltv_name: Option<String>,
    pub allies_are_british: bool,
    pub server_name: Option<String>,
    pub server_address: Option<String>,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
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

    /// Map checksum / CRC.
    pub map_checksum: u32,
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
                                if matches!(
                                    **eng_msg,
                                    EngineMessage::SvcHltv(_) | EngineMessage::SvcDirector(_)
                                ) {
                                    is_hltv = true;
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
            }
        }
        let demo_type = if is_hltv {
            "HLTV".to_string()
        } else {
            "POV".to_string()
        };

        Self {
            demo_protocol: value.header.demo_protocol,
            map_name,
            network_protocol: value.header.network_protocol,
            playback_time,
            game_directory,
            demo_type,
            map_checksum: value.header.map_checksum,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum GameEvent {
    Kill(String, String, String), // Killer, Victim, Weapon
    ScoreUpdate(String, i32, i32), // Player, Kills, Deaths
    ServerReset,
    GameCommencing,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TimelineEvent {
    pub tick: u32,
    pub event: GameEvent,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PlayerStats {
    pub kills: i32,
    pub deaths: i32,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct Analysis {
    pub demo_info: DemoInfo,
    pub state: AnalyzerState,
    pub events: Vec<TimelineEvent>,
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
            | b"TimeLeft"
            | b"WaveTime"
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

pub fn use_time_left_updates(state: &mut AnalyzerState, event: &AnalyzerEvent) {
    if let AnalyzerEvent::UserMessage(dod::UserMessage::TimeLeft(time_left)) = event {
        let duration = time_left.0;
        if state.first_time_left.is_none() {
            state.first_time_left = Some(duration);
        }
        state.last_time_left = Some(duration);
    }
}

fn is_close_to_match_end(duration: Duration) -> bool {
    let secs = duration.as_secs();
    let targets = [900u64, 1200, 1500, 1800, 2400, 2700];
    for &target in &targets {
        if secs >= target.saturating_sub(10) && secs <= target + 30 {
            return true;
        }
    }
    false
}

fn extract_ip_port(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let n = bytes.len();
    for i in 0..n {
        if bytes[i] == b':' {
            // Read digits to the right (port)
            let mut j = i + 1;
            while j < n && bytes[j].is_ascii_digit() {
                j += 1;
            }
            let port_len = j - (i + 1);
            if port_len > 0 {
                // Read valid domain/ip characters to the left
                let mut k = i;
                while k > 0 {
                    let c = bytes[k - 1];
                    if c.is_ascii_alphanumeric() || c == b'.' || c == b'-' {
                        k -= 1;
                    } else {
                        break;
                    }
                }
                let host_str = &s[k..i];
                // Must contain at least one dot to be a valid domain or IP
                if host_str.contains('.') && !host_str.starts_with('.') && !host_str.ends_with('.') {
                    let port_str = &s[i + 1..j];
                    return Some(format!("{}:{}", host_str, port_str));
                }
            }
        }
    }
    None
}

pub fn use_general_finalization(state: &mut AnalyzerState, event: &AnalyzerEvent) {
    if let AnalyzerEvent::EngineMessage(EngineMessage::SvcServerInfo(msg)) = event {
        let hostname = String::from_utf8_lossy(&msg.hostname).trim_end_matches('\0').to_string();
        state.server_name = Some(hostname.clone());
        if let Some(addr) = extract_ip_port(&hostname) {
            state.server_address = Some(addr);
        }

        let map_name = String::from_utf8_lossy(&msg.map_file_name)
            .trim_end_matches('\0')
            .to_string();
        let clean_map = map_name
            .trim_start_matches("maps/")
            .trim_end_matches(".bsp")
            .to_string();
        if let Some(ref initial) = state.initial_map_name {
            if initial != &clean_map {
                let has_gameplay = state.rounds.iter().any(|r| matches!(r, Round::Completed { .. }))
                    || state.players.iter().any(|p| p.stats.0 > 0 || p.stats.1 > 0 || p.stats.2 > 0);
                if has_gameplay {
                    state.map_changed = true;
                } else {
                    state.initial_map_name = Some(clean_map);
                    state.players.clear();
                    state.rounds.clear();
                    state.team_scores.reset();
                    state.clan_match_detected = false;
                    state.clan_match_detection = ClanMatchDetection::WaitingForReset;
                }
            }
        } else {
            state.initial_map_name = Some(clean_map);
        }
    }

    if let AnalyzerEvent::EngineMessage(EngineMessage::SvcStuffText(msg)) = event {
        let cmd = String::from_utf8_lossy(msg.command.as_slice());
        if let Some(addr) = extract_ip_port(&cmd) {
            state.server_address = Some(addr);
        }
    }

    if let AnalyzerEvent::Frame(frame) = event {
        if let FrameData::ConsoleCommand(cmd) = &frame.frame_data {
            let cmd_str = String::from_utf8_lossy(cmd.command.as_slice());
            if let Some(addr) = extract_ip_port(&cmd_str) {
                state.server_address = Some(addr);
            }
        }
    }

    use_time_left_updates(state, event);

    if let AnalyzerEvent::Finalization = event {
        if state.is_clan_match() {
            state.started_late = !state.match_start_witnessed;
        } else {
            state.started_late = state.players.iter().any(|p| p.has_pre_demo_activity);
        }

        let match_duration = if let Some(first_round) = state.rounds.first() {
            let start = match first_round {
                Round::Active { start_time, .. } => start_time.real_offset,
                Round::Completed { start_time, .. } => start_time.real_offset,
            };
            state.current_time.real_offset.saturating_sub(start)
        } else {
            Duration::ZERO
        };

        let is_natural_end = state.map_changed
            || state.last_time_left.map_or(false, |tl| tl <= Duration::from_secs(10))
            || is_close_to_match_end(match_duration);

        state.ended_early = if is_natural_end {
            false
        } else if let Some(last_round) = state.rounds.last() {
            match last_round {
                Round::Completed { winner_stats, .. } => winner_stats.is_none(),
                _ => true,
            }
        } else {
            false
        };
    }
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
                            let entry = state
                                .pov_stats
                                .weapon_stats
                                .entry(msg.weapon.clone())
                                .or_default();
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
                    let entry = state
                        .pov_stats
                        .weapon_stats
                        .entry(active_weapon.clone())
                        .or_default();
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
                        state.pov_stats.total_damage_taken +=
                            state.pov_stats.prev_health - health_val;
                    }
                }
                state.pov_stats.prev_health = health_val;
                state.pov_stats.has_received_health = true;
            }
            UserMessage::DeathMsg(msg) => {
                if let Some(pov_idx) = state.pov_player_index {
                    let is_killer_pov =
                        msg.killer_client_index > 0 && msg.killer_client_index - 1 == pov_idx;
                    let is_victim_pov =
                        msg.victim_client_index > 0 && msg.victim_client_index - 1 == pov_idx;

                    if is_killer_pov {
                        if msg.killer_client_index == msg.victim_client_index {
                            state.pov_stats.suicides += 1;
                        } else {
                            let killer_team = state
                                .find_player_by_client_index(msg.killer_client_index - 1)
                                .and_then(|p| p.team.clone());
                            let victim_team = state
                                .find_player_by_client_index(msg.victim_client_index - 1)
                                .and_then(|p| p.team.clone());
                            if killer_team.is_some()
                                && victim_team.is_some()
                                && killer_team == victim_team
                            {
                                state.pov_stats.teamkills_committed += 1;
                            } else {
                                let entry = state
                                    .pov_stats
                                    .weapon_stats
                                    .entry(msg.weapon.clone())
                                    .or_default();
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
                            let killer_team = state
                                .find_player_by_client_index(msg.killer_client_index - 1)
                                .and_then(|p| p.team.clone());
                            let victim_team = state
                                .find_player_by_client_index(msg.victim_client_index - 1)
                                .and_then(|p| p.team.clone());
                            if killer_team.is_some()
                                && victim_team.is_some()
                                && killer_team == victim_team
                            {
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
fn check_and_promote_british(state: &mut AnalyzerState) {
    if state.allies_are_british {
        for player in &mut state.players {
            if player.team == Some(Team::Allies) {
                player.team = Some(Team::British);
            }
        }
    } else {
        let any_british = state.players.iter().any(|p| {
            p.class.as_ref().map(|c| c.is_british()).unwrap_or(false)
        });
        if any_british {
            state.allies_are_british = true;
            for player in &mut state.players {
                if player.team == Some(Team::Allies) {
                    player.team = Some(Team::British);
                }
            }
            state.team_scores.convert_allies_to_british();
            for round in &mut state.rounds {
                if let Round::Completed { winner_stats: Some((winner_team, _)), .. } = round {
                    if *winner_team == Team::Allies {
                        *winner_team = Team::British;
                    }
                }
            }
            for chat in &mut state.chat_messages {
                if chat.sender_team == Some(Team::Allies) {
                    chat.sender_team = Some(Team::British);
                }
            }
        }
    }
}

impl Analysis {
    fn new(demo_info: DemoInfo, state: AnalyzerState, events: Vec<TimelineEvent>) -> Self {
        Self { demo_info, state, events }
    }

    pub fn build_scoreboard(&self) -> std::collections::HashMap<String, PlayerStats> {
        let mut scoreboard = std::collections::HashMap::new();
        for timeline_event in &self.events {
            match &timeline_event.event {
                GameEvent::ServerReset | GameEvent::GameCommencing => {
                    scoreboard.clear();
                }
                GameEvent::Kill(killer, victim, _weapon) => {
                    if !killer.is_empty() {
                        let stats = scoreboard.entry(killer.clone()).or_insert(PlayerStats::default());
                        stats.kills += 1;
                    }
                    if !victim.is_empty() {
                        let stats = scoreboard.entry(victim.clone()).or_insert(PlayerStats::default());
                        stats.deaths += 1;
                    }
                }
                GameEvent::ScoreUpdate(player, kills, deaths) => {
                    let stats = scoreboard.entry(player.clone()).or_insert(PlayerStats::default());
                    if *kills > stats.kills {
                        stats.kills = *kills;
                    }
                    if *deaths > stats.deaths {
                        stats.deaths = *deaths;
                    }
                }
            }
        }
        scoreboard
    }

    pub fn try_from_bytes(value: &[u8]) -> Result<Self, String> {
        Self::try_from_bytes_with_progress(value, |_, _| {})
    }

    pub fn try_from_bytes_with_progress<F>(value: &[u8], mut progress_cb: F) -> Result<Self, String>
    where
        F: FnMut(usize, usize),
    {
        let demo_res =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| open_demo_from_bytes(value)));
        let demo = match demo_res {
            Ok(Ok(d)) => d,
            Ok(Err(e)) => return Err(format!("Parse error: {}", e)),
            Err(_) => return Err("Parser panicked during demo structural decoding".to_string()),
        };

        let mut state = AnalyzerState::default();
        let mut events = Vec::new();
        let mut client_id_to_name = std::collections::HashMap::new();

        let process_event = |state: &mut AnalyzerState, event: &AnalyzerEvent| {
            if state.map_changed && !matches!(event, AnalyzerEvent::Finalization) {
                return;
            }
            use_timing_updates(state, event);
            use_player_updates(state, event);
            with_mortality_detection(state, event);
            use_scoreboard_updates(state, event);
            use_kill_streak_updates(state, event);
            use_weapon_breakdown_updates(state, event);
            use_team_score_updates(state, event);
            use_rounds_updates(state, event);
            use_chat_updates(state, event);
            use_clan_match_detection_updates(Duration::from_secs(30), state, event);
            use_pov_stats_updates(state, event);
            use_general_finalization(state, event);
            check_and_promote_british(state);
        };

        process_event(&mut state, &AnalyzerEvent::Initialization);

        let total_frames: usize = demo
            .directory
            .entries
            .iter()
            .map(|entry| entry.frames.len())
            .sum();
        let mut processed_frames = 0;

        for entry in &demo.directory.entries {
            for frame in &entry.frames {
                process_event(&mut state, &AnalyzerEvent::Frame(frame));
                if let FrameData::NetworkMessage(box_type) = &frame.frame_data {
                    if let MessageData::Parsed(msgs) = &box_type.1.messages {
                        for net_msg in msgs {
                            match net_msg {
                                NetMessage::EngineMessage(engine_msg) => {
                                    process_event(
                                        &mut state,
                                        &AnalyzerEvent::EngineMessage(engine_msg),
                                    );

                                    if let EngineMessage::SvcUpdateUserInfo(user_info) = &**engine_msg {
                                        let fields = user_info.user_info.to_str()
                                            .map(|s| s.trim_matches(['\0', '\\']).split('\\').collect::<Vec<_>>())
                                            .unwrap_or_default()
                                            .chunks_exact(2)
                                            .fold(std::collections::HashMap::new(), |mut map, chunk| {
                                                if let [key, value] = chunk {
                                                    map.insert(*key, *value);
                                                }
                                                map
                                            });
                                        if let Some(name) = fields.get("name") {
                                            client_id_to_name.insert(user_info.index, name.to_string());
                                        }
                                    }
                                }
                                NetMessage::UserMessage(user_msg) => {
                                    if is_relevant_message(user_msg.name.as_ref()) {
                                        if let Ok(msg) =
                                            UserMessage::new(&user_msg.name, &user_msg.data)
                                        {
                                            match &msg {
                                                UserMessage::DeathMsg(death_msg) => {
                                                    let killer_name = if death_msg.killer_client_index > 0 {
                                                        client_id_to_name.get(&(death_msg.killer_client_index - 1)).cloned().unwrap_or_default()
                                                    } else {
                                                        String::new()
                                                    };
                                                    let victim_name = client_id_to_name.get(&(death_msg.victim_client_index - 1)).cloned().unwrap_or_default();
                                                    let weapon_name = format!("{:?}", death_msg.weapon);
                                                    events.push(TimelineEvent {
                                                        tick: processed_frames as u32,
                                                        event: GameEvent::Kill(killer_name, victim_name, weapon_name),
                                                    });
                                                }
                                                UserMessage::ScoreInfo(score_info) => {
                                                    if let Some(player_name) = client_id_to_name.get(&(score_info.client_index - 1)).cloned() {
                                                        events.push(TimelineEvent {
                                                            tick: processed_frames as u32,
                                                            event: GameEvent::ScoreUpdate(player_name, score_info.kills as i32, score_info.deaths as i32),
                                                        });
                                                    }
                                                }
                                                UserMessage::ScoreInfoLong(score_info_long) => {
                                                    if let Some(player_name) = client_id_to_name.get(&(score_info_long.client_index - 1)).cloned() {
                                                        events.push(TimelineEvent {
                                                            tick: processed_frames as u32,
                                                            event: GameEvent::ScoreUpdate(player_name, score_info_long.frags as i32, score_info_long.deaths as i32),
                                                        });
                                                    }
                                                }
                                                UserMessage::ScoreShort(score_short) => {
                                                    if let Some(player_name) = client_id_to_name.get(&(score_short.client_index - 1)).cloned() {
                                                        events.push(TimelineEvent {
                                                            tick: processed_frames as u32,
                                                            event: GameEvent::ScoreUpdate(player_name, score_short.kills as i32, score_short.deaths as i32),
                                                        });
                                                    }
                                                }
                                                UserMessage::RoundState(dod::RoundState::Reset) => {
                                                    events.push(TimelineEvent {
                                                        tick: processed_frames as u32,
                                                        event: GameEvent::ServerReset,
                                                    });
                                                }
                                                UserMessage::TextMsg(text_msg) => {
                                                    let is_commencing = text_msg.text.contains("#Game_Commencing")
                                                        || text_msg.arg1.as_ref().map_or(false, |s| s.contains("#Game_Commencing"))
                                                        || text_msg.arg2.as_ref().map_or(false, |s| s.contains("#Game_Commencing"))
                                                        || text_msg.arg3.as_ref().map_or(false, |s| s.contains("#Game_Commencing"))
                                                        || text_msg.arg4.as_ref().map_or(false, |s| s.contains("#Game_Commencing"));
                                                    if is_commencing {
                                                        events.push(TimelineEvent {
                                                            tick: processed_frames as u32,
                                                            event: GameEvent::GameCommencing,
                                                        });
                                                    }
                                                }
                                                _ => {}
                                            }

                                            process_event(
                                                &mut state,
                                                &AnalyzerEvent::UserMessage(msg),
                                            );
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

        let analysis = Analysis::new(demo.into(), state, events);
        let scoreboard = analysis.build_scoreboard();
        let mut final_analysis = analysis;
        for player in &mut final_analysis.state.players {
            if let Some(stats) = scoreboard.get(&player.name) {
                player.stats.1 = stats.kills;
                player.stats.2 = stats.deaths;
            }
        }

        Ok(final_analysis)
    }

    pub fn parse_with_diagnostics(value: &[u8]) -> Result<(Self, ParseDiagnostics), String> {
        let demo_res =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| open_demo_from_bytes(value)));
        let demo = match demo_res {
            Ok(Ok(d)) => d,
            Ok(Err(e)) => return Err(format!("Parse error: {}", e)),
            Err(_) => return Err("Parser panicked during demo structural decoding".to_string()),
        };

        let process_event = |state: &mut AnalyzerState, event: &AnalyzerEvent| {
            if state.map_changed && !matches!(event, AnalyzerEvent::Finalization) {
                return;
            }
            use_timing_updates(state, event);
            use_player_updates(state, event);
            with_mortality_detection(state, event);
            use_scoreboard_updates(state, event);
            use_kill_streak_updates(state, event);
            use_weapon_breakdown_updates(state, event);
            use_team_score_updates(state, event);
            use_rounds_updates(state, event);
            use_chat_updates(state, event);
            use_clan_match_detection_updates(Duration::from_secs(30), state, event);
            use_pov_stats_updates(state, event);
            use_general_finalization(state, event);
            check_and_promote_british(state);
        };

        // 1. Unoptimized Parse (no filtering, processes every single user message)
        let start_unopt = std::time::Instant::now();
        let mut _last_live_frame_unopt = None;
        let mut state_unopt = AnalyzerState::default();
        process_event(&mut state_unopt, &AnalyzerEvent::Initialization);
        let total_frames: usize = demo
            .directory
            .entries
            .iter()
            .map(|entry| entry.frames.len())
            .sum();
        let mut processed_frames = 0;

        for entry in &demo.directory.entries {
            for frame in &entry.frames {
                process_event(&mut state_unopt, &AnalyzerEvent::Frame(frame));
                if let FrameData::NetworkMessage(box_type) = &frame.frame_data {
                    if let MessageData::Parsed(msgs) = &box_type.1.messages {
                        for net_msg in msgs {
                            match net_msg {
                                NetMessage::EngineMessage(engine_msg) => {
                                    process_event(
                                        &mut state_unopt,
                                        &AnalyzerEvent::EngineMessage(engine_msg),
                                    );
                                }
                                NetMessage::UserMessage(user_msg) => {
                                    if let Ok(msg) =
                                        UserMessage::new(&user_msg.name, &user_msg.data)
                                    {
                                        let old_live = matches!(
                                            state_unopt.clan_match_detection,
                                            ClanMatchDetection::MatchIsLive
                                        );
                                        process_event(
                                            &mut state_unopt,
                                            &AnalyzerEvent::UserMessage(msg),
                                        );
                                        let new_live = matches!(
                                            state_unopt.clan_match_detection,
                                            ClanMatchDetection::MatchIsLive
                                        );
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
        let mut events = Vec::new();
        let mut client_id_to_name = std::collections::HashMap::new();

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
                                    process_event(
                                        &mut state_opt,
                                        &AnalyzerEvent::EngineMessage(engine_msg),
                                    );

                                    if let EngineMessage::SvcUpdateUserInfo(user_info) = &**engine_msg {
                                        let fields = user_info.user_info.to_str()
                                            .map(|s| s.trim_matches(['\0', '\\']).split('\\').collect::<Vec<_>>())
                                            .unwrap_or_default()
                                            .chunks_exact(2)
                                            .fold(std::collections::HashMap::new(), |mut map, chunk| {
                                                if let [key, value] = chunk {
                                                    map.insert(*key, *value);
                                                }
                                                map
                                            });
                                        if let Some(name) = fields.get("name") {
                                            client_id_to_name.insert(user_info.index, name.to_string());
                                        }
                                    }
                                }
                                NetMessage::UserMessage(user_msg) => {
                                    if is_relevant_message(user_msg.name.as_ref()) {
                                        if let Ok(msg) =
                                            UserMessage::new(&user_msg.name, &user_msg.data)
                                        {
                                            let old_live = matches!(
                                                state_opt.clan_match_detection,
                                                ClanMatchDetection::MatchIsLive
                                            );
                                            match &msg {
                                                UserMessage::DeathMsg(death_msg) => {
                                                    let killer_name = if death_msg.killer_client_index > 0 {
                                                        client_id_to_name.get(&(death_msg.killer_client_index - 1)).cloned().unwrap_or_default()
                                                    } else {
                                                        String::new()
                                                    };
                                                    let victim_name = client_id_to_name.get(&(death_msg.victim_client_index - 1)).cloned().unwrap_or_default();
                                                    let weapon_name = format!("{:?}", death_msg.weapon);
                                                    events.push(TimelineEvent {
                                                        tick: processed_frames as u32,
                                                        event: GameEvent::Kill(killer_name, victim_name, weapon_name),
                                                    });
                                                }
                                                UserMessage::ScoreInfo(score_info) => {
                                                    if let Some(player_name) = client_id_to_name.get(&(score_info.client_index - 1)).cloned() {
                                                        events.push(TimelineEvent {
                                                            tick: processed_frames as u32,
                                                            event: GameEvent::ScoreUpdate(player_name, score_info.kills as i32, score_info.deaths as i32),
                                                        });
                                                    }
                                                }
                                                UserMessage::ScoreInfoLong(score_info_long) => {
                                                    if let Some(player_name) = client_id_to_name.get(&(score_info_long.client_index - 1)).cloned() {
                                                        events.push(TimelineEvent {
                                                            tick: processed_frames as u32,
                                                            event: GameEvent::ScoreUpdate(player_name, score_info_long.frags as i32, score_info_long.deaths as i32),
                                                        });
                                                    }
                                                }
                                                UserMessage::ScoreShort(score_short) => {
                                                    if let Some(player_name) = client_id_to_name.get(&(score_short.client_index - 1)).cloned() {
                                                        events.push(TimelineEvent {
                                                            tick: processed_frames as u32,
                                                            event: GameEvent::ScoreUpdate(player_name, score_short.kills as i32, score_short.deaths as i32),
                                                        });
                                                    }
                                                }
                                                UserMessage::RoundState(dod::RoundState::Reset) => {
                                                    events.push(TimelineEvent {
                                                        tick: processed_frames as u32,
                                                        event: GameEvent::ServerReset,
                                                    });
                                                }
                                                UserMessage::TextMsg(text_msg) => {
                                                    let is_commencing = text_msg.text.contains("#Game_Commencing")
                                                        || text_msg.arg1.as_ref().map_or(false, |s| s.contains("#Game_Commencing"))
                                                        || text_msg.arg2.as_ref().map_or(false, |s| s.contains("#Game_Commencing"))
                                                        || text_msg.arg3.as_ref().map_or(false, |s| s.contains("#Game_Commencing"))
                                                        || text_msg.arg4.as_ref().map_or(false, |s| s.contains("#Game_Commencing"));
                                                    if is_commencing {
                                                        events.push(TimelineEvent {
                                                            tick: processed_frames as u32,
                                                            event: GameEvent::GameCommencing,
                                                        });
                                                    }
                                                }
                                                _ => {}
                                            }

                                            process_event(
                                                &mut state_opt,
                                                &AnalyzerEvent::UserMessage(msg),
                                            );
                                            let new_live = matches!(
                                                state_opt.clan_match_detection,
                                                ClanMatchDetection::MatchIsLive
                                            );
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

        let analysis = Analysis::new(demo.into(), state_opt, events);
        let scoreboard = analysis.build_scoreboard();
        let mut final_analysis = analysis;
        for player in &mut final_analysis.state.players {
            if let Some(stats) = scoreboard.get(&player.name) {
                player.stats.1 = stats.kills;
                player.stats.2 = stats.deaths;
            }
        }
        Ok((final_analysis, diagnostics))
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
        return Err(format!(
            "clan_match_detection mismatch: {:?} vs {:?}",
            left.clan_match_detection, right.clan_match_detection
        ));
    }
    if left.clan_match_detected != right.clan_match_detected {
        return Err(format!(
            "clan_match_detected mismatch: {:?} vs {:?}",
            left.clan_match_detected, right.clan_match_detected
        ));
    }
    if left.match_start_witnessed != right.match_start_witnessed {
        return Err(format!(
            "match_start_witnessed mismatch: {:?} vs {:?}",
            left.match_start_witnessed, right.match_start_witnessed
        ));
    }
    if left.started_late != right.started_late {
        return Err(format!(
            "started_late mismatch: {:?} vs {:?}",
            left.started_late, right.started_late
        ));
    }
    if left.ended_early != right.ended_early {
        return Err(format!(
            "ended_early mismatch: {:?} vs {:?}",
            left.ended_early, right.ended_early
        ));
    }
    if left.first_time_left != right.first_time_left {
        return Err(format!(
            "first_time_left mismatch: {:?} vs {:?}",
            left.first_time_left, right.first_time_left
        ));
    }
    if left.last_time_left != right.last_time_left {
        return Err(format!(
            "last_time_left mismatch: {:?} vs {:?}",
            left.last_time_left, right.last_time_left
        ));
    }
    if left.map_changed != right.map_changed {
        return Err(format!(
            "map_changed mismatch: {:?} vs {:?}",
            left.map_changed, right.map_changed
        ));
    }
    if left.initial_map_name != right.initial_map_name {
        return Err(format!(
            "initial_map_name mismatch: {:?} vs {:?}",
            left.initial_map_name, right.initial_map_name
        ));
    }
    if left.current_time.real_offset != right.current_time.real_offset
        || left.current_time.viewdemo_offset != right.current_time.viewdemo_offset
    {
        return Err(format!("current_time mismatch"));
    }
    if format!("{:?}", left.team_scores) != format!("{:?}", right.team_scores) {
        return Err(format!("team_scores mismatch"));
    }
    if left.server_name != right.server_name {
        return Err(format!(
            "server_name mismatch: {:?} vs {:?}",
            left.server_name, right.server_name
        ));
    }
    if left.server_address != right.server_address {
        return Err(format!(
            "server_address mismatch: {:?} vs {:?}",
            left.server_address, right.server_address
        ));
    }
    if left.pov_player_index != right.pov_player_index {
        return Err(format!(
            "pov_player_index mismatch: {:?} vs {:?}",
            left.pov_player_index, right.pov_player_index
        ));
    }
    if left.pov_stats != right.pov_stats {
        return Err(format!(
            "pov_stats mismatch: {:?} vs {:?}",
            left.pov_stats, right.pov_stats
        ));
    }
    if left.rounds.len() != right.rounds.len() {
        return Err(format!(
            "rounds count mismatch: {} vs {}",
            left.rounds.len(),
            right.rounds.len()
        ));
    }
    for (i, (r_l, r_r)) in left.rounds.iter().zip(right.rounds.iter()).enumerate() {
        if format!("{:?}", r_l) != format!("{:?}", r_r) {
            return Err(format!("round {} mismatched", i));
        }
    }
    if left.players.len() != right.players.len() {
        return Err(format!(
            "players count mismatch: {} vs {}",
            left.players.len(),
            right.players.len()
        ));
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
        if p_l.stats_seeded != p_r.stats_seeded {
            return Err(format!("player {} stats_seeded mismatched", i));
        }
        if p_l.has_pre_demo_activity != p_r.has_pre_demo_activity {
            return Err(format!("player {} has_pre_demo_activity mismatched", i));
        }
        if p_l.has_reconnected != p_r.has_reconnected {
            return Err(format!("player {} has_reconnected mismatched", i));
        }
        if p_l.kill_streaks.len() != p_r.kill_streaks.len() {
            return Err(format!("player {} kill_streaks len mismatched", i));
        }
        for (j, (k_l, k_r)) in p_l
            .kill_streaks
            .iter()
            .zip(p_r.kill_streaks.iter())
            .enumerate()
        {
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
            if m_l.time().real_offset != m_r.time().real_offset
                || m_l.time().viewdemo_offset != m_r.time().viewdemo_offset
                || m_l.mortality() != m_r.mortality()
            {
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
    pub fn is_clan_match(&self) -> bool {
        self.clan_match_detected
    }

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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DemoFingerprint {
    pub map_name: String,
    pub server_ip: String,
    pub player_roster_hash: u64,
    pub event_signature: Vec<String>,
}

pub fn parse_fingerprint(bytes: &[u8]) -> Result<(String, String, u64, Vec<String>, Option<String>), std::io::Error> {
    let (mut input, header) = dem::demo_parser::parse_header(bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    let map_name = String::from_utf8_lossy(header.map_name.as_slice())
        .trim_end_matches('\0')
        .trim_start_matches("maps/")
        .trim_end_matches(".bsp")
        .to_string()
        .to_lowercase();

    let mut match_started = false;
    let mut frames_parsed: u32 = 0;
    let patience_limit: u32 = 15_000;
    let mut client_slot: Option<u8> = None;
    let mut recorder_id: Option<String> = None;
    let mut roster_accumulator: Vec<String> = Vec::new();
    let mut event_signature: Vec<String> = Vec::with_capacity(10);
    let mut server_ip: Option<String> = None;

    let mut client_id_to_player_id: std::collections::HashMap<u8, String> = std::collections::HashMap::new();

    let aux = dem::types::Aux::new2();

    while !input.is_empty() {
        match dem::demo_parser::parse_frame(input, dem::types::MessageDataParseMode::Parse, aux.clone()) {
            Ok((next_input, frame)) => {
                input = next_input;
                frames_parsed += 1;

                if let dem::types::FrameData::NetworkMessage(box_type) = &frame.frame_data {
                    if let dem::types::MessageData::Parsed(msgs) = &box_type.1.messages {
                        for net_msg in msgs {
                            match net_msg {
                                dem::types::NetMessage::EngineMessage(engine_msg) => {
                                    match &**engine_msg {
                                        dem::types::EngineMessage::SvcServerInfo(info) => {
                                            let hostname = String::from_utf8_lossy(&info.hostname)
                                                .trim_end_matches('\0')
                                                .to_string();
                                            if let Some(addr) = extract_ip_port(&hostname) {
                                                server_ip = Some(addr);
                                            }
                                            client_slot = Some(info.player_index);
                                        }
                                        dem::types::EngineMessage::SvcUpdateUserInfo(user_info) => {
                                            let fields = user_info
                                                .user_info
                                                .to_str()
                                                .map(|s| s.trim_matches(['\0', '\\']).split('\\').collect::<Vec<_>>())
                                                .unwrap_or_default()
                                                .chunks_exact(2)
                                                .fold(std::collections::HashMap::new(), |mut map, chunk| {
                                                    if let [key, value] = chunk {
                                                        map.insert(*key, *value);
                                                    }
                                                    map
                                                });

                                            if fields.is_empty() {
                                                client_id_to_player_id.remove(&user_info.index);
                                                continue;
                                            }

                                            // Skip HLTV slots from roster
                                            if let Some(&"1") = fields.get("*hltv") {
                                                continue;
                                            }

                                            let id_opt = fields
                                                .get("*sid")
                                                .map(|s| s.to_string())
                                                .or_else(|| fields.get("*fid").map(|fid| format!("PLAYER_{fid}")))
                                                .or_else(|| fields.get("name").map(|n| n.to_string()));

                                            if let Some(id) = id_opt {
                                                if Some(user_info.index) == client_slot {
                                                    recorder_id = Some(id.clone());
                                                }
                                                client_id_to_player_id.insert(user_info.index, id.clone());
                                                if !roster_accumulator.contains(&id) {
                                                    roster_accumulator.push(id);
                                                }
                                            }
                                        }
                                        dem::types::EngineMessage::SvcStuffText(stuff) => {
                                            let cmd = String::from_utf8_lossy(stuff.command.as_slice());
                                            if let Some(addr) = extract_ip_port(&cmd) {
                                                server_ip = Some(addr);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                dem::types::NetMessage::UserMessage(user_msg) => {
                                    if is_relevant_message(user_msg.name.as_ref()) {
                                        if let Ok(msg) = dod::UserMessage::new(&user_msg.name, &user_msg.data) {
                                            match msg {
                                                dod::UserMessage::TextMsg(text_msg) => {
                                                    let is_commencing = text_msg.text.contains("#Game_Commencing")
                                                        || text_msg.arg1.as_ref().map_or(false, |s| s.contains("#Game_Commencing"))
                                                        || text_msg.arg2.as_ref().map_or(false, |s| s.contains("#Game_Commencing"))
                                                        || text_msg.arg3.as_ref().map_or(false, |s| s.contains("#Game_Commencing"))
                                                        || text_msg.arg4.as_ref().map_or(false, |s| s.contains("#Game_Commencing"));
                                                    if is_commencing {
                                                        match_started = true;
                                                        event_signature.clear();
                                                    }
                                                }
                                                dod::UserMessage::RoundState(dod::RoundState::Reset) => {
                                                    match_started = true;
                                                    event_signature.clear();
                                                }
                                                dod::UserMessage::DeathMsg(death) => {
                                                    if match_started {
                                                        let killer_id = if death.killer_client_index == 0 {
                                                            "world".to_string()
                                                        } else {
                                                            client_id_to_player_id.get(&(death.killer_client_index - 1))
                                                                .cloned()
                                                                .unwrap_or_else(|| format!("slot_{}", death.killer_client_index - 1))
                                                        };
                                                        let victim_id = client_id_to_player_id.get(&(death.victim_client_index - 1))
                                                            .cloned()
                                                            .unwrap_or_else(|| format!("slot_{}", death.victim_client_index - 1));

                                                        let event_str = format!("{}>{}:{:?}", killer_id, victim_id, death.weapon);
                                                        event_signature.push(event_str);
                                                    }
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Check patience limit fail-safe
                if frames_parsed > patience_limit && !match_started {
                    match_started = true;
                }

                // Check early exit condition
                if event_signature.len() == 10 {
                    break;
                }
            }
            Err(_) => {
                break;
            }
        }
    }

    roster_accumulator.sort();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::Hash;
    roster_accumulator.hash(&mut hasher);
    let roster_hash = std::hash::Hasher::finish(&hasher);

    Ok((
        map_name,
        server_ip.unwrap_or_default(),
        roster_hash,
        event_signature,
        recorder_id,
    ))
}

pub fn extract_match_fingerprint(bytes: &[u8]) -> Result<DemoFingerprint, String> {
    match parse_fingerprint(bytes) {
        Ok((map_name, server_ip, player_roster_hash, event_signature, _recorder_id)) => {
            Ok(DemoFingerprint {
                map_name,
                server_ip,
                player_roster_hash,
                event_signature,
            })
        }
        Err(e) => Err(e.to_string()),
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
                if state.map_changed && !matches!(event, AnalyzerEvent::Finalization) {
                    return;
                }
                use_timing_updates(state, event);
                use_player_updates(state, event);
                with_mortality_detection(state, event);
                use_scoreboard_updates(state, event);
                use_kill_streak_updates(state, event);
                use_weapon_breakdown_updates(state, event);
                use_team_score_updates(state, event);
                use_rounds_updates(state, event);
                use_chat_updates(state, event);
                use_clan_match_detection_updates(Duration::from_secs(30), state, event);
                use_pov_stats_updates(state, event);
                use_general_finalization(state, event);
            };
            process_event(&mut state_unopt, &AnalyzerEvent::Initialization);
            for entry in &demo.directory.entries {
                for frame in &entry.frames {
                    let old_live = matches!(
                        state_unopt.clan_match_detection,
                        ClanMatchDetection::MatchIsLive
                    );
                    process_event(&mut state_unopt, &AnalyzerEvent::Frame(frame));
                    if let FrameData::NetworkMessage(box_type) = &frame.frame_data {
                        if let MessageData::Parsed(msgs) = &box_type.1.messages {
                            for net_msg in msgs {
                                match net_msg {
                                    NetMessage::EngineMessage(engine_msg) => {
                                        process_event(
                                            &mut state_unopt,
                                            &AnalyzerEvent::EngineMessage(engine_msg),
                                        );
                                    }
                                    NetMessage::UserMessage(user_msg) => {
                                        if let Ok(msg) =
                                            UserMessage::new(&user_msg.name, &user_msg.data)
                                        {
                                            process_event(
                                                &mut state_unopt,
                                                &AnalyzerEvent::UserMessage(msg),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let new_live = matches!(
                        state_unopt.clan_match_detection,
                        ClanMatchDetection::MatchIsLive
                    );
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
            let total_frames: usize = demo
                .directory
                .entries
                .iter()
                .map(|entry| entry.frames.len())
                .sum();
            println!(
                "  -> Total frames: {}, Live frame index: {:?} (Warmup frames skipped: {:?})",
                total_frames,
                last_live_frame,
                last_live_frame.unwrap_or(0)
            );

            assert_states_eq(&state_unopt, &state_opt);
            tested_any = true;
        }

        assert!(
            tested_any,
            "No demo files were found to run the comparison test!"
        );
        println!("All existing demos match perfectly!");
    }

    fn assert_states_eq(left: &AnalyzerState, right: &AnalyzerState) {
        // Compare clan_match_detection
        assert_eq!(
            format!("{:?}", left.clan_match_detection),
            format!("{:?}", right.clan_match_detection)
        );

        // Compare current_time
        assert_eq!(
            left.current_time.real_offset,
            right.current_time.real_offset
        );
        assert_eq!(
            left.current_time.viewdemo_offset,
            right.current_time.viewdemo_offset
        );

        assert_eq!(left.match_start_witnessed, right.match_start_witnessed);
        assert_eq!(left.started_late, right.started_late);
        assert_eq!(left.ended_early, right.ended_early);
        assert_eq!(left.first_time_left, right.first_time_left);
        assert_eq!(left.last_time_left, right.last_time_left);
        assert_eq!(left.map_changed, right.map_changed);
        assert_eq!(left.initial_map_name, right.initial_map_name);

        // Compare team_scores
        assert_eq!(
            format!("{:?}", left.team_scores),
            format!("{:?}", right.team_scores)
        );

        // Compare POV stats
        assert_eq!(left.pov_player_index, right.pov_player_index);
        assert_eq!(left.pov_stats, right.pov_stats);

        // Compare rounds
        assert_eq!(left.rounds.len(), right.rounds.len());
        for (i, (r_l, r_r)) in left.rounds.iter().zip(right.rounds.iter()).enumerate() {
            assert_eq!(
                format!("{:?}", r_l),
                format!("{:?}", r_r),
                "Round {} mismatched",
                i
            );
        }

        // Compare players
        assert_eq!(left.players.len(), right.players.len());
        for (i, (p_l, p_r)) in left.players.iter().zip(right.players.iter()).enumerate() {
            assert_eq!(p_l.id, p_r.id, "Player {} ID mismatched", i);
            assert_eq!(
                format!("{:?}", p_l.connection),
                format!("{:?}", p_r.connection),
                "Player {} connection mismatched",
                i
            );
            assert_eq!(p_l.name, p_r.name, "Player {} name mismatched", i);
            assert_eq!(p_l.team, p_r.team, "Player {} team mismatched", i);
            assert_eq!(
                format!("{:?}", p_l.class),
                format!("{:?}", p_r.class),
                "Player {} class mismatched",
                i
            );
            assert_eq!(p_l.stats, p_r.stats, "Player {} stats mismatched", i);
            assert_eq!(p_l.stats_seeded, p_r.stats_seeded, "Player {} stats_seeded mismatched", i);
            assert_eq!(p_l.has_pre_demo_activity, p_r.has_pre_demo_activity, "Player {} has_pre_demo_activity mismatched", i);
            assert_eq!(p_l.has_reconnected, p_r.has_reconnected, "Player {} has_reconnected mismatched", i);

            // kill_streaks
            assert_eq!(
                p_l.kill_streaks.len(),
                p_r.kill_streaks.len(),
                "Player {} kill_streaks len mismatched",
                i
            );
            for (j, (k_l, k_r)) in p_l
                .kill_streaks
                .iter()
                .zip(p_r.kill_streaks.iter())
                .enumerate()
            {
                assert_eq!(
                    format!("{:?}", k_l.kills),
                    format!("{:?}", k_r.kills),
                    "Player {} kill_streak {} mismatched",
                    i,
                    j
                );
            }

            // weapon_breakdown (HashMap == is order-independent)
            assert_eq!(
                p_l.weapon_breakdown, p_r.weapon_breakdown,
                "Player {} weapon_breakdown mismatched",
                i
            );

            // mortality
            assert_eq!(
                p_l.mortality.len(),
                p_r.mortality.len(),
                "Player {} mortality len mismatched",
                i
            );
            for (j, (m_l, m_r)) in p_l.mortality.iter().zip(p_r.mortality.iter()).enumerate() {
                assert_eq!(
                    m_l.time().real_offset,
                    m_r.time().real_offset,
                    "Player {} mortality {} real_offset mismatched",
                    i,
                    j
                );
                assert_eq!(
                    m_l.time().viewdemo_offset,
                    m_r.time().viewdemo_offset,
                    "Player {} mortality {} viewdemo_offset mismatched",
                    i,
                    j
                );
                assert_eq!(
                    m_l.mortality(),
                    m_r.mortality(),
                    "Player {} mortality {} status mismatched",
                    i,
                    j
                );
            }
        }

        // Compare chat messages
        assert_eq!(
            left.chat_messages.len(),
            right.chat_messages.len(),
            "Chat messages length mismatched"
        );
        for (i, (c_l, c_r)) in left
            .chat_messages
            .iter()
            .zip(right.chat_messages.iter())
            .enumerate()
        {
            assert_eq!(
                c_l.chat_type, c_r.chat_type,
                "Chat message {} type mismatched",
                i
            );
            assert_eq!(
                c_l.sender_name, c_r.sender_name,
                "Chat message {} sender mismatched",
                i
            );
            assert_eq!(
                c_l.sender_team, c_r.sender_team,
                "Chat message {} team mismatched",
                i
            );
            assert_eq!(
                c_l.sender_dead, c_r.sender_dead,
                "Chat message {} dead flag mismatched",
                i
            );
            assert_eq!(c_l.text, c_r.text, "Chat message {} text mismatched", i);
            assert_eq!(
                c_l.system_token, c_r.system_token,
                "Chat message {} system_token mismatched",
                i
            );
            assert_eq!(
                c_l.system_args, c_r.system_args,
                "Chat message {} system_args mismatched",
                i
            );
        }
    }

    #[test]
    fn test_reconnect_stats_accumulation() {
        let mut player = Player::new_mock(1, "STEAM_0:0:99999");

        // Session 1: Player gets 5 kills, 2 deaths, 10 score
        player.update_session_stats(10, 5, 2);
        assert_eq!(player.stats, (10, 5, 2));

        // Player disconnects
        player.needs_reconnect_sync = true;

        // Player reconnects. Server resets stats, so first update is 0 kills, 0 deaths, 0 score.
        player.update_session_stats(0, 0, 0);

        // Assert old session accumulated, active session reset to 0, total displayed stats are still 5 kills, 2 deaths, 10 score
        assert_eq!(player.accumulated_stats, (10, 5, 2));
        assert_eq!(player.session_stats, (0, 0, 0));
        assert_eq!(player.stats, (10, 5, 2));

        // Player gets a kill and a death in Session 2
        player.update_session_stats(12, 1, 1);
        assert_eq!(player.accumulated_stats, (10, 5, 2));
        assert_eq!(player.session_stats, (12, 1, 1));
        assert_eq!(player.stats, (22, 6, 3)); // 10+12 score, 5+1 kills, 2+1 deaths

        // Player disconnects and reconnects again. This time, server preserves/restores stats (e.g. they reconnect and have 1 kill, 1 death, 12 score).
        player.needs_reconnect_sync = true;
        player.update_session_stats(12, 1, 1);

        // Assert no double counting (accumulated stats should remain unchanged)
        assert_eq!(player.accumulated_stats, (10, 5, 2));
        assert_eq!(player.session_stats, (12, 1, 1));
        assert_eq!(player.stats, (22, 6, 3));
    }

    #[test]
    fn test_partial_demo_and_pre_activity_detection() {
        let mut state = AnalyzerState::default();

        // Simulate start of demo
        use_general_finalization(&mut state, &AnalyzerEvent::Initialization);

        // Send TimeLeft message
        use_general_finalization(
            &mut state,
            &AnalyzerEvent::UserMessage(dod::UserMessage::TimeLeft(dod::TimeLeft(
                Duration::from_secs(900), // 15 mins
            ))),
        );

        // Seed a player with pre-demo activity (e.g. they already had 2 score, 1 kill, 1 death before demo started)
        let mut player = Player::new_mock(1, "Player 1");
        player.update_session_stats(2, 1, 1);
        state.players.push(player);

        // Send another TimeLeft message near the end
        use_general_finalization(
            &mut state,
            &AnalyzerEvent::UserMessage(dod::UserMessage::TimeLeft(dod::TimeLeft(
                Duration::from_secs(300), // 5 mins
            ))),
        );

        // Add a completed round (winner_stats is Some)
        state.rounds.push(Round::Completed {
            start_time: crate::time::GameTime::default(),
            end_time: crate::time::GameTime::default(),
            winner_stats: Some((Team::Allies, 1)),
        });

        // Add an active/incomplete round at the end
        state.rounds.push(Round::Completed {
            start_time: crate::time::GameTime::default(),
            end_time: crate::time::GameTime::default(),
            winner_stats: None,
        });

        // Simulate Finalization
        use_general_finalization(&mut state, &AnalyzerEvent::Finalization);

        assert_eq!(state.first_time_left, Some(Duration::from_secs(900)));
        assert_eq!(state.last_time_left, Some(Duration::from_secs(300)));
        assert!(state.started_late);
        assert!(state.ended_early);
        assert!(state.players[0].has_pre_demo_activity);
    }

    #[test]
    fn test_inspect_lenn_demo() {
        let mut path = "demos/ktps8w1-m00cat_soul_lenn_h2.dem";
        if !std::path::Path::new(path).exists() {
            path = "../demos/ktps8w1-m00cat_soul_lenn_h2.dem";
        }
        if std::path::Path::new(path).exists() {
            let file_bytes = fs::read(path).unwrap();
            let analysis = Analysis::try_from_bytes(&file_bytes).unwrap();
            println!("CLAN MATCH: map_name: {}", analysis.demo_info.map_name);
            println!("CLAN MATCH: clan_match_detected: {}", analysis.state.clan_match_detected);
            println!("CLAN MATCH: match_start_witnessed: {}", analysis.state.match_start_witnessed);
            println!("CLAN MATCH: started_late: {}", analysis.state.started_late);
            println!("CLAN MATCH: ended_early: {}", analysis.state.ended_early);
            println!("CLAN MATCH: first_time_left: {:?}", analysis.state.first_time_left);
            println!("CLAN MATCH: last_time_left: {:?}", analysis.state.last_time_left);
            println!("CLAN MATCH: map_changed: {}", analysis.state.map_changed);
            println!("CLAN MATCH: initial_map_name: {:?}", analysis.state.initial_map_name);
            for p in &analysis.state.players {
                if p.has_pre_demo_activity {
                    println!("CLAN MATCH: Player {} has pre-demo activity! Stats: {:?}", p.name, p.stats);
                }
            }
        } else {
            panic!("Demo file not found at either path!");
        }
    }

    #[test]
    fn test_pub_demo_natural_end_detection() {
        let mut state = AnalyzerState::default();
        state.clan_match_detected = false;

        // 1. Without rounds and map change/timeleft, duration is 0, which is NOT close to match end.
        // It has no rounds, so ended_early defaults to false.
        use_general_finalization(&mut state, &AnalyzerEvent::Finalization);
        assert!(!state.ended_early);

        // 2. Add an incomplete round, making it not natural end.
        let mut state = AnalyzerState::default();
        state.clan_match_detected = false;
        state.rounds.push(Round::Completed {
            start_time: crate::time::GameTime { real_offset: Duration::ZERO, ..Default::default() },
            end_time: crate::time::GameTime { real_offset: Duration::ZERO, ..Default::default() },
            winner_stats: None,
        });
        use_general_finalization(&mut state, &AnalyzerEvent::Finalization);
        assert!(state.ended_early); // Ended early because not close to match end, no map change, and last round is incomplete

        // 3. Make duration close to standard map end (e.g., 20 mins = 1200s, let's use 1195s)
        let mut state = AnalyzerState::default();
        state.clan_match_detected = false;
        state.rounds.push(Round::Completed {
            start_time: crate::time::GameTime { real_offset: Duration::ZERO, ..Default::default() },
            end_time: crate::time::GameTime { real_offset: Duration::ZERO, ..Default::default() },
            winner_stats: None,
        });
        state.current_time.real_offset = Duration::from_secs(1195);
        use_general_finalization(&mut state, &AnalyzerEvent::Finalization);
        assert!(!state.ended_early); // Not ended early because duration is close to 1200s (natural end)
    }

    #[test]
    fn test_stealth_partial_demo() {
        let mut path = "demos/ktps8w8-stealth_ih_saints_h1_p2.dem";
        if !std::path::Path::new(path).exists() {
            path = "../demos/ktps8w8-stealth_ih_saints_h1_p2.dem";
        }
        if std::path::Path::new(path).exists() {
            let file_bytes = fs::read(path).unwrap();
            let analysis = Analysis::try_from_bytes(&file_bytes).unwrap();
            assert!(analysis.state.clan_match_detected);
            assert!(!analysis.state.match_start_witnessed);
            assert!(analysis.state.started_late);
            assert!(analysis.state.ended_early);
        }
    }

    #[test]
    fn test_parse_fingerprint() {
        let mut path = "demos/ktps8w8-stealth_ih_saints_h1_p2.dem";
        if !std::path::Path::new(path).exists() {
            path = "../demos/ktps8w8-stealth_ih_saints_h1_p2.dem";
        }
        if std::path::Path::new(path).exists() {
            let file_bytes = fs::read(path).unwrap();
            let res = parse_fingerprint(&file_bytes);
            assert!(res.is_ok());
            let (map_name, server_ip, roster_hash, event_signature, recorder_id) = res.unwrap();
            assert!(!map_name.is_empty());
            println!("Map: {}, Server: {}, Roster Hash: {}, Events: {}, Recorder: {:?}", map_name, server_ip, roster_hash, event_signature.len(), recorder_id);
        }
    }
}
