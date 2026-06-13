use crate::views::{
    ALLIES_COLOR, AXIS_COLOR, BRITISH_COLOR, PlayerHighlighting, TABLE_ROW_HEIGHT, t, weapon_name,
};
use analysis::{Analysis, Connection, Player, Team, Weapon, SteamId, MortalityState};
use egui::{Align, Layout, ScrollArea, Ui, Color32, Stroke};
use egui_extras::{Column, TableBuilder};
use std::collections::HashSet;
use std::time::Duration;

pub fn player_details_ui(
    analysis: Option<&Analysis>,
    player_highlighting: &mut PlayerHighlighting,
    ui: &mut Ui,
) {
    let analysis = match analysis {
        Some(a) => a,
        None => {
            ui.label(t("#app_chat_no_analysis"));
            return;
        }
    };

    let players = &analysis.state.players;
    if players.is_empty() {
        ui.label("No players found in this demo.");
        return;
    }

    let mut sorted_players: Vec<&Player> = players.iter().collect();
    sorted_players.sort_by(|l, r| l.name.cmp(&r.name));

    // Persistent selected player ID
    let selected_player_id_key = egui::Id::new("player_details_selected_player_id");
    let mut selected_id = ui.data(|d| d.get_temp::<analysis::PlayerGlobalId>(selected_player_id_key));

    // Sync with player_highlighting (Scoreboard selection)
    let highlighted_id = player_highlighting.highlighted.iter().next().cloned();

    let mut current_id = None;
    if let Some(ref h_id) = highlighted_id {
        if sorted_players.iter().any(|p| &p.id == h_id) {
            current_id = Some(h_id.clone());
        }
    }

    // Fall back to egui temp data or first player
    if current_id.is_none() {
        if let Some(ref sel_id) = selected_id {
            if sorted_players.iter().any(|p| &p.id == sel_id) {
                current_id = Some(sel_id.clone());
            }
        }
    }

    if current_id.is_none() {
        current_id = sorted_players.first().map(|p| p.id.clone());
    }

    // Update both if changed
    if selected_id.as_ref() != current_id.as_ref() {
        selected_id = current_id.clone();
        if let Some(ref id) = selected_id {
            ui.data_mut(|d| d.insert_temp(selected_player_id_key, id.clone()));
        }
    }

    // Update player_highlighting to match selected player
    if let Some(ref id) = selected_id {
        if !player_highlighting.highlighted.contains(id) {
            player_highlighting.highlighted.clear();
            player_highlighting.highlighted.insert(id.clone());
        }
    }

    // Render Dropdown
    ui.horizontal(|ui| {
        ui.label(t("#app_player_details_select"));
        
        let current_name = selected_id.as_ref()
            .and_then(|id| sorted_players.iter().find(|p| &p.id == id))
            .map(|p| p.name.as_str())
            .unwrap_or("");

        egui::ComboBox::from_id_salt("player_details_select")
            .selected_text(current_name)
            .show_ui(ui, |ui| {
                for p in &sorted_players {
                    let selected = selected_id.as_ref() == Some(&p.id);
                    if ui.selectable_label(selected, &p.name).clicked() {
                        selected_id = Some(p.id.clone());
                        ui.data_mut(|d| d.insert_temp(selected_player_id_key, p.id.clone()));
                        
                        // Update highlight selection
                        player_highlighting.highlighted.clear();
                        player_highlighting.highlighted.insert(p.id.clone());
                    }
                }
            });
    });

    ui.add_space(8.0);

    let active_player = match selected_id.as_ref().and_then(|id| players.iter().find(|p| &p.id == id)) {
        Some(p) => p,
        None => return,
    };

    // 1. Hero Card Header
    render_hero_card(active_player, ui);
    ui.add_space(12.0);

    // 2. Highlights Row (Stat Cards)
    render_stat_cards(active_player, ui);
    ui.add_space(16.0);

    // 3. Two-Column Details Area
    ui.columns(2, |cols| {
        // Left Column: Weapon Breakdown
        let ui_left = &mut cols[0];
        ui_left.strong(t("#app_player_details_weapon_breakdown"));
        ui_left.add_space(4.0);
        ScrollArea::vertical()
            .id_salt("player_details_weapons_scroll")
            .auto_shrink(false)
            .min_scrolled_height(260.)
            .show(ui_left, |ui| {
                render_weapon_breakdown(&active_player.weapon_breakdown, ui);
            });

        // Right Column: Kill Streaks
        let ui_right = &mut cols[1];
        ui_right.strong(t("#app_player_details_kill_streaks"));
        ui_right.add_space(4.0);
        
        let filter_key = egui::Id::new("player_details_streak_filters").with(&active_player.id);
        let mut disabled_weapons: HashSet<String> = ui_right.data(|d| d.get_temp(filter_key).unwrap_or_default());

        let mut all_weapons: Vec<Weapon> = active_player
            .kill_streaks
            .iter()
            .flat_map(|s| s.kills.iter().map(|(_, w, _)| w.clone()))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        all_weapons.sort_by_key(|w| weapon_name(w));

        if !all_weapons.is_empty() {
            ui_right.horizontal(|ui| {
                ui.label(egui::RichText::new(t("#app_streaks_filter_weapons")).small().weak());
                if ui.small_button(t("#app_chat_select_all")).clicked() {
                    disabled_weapons.clear();
                    ui.data_mut(|d| d.insert_temp(filter_key, disabled_weapons.clone()));
                }
                if ui.small_button(t("#app_chat_clear_all")).clicked() {
                    disabled_weapons = all_weapons.iter().map(|w| format!("{:?}", w)).collect();
                    ui.data_mut(|d| d.insert_temp(filter_key, disabled_weapons.clone()));
                }
            });

            // Grouped checkboxes
            render_streak_weapon_filters(&all_weapons, &mut disabled_weapons, filter_key, ui_right);
            ui_right.add_space(4.0);
        }

        ui_right.separator();
        ui_right.add_space(4.0);

        ScrollArea::vertical()
            .id_salt("player_details_streaks_scroll")
            .auto_shrink(false)
            .min_scrolled_height(260.)
            .show(ui_right, |ui| {
                if active_player.kill_streaks.is_empty() {
                    ui.label("No kill streaks found for this player.");
                } else {
                    render_kill_streaks_table(
                        active_player,
                        &disabled_weapons,
                        analysis,
                        player_highlighting,
                        ui,
                    );
                }
            });
    });
}

fn render_hero_card(p: &Player, ui: &mut Ui) {
    let team_color = match &p.team {
        Some(Team::Allies) => ALLIES_COLOR,
        Some(Team::Axis) => AXIS_COLOR,
        Some(Team::British) => BRITISH_COLOR,
        _ => Color32::GRAY,
    };
    
    let bg_color = ui.visuals().window_fill();
    let border_color = team_color.linear_multiply(0.3);

    egui::Frame::NONE
        .fill(bg_color)
        .stroke(Stroke::new(1.5, border_color))
        .corner_radius(6.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.heading(egui::RichText::new(&p.name).size(24.0).strong());
                        ui.horizontal(|ui| {
                            let team_name = match &p.team {
                                Some(Team::Allies) => "ALLIES",
                                Some(Team::Axis) => "AXIS",
                                Some(Team::British) => "BRITISH",
                                Some(Team::Spectators) => "SPECTATORS",
                                _ => "UNASSIGNED",
                            };
                            ui.colored_label(team_color, egui::RichText::new(team_name).strong().small());
                            
                            if let Some(ref class) = p.class {
                                ui.colored_label(ui.visuals().weak_text_color(), "|");
                                ui.label(egui::RichText::new(format!("{:?}", class).to_uppercase()).small());
                            }
                        });
                    });

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Profile Links
                        match SteamId::try_from(&p.id) {
                            Ok(steam_id) => {
                                let steam_url = format!("https://steamcommunity.com/profiles/{}", p.id);
                                let lp_url = format!("https://www.legit-proof.com/search?q={}", steam_id);

                                if ui.link("Legit-Proof").on_hover_text("Search Legit-Proof for this Steam ID").clicked() {
                                    ui.ctx().open_url(egui::OpenUrl::new_tab(lp_url));
                                }
                                ui.label("/");
                                if ui.link("Steam").on_hover_text("Open Steam Community Profile").clicked() {
                                    ui.ctx().open_url(egui::OpenUrl::new_tab(steam_url));
                                }
                            }
                            _ => {
                                ui.weak(t("#app_player_details_no_steam"));
                            }
                        }
                    });
                });

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    // ID & Status info
                    let id_text = match SteamId::try_from(&p.id) {
                        Ok(steam_id) => steam_id.to_string(),
                        _ => p.id.to_string(),
                    };
                    ui.weak("Steam ID:");
                    ui.monospace(&id_text);
                    if ui.small_button("📋").on_hover_text("Copy Steam ID").clicked() {
                        ui.ctx().copy_text(id_text);
                    }

                    ui.add_space(16.0);
                    ui.weak("Status:");
                    match &p.connection {
                        Connection::Connected { client_id } => {
                            ui.colored_label(Color32::GREEN, format!("Connected (Slot {})", client_id));
                        }
                        Connection::Disconnected => {
                            ui.colored_label(Color32::GRAY, "Disconnected");
                        }
                    }

                    if p.has_reconnected || p.has_pre_demo_activity {
                        ui.add_space(16.0);
                        ui.weak("Flags:");
                        if p.has_reconnected {
                            ui.colored_label(Color32::from_rgb(251, 191, 36), "🔄 Reconnects")
                                .on_hover_text(t("#app_player_reconnected_desc"));
                        }
                        if p.has_pre_demo_activity {
                            ui.colored_label(Color32::from_rgb(251, 191, 36), "* Pre-Demo Activity")
                                .on_hover_text(t("#app_player_pre_demo_desc"));
                        }
                    }
                });
            });
        });
}

fn render_stat_cards(p: &Player, ui: &mut Ui) {
    ui.horizontal(|ui| {
        let card_width = 130.0;
        let card_height = 60.0;

        // Score Card
        render_card(ui, card_width, card_height, t("#app_player_details_card_score"), format!("{}", p.stats.0), None);
        // Kills Card
        render_card(ui, card_width, card_height, t("#app_player_details_card_kills"), format!("{}", p.stats.1), None);
        
        // Deaths Card with K/D Ratio
        let kd = if p.stats.2 > 0 {
            p.stats.1 as f32 / p.stats.2 as f32
        } else {
            p.stats.1 as f32
        };
        let kd_badge = Some((
            format!("{:.2} K/D", kd),
            if kd >= 1.0 { Color32::from_rgb(34, 197, 94) } else { Color32::from_rgb(239, 68, 68) }
        ));
        render_card(ui, card_width, card_height, t("#app_player_details_card_deaths"), format!("{}", p.stats.2), kd_badge);

        // Lifespan Card
        let avg_life = p.avg_lifespan().as_secs_f32();
        let min_life = p.min_lifespan().as_secs_f32();
        let max_life = p.max_lifespan().as_secs_f32();
        let lifespan_badge = Some((
            format!("Min: {:.1}s / Max: {:.1}s", min_life, max_life),
            ui.visuals().weak_text_color()
        ));
        render_card(ui, card_width, card_height, t("#app_player_details_card_lifespan"), format!("{:.1}s", avg_life), lifespan_badge);
    });
}

fn render_card(ui: &mut Ui, width: f32, height: f32, title: String, value: String, badge: Option<(String, Color32)>) {
    let bg_color = ui.visuals().faint_bg_color;
    egui::Frame::NONE
        .fill(bg_color)
        .corner_radius(4.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width(width);
            ui.set_height(height);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(title).small().weak());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(value).size(18.0).strong());
                    if let Some((badge_text, color)) = badge {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(egui::RichText::new(badge_text).small().color(color));
                        });
                    }
                });
            });
        });
}

fn render_weapon_breakdown(
    breakdown: &std::collections::HashMap<Weapon, (u32, u32)>,
    ui: &mut Ui,
) {
    let mut weapon_breakdown: Vec<(String, (u32, u32))> = breakdown
        .iter()
        .map(|(w, stats)| (weapon_name(w), *stats))
        .collect();

    weapon_breakdown.sort_by(|(name_a, l), (name_b, r)| {
        let cmp = l.cmp(r).reverse();
        if cmp == std::cmp::Ordering::Equal {
            name_a.cmp(name_b)
        } else {
            cmp
        }
    });

    let total_kills = weapon_breakdown.iter().map(|(_, (k, _))| k).sum::<u32>();

    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .columns(Column::auto(), 4)
        .header(TABLE_ROW_HEIGHT, |mut row| {
            row.col(|ui| { ui.strong(t("#app_col_weapon")); });
            row.col(|ui| { ui.strong(t("#app_col_kills")); });
            row.col(|ui| { ui.strong(t("#app_col_pct_total")); });
            row.col(|ui| { ui.strong(t("#app_col_teamkills")); });
        })
        .body(|mut body| {
            for (w_name, (kills, teamkills)) in weapon_breakdown {
                body.row(TABLE_ROW_HEIGHT, |mut row| {
                    row.col(|ui| { ui.label(w_name); });
                    row.col(|ui| { ui.label(format!("{}", kills)); });
                    row.col(|ui| {
                        let pct = if total_kills > 0 {
                            (kills as f32 / total_kills as f32) * 100.0
                        } else {
                            0.0
                        };
                        ui.horizontal(|ui| {
                            ui.label(format!("{:.1}%", pct));
                            ui.add(egui::ProgressBar::new(pct / 100.0).desired_width(40.0));
                        });
                    });
                    row.col(|ui| { ui.label(format!("{}", teamkills)); });
                });
            }
        });
}

fn render_streak_weapon_filters(
    all_weapons: &[Weapon],
    disabled_weapons: &mut HashSet<String>,
    filter_key: egui::Id,
    ui: &mut Ui,
) {
    let categories: &[(&str, &[Weapon])] = &[
        ("Grenades", &[
            Weapon::Mk2Grenade, Weapon::StickGrenade, Weapon::MillsBomb,
        ]),
        ("Melee", &[
            Weapon::Kabar, Weapon::GermanKnife, Weapon::BritishKnife,
            Weapon::Spade, Weapon::K98Bayonet, Weapon::EnfieldBayonet,
            Weapon::ButtStock,
        ]),
        ("Allied", &[
            Weapon::M1911, Weapon::Garand, Weapon::Springfield,
            Weapon::Thompson, Weapon::Bar, Weapon::M1Carbine,
            Weapon::Browning30Cal, Weapon::GreaseGun, Weapon::Bazooka,
            Weapon::LeeEnfield, Weapon::ScopedLeeEnfield, Weapon::Sten,
            Weapon::Bren, Weapon::Webley, Weapon::Piat, Weapon::M1A1Carbine,
            Weapon::Mortar,
        ]),
        ("Axis", &[
            Weapon::Luger, Weapon::ScopedK98, Weapon::Stg44, Weapon::K98,
            Weapon::Mp40, Weapon::Mg42, Weapon::Mg34, Weapon::Fg42,
            Weapon::ScopedFg42, Weapon::K43, Weapon::Panzerschreck,
        ]),
    ];

    let categorized: HashSet<String> = categories
        .iter()
        .flat_map(|(_, weapons)| weapons.iter().map(|w| format!("{:?}", w)))
        .collect();
    let mut other_weapons: Vec<&Weapon> = all_weapons
        .iter()
        .filter(|w| !categorized.contains(&format!("{:?}", w)))
        .collect();
    other_weapons.sort_by_key(|w| weapon_name(w));

    for (group_label, group_weapons) in categories {
        let present: Vec<&Weapon> = group_weapons
            .iter()
            .filter(|w| all_weapons.contains(w))
            .collect();
        if present.is_empty() {
            continue;
        }

        let group_keys: Vec<String> = present
            .iter()
            .map(|w| format!("{:?}", w))
            .collect();
        let all_enabled = group_keys.iter().all(|k| !disabled_weapons.contains(k));

        ui.horizontal_wrapped(|ui| {
            let label_text = egui::RichText::new(format!("[{}]", group_label))
                .small()
                .strong();
            let resp = ui.add(
                egui::Label::new(label_text).sense(egui::Sense::click())
            );
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                if all_enabled {
                    for k in &group_keys { disabled_weapons.insert(k.clone()); }
                } else {
                    for k in &group_keys { disabled_weapons.remove(k); }
                }
                ui.data_mut(|d| d.insert_temp(filter_key, disabled_weapons.clone()));
            }

            for weapon in &present {
                let name = weapon_name(weapon);
                let key = format!("{:?}", weapon);
                let mut enabled = !disabled_weapons.contains(&key);
                if ui.checkbox(&mut enabled, &name).changed() {
                    if enabled { disabled_weapons.remove(&key); }
                    else { disabled_weapons.insert(key); }
                    ui.data_mut(|d| d.insert_temp(filter_key, disabled_weapons.clone()));
                }
            }
        });
    }

    if !other_weapons.is_empty() {
        let other_keys: Vec<String> = other_weapons
            .iter()
            .map(|w| format!("{:?}", w))
            .collect();
        let all_enabled = other_keys.iter().all(|k| !disabled_weapons.contains(k));
        ui.horizontal_wrapped(|ui| {
            let label_text = egui::RichText::new("[Other]").small().strong();
            let resp = ui.add(egui::Label::new(label_text).sense(egui::Sense::click()));
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                if all_enabled {
                    for k in &other_keys { disabled_weapons.insert(k.clone()); }
                } else {
                    for k in &other_keys { disabled_weapons.remove(k); }
                }
                ui.data_mut(|d| d.insert_temp(filter_key, disabled_weapons.clone()));
            }
            for weapon in &other_weapons {
                let name = weapon_name(weapon);
                let key = format!("{:?}", weapon);
                let mut enabled = !disabled_weapons.contains(&key);
                if ui.checkbox(&mut enabled, &name).changed() {
                    if enabled { disabled_weapons.remove(&key); }
                    else { disabled_weapons.insert(key); }
                    ui.data_mut(|d| d.insert_temp(filter_key, disabled_weapons.clone()));
                }
            }
        });
    }
}

fn render_kill_streaks_table(
    p: &Player,
    disabled_weapons: &HashSet<String>,
    analysis: &Analysis,
    player_highlighting: &mut PlayerHighlighting,
    ui: &mut Ui,
) {
    TableBuilder::new(ui)
        .striped(false)
        .cell_layout(Layout::left_to_right(Align::Center))
        .columns(Column::auto(), 5)
        .header(TABLE_ROW_HEIGHT, |mut row| {
            row.col(|ui| { ui.strong(t("#app_col_wave")); });
            row.col(|ui| { ui.strong(t("#app_col_total_kills")); });
            row.col(|ui| { ui.strong(t("#app_col_time")); });
            row.col(|ui| { ui.strong(t("#app_col_duration")); });
            row.col(|ui| { ui.strong(t("#app_col_streak_details")); });
        })
        .body(|mut body| {
            let mut displayed_wave = 0usize;

            for streak in &p.kill_streaks {
                let filtered_kills: Vec<_> = streak
                    .kills
                    .iter()
                    .filter(|(_, w, _)| !disabled_weapons.contains(&format!("{:?}", w)))
                    .collect();

                if filtered_kills.is_empty() {
                    continue;
                }

                displayed_wave += 1;

                let first = filtered_kills.first().unwrap();
                let last = filtered_kills.last().unwrap();
                let start_dur = first.0.viewdemo_offset;
                let total_dur = last.0.viewdemo_offset
                    .checked_sub(first.0.viewdemo_offset)
                    .unwrap_or(Duration::ZERO);

                let mut grouped: Vec<(String, usize)> = Vec::new();
                for (_, weapon, _) in &filtered_kills {
                    let name = weapon_name(weapon);
                    if let Some((last_name, count)) = grouped.last_mut() {
                        if *last_name == name {
                            *count += 1;
                            continue;
                        }
                    }
                    grouped.push((name, 1));
                }
                let weapons_summary = grouped
                    .iter()
                    .map(|(name, count)| {
                        if *count > 1 { format!("{} x{}", name, count) } else { name.clone() }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");

                body.row(TABLE_ROW_HEIGHT, |mut row| {
                    row.col(|ui| { ui.label(displayed_wave.to_string()); });
                    row.col(|ui| { ui.label(filtered_kills.len().to_string()); });
                    row.col(|ui| { ui.label(format_game_time(&start_dur)); });
                    row.col(|ui| { ui.label(format_duration_ms(&total_dur)); });
                    row.col(|ui| { ui.label(&weapons_summary); });
                });

                for (i, (time, weapon, victim_id)) in filtered_kills.iter().enumerate() {
                    let delta_label = if i == 0 {
                        "  —".to_string()
                    } else {
                        let prev_time = &filtered_kills[i - 1].0;
                        let delta = time.viewdemo_offset
                            .checked_sub(prev_time.viewdemo_offset)
                            .unwrap_or(Duration::ZERO);
                        format!("  +{}", format_duration_ms(&delta))
                    };

                    body.row(TABLE_ROW_HEIGHT, |mut row| {
                        row.col(|ui| {
                            ui.label(
                                egui::RichText::new(format!("  ↳ {}", i + 1))
                                    .small(),
                            );
                        });
                        row.col(|_ui| {});
                        row.col(|ui| {
                            ui.label(
                                egui::RichText::new(format_game_time(&time.viewdemo_offset))
                                    .small(),
                            );
                        });
                        row.col(|ui| {
                            ui.label(egui::RichText::new(&delta_label).small());
                        });
                        row.col(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(weapon_name(weapon))
                                        .small(),
                                );
                                ui.label(
                                    egui::RichText::new(" ⚔️ ")
                                        .small(),
                                );
                                if let Some(victim) = analysis.state.players.iter().find(|pl| pl.id == *victim_id) {
                                    let name_color = match &victim.team {
                                        Some(Team::Allies) => ALLIES_COLOR,
                                        Some(Team::Axis) => AXIS_COLOR,
                                        Some(Team::British) => BRITISH_COLOR,
                                        _ => egui::Color32::LIGHT_BLUE,
                                    };
                                    let label = egui::RichText::new(&victim.name)
                                        .color(name_color)
                                        .small()
                                        .strong();
                                    
                                    let resp = ui.add(
                                        egui::Label::new(label)
                                            .sense(egui::Sense::click())
                                    );
                                    if resp.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                    if resp.clicked() {
                                        if player_highlighting.highlighted.contains(&victim.id) {
                                            player_highlighting.highlighted.remove(&victim.id);
                                        } else {
                                            player_highlighting.highlighted.clear();
                                            player_highlighting.highlighted.insert(victim.id.clone());
                                        }
                                    }
                                } else {
                                    ui.label(
                                        egui::RichText::new(t("#app_team_unknown"))
                                            .small(),
                                    );
                                }
                            });
                        });
                    });
                }
            }
        });
}

fn format_game_time(d: &Duration) -> String {
    let total_secs = d.as_secs();
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    let millis = d.subsec_millis();
    let centis = millis / 10;
    format!("{:02}:{:02}:{:02}", mins, secs, centis)
}

fn format_duration_ms(d: &Duration) -> String {
    let total_secs = d.as_secs();
    let millis = d.subsec_millis();
    let centis = millis / 10;
    
    if total_secs >= 60 {
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{}:{:02}.{:02}s", mins, secs, centis)
    } else {
        format!("{}.{:02}s", total_secs, centis)
    }
}
