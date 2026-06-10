use crate::{AnalyzerEvent, AnalyzerState, time::GameTime};
use crate::mortality::MortalityState;
use dod::{Team, UserMessage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatType {
    Mm1,    // Public chat
    Mm2,    // Team chat
    System, // System message / Console
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub time: GameTime,
    pub frame_index: usize,
    pub chat_type: ChatType,
    pub sender_name: Option<String>,
    pub sender_team: Option<Team>,
    pub sender_dead: bool,
    pub text: String,
}

fn clean_control_chars(s: &str) -> String {
    s.chars()
        .filter(|&c| !c.is_control() && (c as u32) >= 32)
        .collect()
}

pub fn use_chat_updates(state: &mut AnalyzerState, event: &AnalyzerEvent) {
    match event {
        AnalyzerEvent::UserMessage(UserMessage::SayText(say_text)) => {
            let raw_text = &say_text.text;
            let cleaned_raw = clean_control_chars(raw_text);

            let (sender_block, message_text) = if let Some(pos) = cleaned_raw.find(" :  ") {
                (&cleaned_raw[..pos], &cleaned_raw[pos + 4..])
            } else if let Some(pos) = cleaned_raw.find(" : ") {
                (&cleaned_raw[..pos], &cleaned_raw[pos + 3..])
            } else if let Some(pos) = cleaned_raw.find(":") {
                (&cleaned_raw[..pos], &cleaned_raw[pos + 1..])
            } else {
                ("", cleaned_raw.as_str())
            };

            let sender_block_trimmed = sender_block.trim();
            let is_dead_prefix = sender_block_trimmed.contains("*DEAD*");
            let is_team_prefix = sender_block_trimmed.contains("(TEAM)") || sender_block_trimmed.contains("(Team)");
            let is_spec_prefix = sender_block_trimmed.contains("(SPECTATOR)") || sender_block_trimmed.contains("(Spectator)");

            let (sender_name, sender_team, is_dead_state) = if say_text.client_index > 0 {
                let player = state.find_player_by_client_index(say_text.client_index - 1);
                if let Some(player) = player {
                    (
                        Some(player.name.clone()),
                        player.team.clone(),
                        player.is_dead(),
                    )
                } else {
                    let name = if !sender_block_trimmed.is_empty() {
                        let mut clean_name = sender_block_trimmed.to_string();
                        clean_name = clean_name.replace("*DEAD*", "");
                        clean_name = clean_name.replace("(TEAM)", "");
                        clean_name = clean_name.replace("(Team)", "");
                        clean_name = clean_name.replace("(SPECTATOR)", "");
                        clean_name = clean_name.replace("(Spectator)", "");
                        Some(clean_name.trim().to_string())
                    } else {
                        None
                    };
                    (name, None, is_dead_prefix)
                }
            } else {
                (Some("Console/Server".to_string()), None, false)
            };

            let is_team_message = say_text.unk != 0 
                || is_team_prefix 
                || is_spec_prefix 
                || (sender_team == Some(Team::Spectators) && is_team_prefix);

            let chat_type = if is_team_message {
                ChatType::Mm2
            } else {
                ChatType::Mm1
            };

            state.chat_messages.push(ChatMessage {
                time: state.current_time.clone(),
                frame_index: state.frame_index,
                chat_type,
                sender_name,
                sender_team,
                sender_dead: is_dead_prefix || is_dead_state,
                text: message_text.trim().to_string(),
            });
        }

        AnalyzerEvent::UserMessage(UserMessage::TextMsg(text_msg)) => {
            let raw_text = clean_control_chars(&text_msg.text);
            
            let formatted_text = if raw_text.starts_with("#Game_join") {
                let name = text_msg.arg1.as_deref().unwrap_or("Someone");
                format!("{} joined the game", name)
            } else if raw_text.starts_with("#Game_connected") {
                let name = text_msg.arg1.as_deref().unwrap_or("Someone");
                format!("{} connected", name)
            } else if raw_text.starts_with("#Game_disconnected") {
                let name = text_msg.arg1.as_deref().unwrap_or("Someone");
                format!("{} disconnected", name)
            } else if raw_text.starts_with("#Game_will_restart_in") {
                let time = text_msg.arg1.as_deref().unwrap_or("?");
                format!("Game will restart in {} seconds", time)
            } else if raw_text.starts_with("#Game_ready_team") {
                let team = text_msg.arg1.as_deref().unwrap_or("Team");
                format!("{} is ready", team)
            } else if raw_text.starts_with("#Game_ready") {
                let name = text_msg.arg1.as_deref().unwrap_or("Player");
                format!("{} is ready", name)
            } else {
                let mut parts = vec![raw_text];
                if let Some(arg) = &text_msg.arg1 { parts.push(clean_control_chars(arg)); }
                if let Some(arg) = &text_msg.arg2 { parts.push(clean_control_chars(arg)); }
                if let Some(arg) = &text_msg.arg3 { parts.push(clean_control_chars(arg)); }
                if let Some(arg) = &text_msg.arg4 { parts.push(clean_control_chars(arg)); }
                parts.join(" ")
            };

            state.chat_messages.push(ChatMessage {
                time: state.current_time.clone(),
                frame_index: state.frame_index,
                chat_type: ChatType::System,
                sender_name: None,
                sender_team: None,
                sender_dead: false,
                text: formatted_text,
            });
        }

        _ => {}
    }
}
