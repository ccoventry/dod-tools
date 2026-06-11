//! Scratch: inspect what stat packets appear around a player disconnect+reconnect.
//!
//! Run with:
//!   cargo run --bin dod_tools_dump -- <demo> (doesn't work for this purpose)
//! Instead compile and run directly:
//!   cargo script or just use the inspect binary approach.
//!
//! This is a standalone scratch binary — add to native/Cargo.toml [[bin]] temporarily if needed.

use dem::open_demo_from_bytes;
use dem::types::{EngineMessage, FrameData, MessageData, NetMessage};
use dod::UserMessage;
use std::collections::HashMap;
use std::fs;

fn null_str(bytes: &[u8]) -> String {
    let trimmed = bytes.split(|&b| b == 0).next().unwrap_or(bytes);
    String::from_utf8_lossy(trimmed).to_string()
}

fn parse_userinfo(user_info: &[u8]) -> HashMap<String, String> {
    let s = String::from_utf8_lossy(user_info);
    let s = s.trim_matches(|c| c == '\\' || c == '\0');
    let parts: Vec<&str> = s.split('\\').collect();
    let mut map = HashMap::new();
    for chunk in parts.chunks_exact(2) {
        map.insert(chunk[0].to_string(), chunk[1].to_string());
    }
    map
}

fn main() {
    let demo_path = std::env::args().nth(1).expect("Usage: inspect_reconnect <demo.dem>");
    let bytes = fs::read(&demo_path).expect("Could not read demo file");
    let demo = open_demo_from_bytes(&bytes).expect("Could not parse demo");

    println!("=== Scanning: {} ===\n", demo_path);

    // We'll track the last 20 stat-related events per slot in a ring buffer,
    // and print a window whenever we see a disconnect or reconnect.

    #[derive(Debug, Clone)]
    enum Event {
        Connect { slot: u8, name: String, sid: String },
        Disconnect { slot: u8, name: String },
        Frags { slot: u8, frags: i16 },
        ScoreShort { slot: u8, score: i16, kills: i16, deaths: i16 },
        DeathMsg { killer_slot: u8, victim_slot: u8, weapon: String },
    }

    // Slot -> last known name (for disconnect messages which have no name)
    let mut slot_names: HashMap<u8, String> = HashMap::new();

    let mut events: Vec<(usize, Event)> = Vec::new();
    let mut frame_idx = 0usize;

    for entry in &demo.directory.entries {
        for frame in &entry.frames {
            frame_idx += 1;

            if let FrameData::NetworkMessage(net_msg_box) = &frame.frame_data {
                if let MessageData::Parsed(msgs) = &net_msg_box.1.messages {
                    for msg in msgs {
                        match msg {
                            NetMessage::EngineMessage(eng) => {
                                if let EngineMessage::SvcUpdateUserInfo(upd) = eng.as_ref() {
                                    let fields = parse_userinfo(upd.user_info.as_ref());
                                    if fields.is_empty() {
                                        // Disconnect
                                        let name = slot_names
                                            .get(&upd.index)
                                            .cloned()
                                            .unwrap_or_else(|| format!("slot_{}", upd.index));
                                        events.push((frame_idx, Event::Disconnect {
                                            slot: upd.index,
                                            name,
                                        }));
                                    } else if fields.get("*hltv").map(|v| v == "1").unwrap_or(false) {
                                        // Skip HLTV
                                    } else {
                                        let name = fields.get("name").cloned().unwrap_or_default();
                                        let sid = fields.get("*sid").cloned().unwrap_or_else(|| {
                                            fields.get("*fid").cloned().unwrap_or_default()
                                        });
                                        slot_names.insert(upd.index, name.clone());
                                        events.push((frame_idx, Event::Connect {
                                            slot: upd.index,
                                            name,
                                            sid,
                                        }));
                                    }
                                }
                            }
                            NetMessage::UserMessage(usr) => {
                                let msg_name = null_str(&usr.name);
                                match msg_name.as_str() {
                                    "Frags" => {
                                        if let Ok(UserMessage::Frags(f)) =
                                            UserMessage::new(&usr.name, &usr.data)
                                        {
                                            events.push((frame_idx, Event::Frags {
                                                slot: f.client_index - 1,
                                                frags: f.frags,
                                            }));
                                        }
                                    }
                                    "ScoreShort" => {
                                        if let Ok(UserMessage::ScoreShort(s)) =
                                            UserMessage::new(&usr.name, &usr.data)
                                        {
                                            events.push((frame_idx, Event::ScoreShort {
                                                slot: s.client_index - 1,
                                                score: s.score,
                                                kills: s.kills,
                                                deaths: s.deaths,
                                            }));
                                        }
                                    }
                                    "DeathMsg" => {
                                        if let Ok(UserMessage::DeathMsg(d)) =
                                            UserMessage::new(&usr.name, &usr.data)
                                        {
                                            events.push((frame_idx, Event::DeathMsg {
                                                killer_slot: d.killer_client_index.saturating_sub(1),
                                                victim_slot: d.victim_client_index.saturating_sub(1),
                                                weapon: format!("{:?}", d.weapon),
                                            }));
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

    // Find all disconnect events and print a ±15 event window around each
    let disconnect_indices: Vec<usize> = events
        .iter()
        .enumerate()
        .filter_map(|(i, (_, e))| matches!(e, Event::Disconnect { .. }).then_some(i))
        .collect();

    if disconnect_indices.is_empty() {
        println!("No disconnect events found in this demo.");
        return;
    }

    println!("Found {} disconnect(s). Showing ±15 events around each:\n", disconnect_indices.len());

    for dc_idx in &disconnect_indices {
        let start = dc_idx.saturating_sub(15);
        let end = (dc_idx + 16).min(events.len());

        let (_, dc_event) = &events[*dc_idx];
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("DISCONNECT at event index {} (frame ~{}): {:?}", dc_idx, events[*dc_idx].0, dc_event);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        for i in start..end {
            let (frame, event) = &events[i];
            let marker = if i == *dc_idx { ">>> " } else { "    " };
            println!("{}{:>7}f  {:?}", marker, frame, event);
        }
        println!();
    }
}
