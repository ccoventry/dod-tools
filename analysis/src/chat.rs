use crate::mortality::MortalityState;
use crate::{AnalyzerEvent, AnalyzerState, time::GameTime};
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
    pub system_token: Option<String>,
    pub system_args: Vec<Option<String>>,
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

            // 1. Try to find the split position based on player name first (if player is resolved)
            let mut split_pos = None;
            if say_text.client_index > 0 {
                if let Some(player) = state.find_player_by_client_index(say_text.client_index - 1) {
                    let p_name = &player.name;
                    if let Some(name_pos) = cleaned_raw.find(p_name) {
                        let after_name = &cleaned_raw[name_pos + p_name.len()..];
                        if after_name.starts_with(" :  ") {
                            split_pos = Some(name_pos + p_name.len() + 1);
                        } else if after_name.starts_with(" : ") {
                            split_pos = Some(name_pos + p_name.len() + 1);
                        } else if after_name.starts_with(":") {
                            split_pos = Some(name_pos + p_name.len());
                        }
                    }
                }
            }

            let (sender_block, message_text) = if let Some(pos) = split_pos {
                let sender = &cleaned_raw[..pos];
                let after_colon = &cleaned_raw[pos + 1..];
                let skip = if after_colon.starts_with("  ") {
                    3
                } else if after_colon.starts_with(' ') {
                    2
                } else {
                    1
                };
                (sender, &cleaned_raw[pos + skip..])
            } else {
                // Fallback splitting logic: ignore colons inside brackets/parentheses (e.g. tag dicE[: :])
                if let Some(pos) = cleaned_raw.find(" :  ") {
                    (&cleaned_raw[..pos], &cleaned_raw[pos + 4..])
                } else if let Some(pos) = cleaned_raw.find(" : ") {
                    (&cleaned_raw[..pos], &cleaned_raw[pos + 3..])
                } else {
                    let mut bracket_depth = 0;
                    let mut paren_depth = 0;
                    let mut found_pos = None;
                    for (idx, c) in cleaned_raw.char_indices() {
                        if c == '[' {
                            bracket_depth += 1;
                        } else if c == ']' {
                            if bracket_depth > 0 {
                                bracket_depth -= 1;
                            }
                        } else if c == '(' {
                            paren_depth += 1;
                        } else if c == ')' {
                            if paren_depth > 0 {
                                paren_depth -= 1;
                            }
                        } else if c == ':' && bracket_depth == 0 && paren_depth == 0 {
                            found_pos = Some(idx);
                            break;
                        }
                    }
                    if let Some(pos) = found_pos {
                        let after_colon = &cleaned_raw[pos + 1..];
                        let skip = if after_colon.starts_with(' ') { 2 } else { 1 };
                        (&cleaned_raw[..pos], &cleaned_raw[pos + skip..])
                    } else {
                        ("", cleaned_raw.as_str())
                    }
                }
            };

            let sender_block_trimmed = sender_block.trim();
            let is_dead_prefix = sender_block_trimmed.contains("*DEAD*");
            let is_team_prefix =
                sender_block_trimmed.contains("(TEAM)") || sender_block_trimmed.contains("(Team)");
            let is_spec_prefix = sender_block_trimmed.contains("(SPECTATOR)")
                || sender_block_trimmed.contains("(Spectator)");

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
                let console_name = crate::localization::translate_key("#app_console_server")
                    .unwrap_or_else(|| "Console/Server".to_string());
                (Some(console_name), None, false)
            };

            let is_team_message =
                is_team_prefix || is_spec_prefix || sender_team == Some(Team::Spectators);

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
                system_token: None,
                system_args: Vec::new(),
            });
        }

        AnalyzerEvent::UserMessage(UserMessage::TextMsg(text_msg)) => {
            let formatted_text = translate_system_message(
                &text_msg.text,
                text_msg.arg1.as_deref(),
                text_msg.arg2.as_deref(),
                text_msg.arg3.as_deref(),
                text_msg.arg4.as_deref(),
            );

            state.chat_messages.push(ChatMessage {
                time: state.current_time.clone(),
                frame_index: state.frame_index,
                chat_type: ChatType::System,
                sender_name: None,
                sender_team: None,
                sender_dead: false,
                text: formatted_text,
                system_token: Some(text_msg.text.clone()),
                system_args: vec![
                    text_msg.arg1.clone(),
                    text_msg.arg2.clone(),
                    text_msg.arg3.clone(),
                    text_msg.arg4.clone(),
                ],
            });
        }

        _ => {}
    }
}

pub fn translate_system_message(
    token: &str,
    arg1: Option<&str>,
    arg2: Option<&str>,
    arg3: Option<&str>,
    arg4: Option<&str>,
) -> String {
    let mut key = token.trim().to_lowercase();
    if !key.starts_with('#') {
        key.insert(0, '#');
    }

    if let Some(template) = crate::localization::translate_key(&key) {
        let mut result = template.clone();
        if let Some(a) = arg1 {
            result = result.replace("%s1", a);
        }
        if let Some(a) = arg2 {
            result = result.replace("%s2", a);
        }
        if let Some(a) = arg3 {
            result = result.replace("%s3", a);
        }
        if let Some(a) = arg4 {
            result = result.replace("%s4", a);
        }

        if result.contains("%s") {
            let args = [arg1, arg2, arg3, arg4];
            let mut arg_idx = 0;
            while let Some(pos) = result.find("%s") {
                if arg_idx < args.len() {
                    let replacement = args[arg_idx].unwrap_or("");
                    result.replace_range(pos..pos + 2, replacement);
                    arg_idx += 1;
                } else {
                    result.replace_range(pos..pos + 2, "");
                }
            }
        }
        result
    } else {
        // Fallback
        let fallback_someone = crate::localization::translate_key("#app_fallback_someone")
            .unwrap_or_else(|| "Someone".to_string());
        let fallback_player = crate::localization::translate_key("#app_fallback_player")
            .unwrap_or_else(|| "Player".to_string());

        if key.starts_with("#game_joined_team") {
            let name = arg1.unwrap_or(&fallback_someone);
            let team = arg2.unwrap_or("a team");
            format!("{} joined team {}", name, team)
        } else if key.starts_with("#game_joined_game") || key.starts_with("#game_join") {
            let name = arg1.unwrap_or(&fallback_someone);
            format!("{} joined the game", name)
        } else if key.starts_with("#game_connected") {
            let name = arg1.unwrap_or(&fallback_someone);
            format!("{} connected", name)
        } else if key.starts_with("#game_disconnected") {
            let name = arg1.unwrap_or(&fallback_someone);
            format!("{} disconnected", name)
        } else if key.starts_with("#game_will_restart_in") {
            let time = arg1.unwrap_or("?");
            format!("Game will restart in {} seconds", time)
        } else if key.starts_with("#game_ready_team") {
            let team = arg1.unwrap_or("Team");
            format!("{} is ready", team)
        } else if key.starts_with("#game_ready") {
            let name = arg1.unwrap_or(&fallback_player);
            format!("{} is ready", name)
        } else {
            let mut parts = vec![token.to_string()];
            if let Some(arg) = arg1 {
                parts.push(clean_control_chars(arg));
            }
            if let Some(arg) = arg2 {
                parts.push(clean_control_chars(arg));
            }
            if let Some(arg) = arg3 {
                parts.push(clean_control_chars(arg));
            }
            if let Some(arg) = arg4 {
                parts.push(clean_control_chars(arg));
            }
            parts.join(" ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_system_message_fallback() {
        let res = translate_system_message(
            "#Game_joined_team",
            Some("Warchyld"),
            Some("Allies"),
            None,
            None,
        );
        if crate::localization::translate_key("#game_joined_team").is_some() {
            assert_eq!(res, "*Warchyld joined Allies");
        } else {
            assert_eq!(res, "Warchyld joined team Allies");
        }

        let res2 = translate_system_message("#Game_disconnected", Some("scrd"), None, None, None);
        if crate::localization::translate_key("#game_disconnected").is_some() {
            assert_eq!(res2, "scrd has left the game");
        } else {
            assert_eq!(res2, "scrd disconnected");
        }

        let res_unknown =
            translate_system_message("#Unknown_Token", Some("arg1"), Some("arg2"), None, None);
        assert_eq!(res_unknown, "#Unknown_Token arg1 arg2");
    }

    #[test]
    fn test_chat_splitter() {
        use crate::Player;
        use dod::{SayText, UserMessage};

        let mut state = AnalyzerState::default();
        // Add a mock player with colons in name
        state.players.push(Player::new_mock(3, "dicE[: :]"));

        // 1. Test standard message with mock player
        let event = AnalyzerEvent::UserMessage(UserMessage::SayText(SayText {
            client_index: 4, // 1-based, so matches player index/client_id 3
            text: "dicE[: :] :  hello there".to_string(),
        }));
        use_chat_updates(&mut state, &event);
        assert_eq!(state.chat_messages.len(), 1);
        assert_eq!(
            state.chat_messages[0].sender_name.as_deref(),
            Some("dicE[: :]")
        );
        assert_eq!(state.chat_messages[0].text, "hello there");
        assert_eq!(state.chat_messages[0].chat_type, ChatType::Mm1); // all chat

        // 2. Test team message with mock player
        let event2 = AnalyzerEvent::UserMessage(UserMessage::SayText(SayText {
            client_index: 4,
            text: "(TEAM) dicE[: :] :  team chat message".to_string(),
        }));
        use_chat_updates(&mut state, &event2);
        assert_eq!(state.chat_messages.len(), 2);
        assert_eq!(
            state.chat_messages[1].sender_name.as_deref(),
            Some("dicE[: :]")
        );
        assert_eq!(state.chat_messages[1].text, "team chat message");
        assert_eq!(state.chat_messages[1].chat_type, ChatType::Mm2); // team chat

        // 3. Test fallback with no known player but colons in tag
        let event3 = AnalyzerEvent::UserMessage(UserMessage::SayText(SayText {
            client_index: 99, // unknown player
            text: "Some[Other:Tag]Player :  hello world".to_string(),
        }));
        use_chat_updates(&mut state, &event3);
        assert_eq!(state.chat_messages.len(), 3);
        assert_eq!(
            state.chat_messages[2].sender_name.as_deref(),
            Some("Some[Other:Tag]Player")
        );
        assert_eq!(state.chat_messages[2].text, "hello world");

        // 4. Test fallback with no spaces around colon
        let event4 = AnalyzerEvent::UserMessage(UserMessage::SayText(SayText {
            client_index: 99,
            text: "dicE[: :]:hello".to_string(),
        }));
        use_chat_updates(&mut state, &event4);
        assert_eq!(state.chat_messages.len(), 4);
        assert_eq!(
            state.chat_messages[3].sender_name.as_deref(),
            Some("dicE[: :]")
        );
        assert_eq!(state.chat_messages[3].text, "hello");
    }
}
