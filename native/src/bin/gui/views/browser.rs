use egui::{Align, Layout, Ui, Context};
use egui_extras::{Column, TableBuilder};
use std::path::PathBuf;
use crate::tree::DemoListItem;
use crate::types::BrowserView;
use crate::views::t;
use crate::SortColumn;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VisibleNode {
    VirtualFolder {
        name: String,
        id: String,
    },
    DemoFile(DemoListItem),
}

pub fn browser_ui(
    ctx: &Context,
    ui: &mut Ui,
    state: &mut crate::Gui,
    analyze_target_file: &mut Option<PathBuf>,
) {
    let selected_path = state.selected_analysis_path.clone();

    // View select combo-box
    ui.horizontal(|ui| {
        ui.label("View:");
        let combo = egui::ComboBox::from_id_salt("browser_view_select")
            .selected_text(match state.browser_view {
                BrowserView::Flat => "Flat List",
                BrowserView::GroupByMatch => "Group by Match",
                BrowserView::GroupByPlayer => "Group by Player/Recorder",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.browser_view, BrowserView::Flat, "Flat List");
                ui.selectable_value(&mut state.browser_view, BrowserView::GroupByMatch, "Group by Match");
                ui.selectable_value(&mut state.browser_view, BrowserView::GroupByPlayer, "Group by Player/Recorder");
            });

        if combo.response.changed() {
            if let Some(ref selected_path_str) = selected_path {
                let mut found_parent = None;
                match state.browser_view {
                    BrowserView::Flat => {}
                    BrowserView::GroupByMatch => {
                        if let Some(cached) = state.cache.demos.get(selected_path_str) {
                            let roster_hash = cached.player_roster_hash.unwrap_or(0);
                            let server_ip = cached.server_ip.clone().unwrap_or_default();
                            let map_name = cached.map_name.clone();
                            found_parent = Some(format!("{}-{}-{}", map_name, server_ip, roster_hash));
                        }
                    }
                    BrowserView::GroupByPlayer => {
                        if let Some(cached) = state.cache.demos.get(selected_path_str) {
                            let rec = cached.recorder_id.clone().unwrap_or_else(|| "Unknown".to_string());
                            found_parent = Some(rec);
                        }
                    }
                }

                if let Some(parent_id) = found_parent {
                    let header_id = egui::Id::new(&parent_id);
                    let mut collapsing_state = egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        header_id,
                        false,
                    );
                    collapsing_state.set_open(true);
                    collapsing_state.store(ui.ctx());
                } else if state.browser_view != BrowserView::Flat {
                    state.selected_analysis_path = None;
                }
            }
        }
    });
    ui.add_space(4.0);

    let mut display_files: Vec<DemoListItem> = state.desktop_files.iter()
        .filter(|item| {
            let path_str = item.path.to_string_lossy();
            state.filter_demo(&item.name, &item.map_name, &item.date, &path_str)
        })
        .cloned()
        .collect();

    if let Some(col) = state.sort_column {
        display_files.sort_by(|a, b| {
            let path_a = a.path.to_string_lossy();
            let path_b = b.path.to_string_lossy();
            let type_a = if let Some((_, analysis)) = state.analyses.get(path_a.as_ref()) {
                analysis.demo_info.demo_type.as_str()
            } else if let Some(cached) = state.cache.demos.get(path_a.as_ref()) {
                cached.demo_type.as_str()
            } else if a.name.to_lowercase().contains("hltv") {
                "HLTV"
            } else {
                "POV"
            };
            let type_b = if let Some((_, analysis)) = state.analyses.get(path_b.as_ref()) {
                analysis.demo_info.demo_type.as_str()
            } else if let Some(cached) = state.cache.demos.get(path_b.as_ref()) {
                cached.demo_type.as_str()
            } else if b.name.to_lowercase().contains("hltv") {
                "HLTV"
            } else {
                "POV"
            };

            let cmp = match col {
                SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortColumn::Type => type_a.cmp(type_b),
                SortColumn::Map => a.map_name.to_lowercase().cmp(&b.map_name.to_lowercase()),
                SortColumn::Date => a.date.cmp(&b.date),
            };

            if state.sort_ascending {
                cmp
            } else {
                cmp.reverse()
            }
        });
    }

    // Build VisibleNode projection
    state.visible_nodes.clear();

    match state.browser_view {
        BrowserView::Flat => {
            for item in &display_files {
                state.visible_nodes.push(VisibleNode::DemoFile(item.clone()));
            }
        }
        BrowserView::GroupByMatch => {
            let mut matches_groups: std::collections::HashMap<String, Vec<DemoListItem>> = std::collections::HashMap::new();
            for item in &display_files {
                let path_str = item.path.to_string_lossy().into_owned();
                let (map_name, server_ip, roster_hash) = if let Some(cached) = state.cache.demos.get(&path_str) {
                    (
                        cached.map_name.clone(),
                        cached.server_ip.clone().unwrap_or_default(),
                        cached.player_roster_hash.unwrap_or(0),
                    )
                } else {
                    (item.map_name.clone(), String::new(), 0)
                };
                let group_key = format!("{}-{}-{}", map_name, server_ip, roster_hash);
                matches_groups.entry(group_key).or_default().push(item.clone());
            }

            let mut sorted_groups: Vec<(String, String, Vec<DemoListItem>)> = matches_groups.into_iter().map(|(key, mut items)| {
                items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                let first = &items[0];
                let path_str = first.path.to_string_lossy().into_owned();
                let (map_name, server_ip, roster_hash) = if let Some(cached) = state.cache.demos.get(&path_str) {
                    (
                        cached.map_name.clone(),
                        cached.server_ip.clone().unwrap_or_default(),
                        cached.player_roster_hash.unwrap_or(0),
                    )
                } else {
                    (first.map_name.clone(), String::new(), 0)
                };
                let display_name = format!("🎮 Match: {} — {} (Roster: {:x})", map_name, if server_ip.is_empty() { "Unknown IP" } else { &server_ip }, roster_hash);
                (key, display_name, items)
            }).collect();

            sorted_groups.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));

            for (key, display_name, items) in sorted_groups {
                state.visible_nodes.push(VisibleNode::VirtualFolder {
                    name: display_name.clone(),
                    id: key.clone(),
                });

                let header_id = egui::Id::new(&key);
                let collapsing_state = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    header_id,
                    false,
                );
                if collapsing_state.is_open() {
                    for item in items {
                        state.visible_nodes.push(VisibleNode::DemoFile(item));
                    }
                }
            }
        }
        BrowserView::GroupByPlayer => {
            let mut player_groups: std::collections::HashMap<String, Vec<DemoListItem>> = std::collections::HashMap::new();
            for item in &display_files {
                let path_str = item.path.to_string_lossy().into_owned();
                let rec_id = if let Some(cached) = state.cache.demos.get(&path_str) {
                    cached.recorder_id.clone().unwrap_or_else(|| "Unknown".to_string())
                } else {
                    "Unknown".to_string()
                };
                player_groups.entry(rec_id).or_default().push(item.clone());
            }

            let mut sorted_groups: Vec<(String, String, Vec<DemoListItem>)> = player_groups.into_iter().map(|(key, mut items)| {
                items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                let display_name = format!("🧑 Recorder: {}", key);
                (key, display_name, items)
            }).collect();

            sorted_groups.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));

            for (key, display_name, items) in sorted_groups {
                state.visible_nodes.push(VisibleNode::VirtualFolder {
                    name: display_name.clone(),
                    id: key.clone(),
                });

                let header_id = egui::Id::new(&key);
                let collapsing_state = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    header_id,
                    false,
                );
                if collapsing_state.is_open() {
                    for item in items {
                        state.visible_nodes.push(VisibleNode::DemoFile(item));
                    }
                }
            }
        }
    }

    // Keyboard navigation for the Demos List
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut move_selection = 0;
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            move_selection = 1;
        } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            move_selection = -1;
        }

        if move_selection != 0 && !state.visible_nodes.is_empty() {
            let mut current_idx = None;
            for (i, node) in state.visible_nodes.iter().enumerate() {
                match node {
                    VisibleNode::VirtualFolder { id, .. } => {
                        if state.selected_folder_id.as_ref() == Some(id) {
                            current_idx = Some(i);
                            break;
                        }
                    }
                    VisibleNode::DemoFile(item) => {
                        let path_str = item.path.to_string_lossy();
                        if state.selected_analysis_path.as_deref() == Some(path_str.as_ref()) {
                            current_idx = Some(i);
                            break;
                        }
                    }
                }
            }

            let new_idx = if let Some(idx) = current_idx {
                (idx as isize + move_selection)
                    .clamp(0, (state.visible_nodes.len() - 1) as isize)
                    as usize
            } else {
                if move_selection > 0 {
                    0
                } else {
                    state.visible_nodes.len() - 1
                }
            };

            if current_idx != Some(new_idx) {
                match &state.visible_nodes[new_idx] {
                    VisibleNode::VirtualFolder { id, .. } => {
                        state.selected_folder_id = Some(id.clone());
                        state.selected_analysis_path = None;
                        state.selection_changed_via_keyboard = true;
                    }
                    VisibleNode::DemoFile(item) => {
                        state.selected_folder_id = None;
                        state.selected_analysis_path = Some(item.path.to_string_lossy().into_owned());
                        state.selection_changed_via_keyboard = true;
                    }
                }
            }
        }

        // Trigger actual analysis when Enter is pressed on the selected file
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            if let Some(path_str) = &state.selected_analysis_path {
                *analyze_target_file = Some(std::path::PathBuf::from(path_str));
            }
        }
    }

    egui::ScrollArea::horizontal().show(ui, |ui| {
        TableBuilder::new(ui)
            .striped(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::initial(300.0).resizable(true).clip(true)) // Name
            .column(Column::initial(80.0).resizable(true)) // Type
            .column(Column::initial(150.0).resizable(true)) // Map
            .column(Column::initial(150.0)) // Date
            .header(20.0, |mut header| {
                header.col(|ui| {
                    let label = match (state.sort_column, state.sort_ascending) {
                        (Some(SortColumn::Name), true) => format!("{} ⏶", t("#app_col_name")),
                        (Some(SortColumn::Name), false) => format!("{} ⏷", t("#app_col_name")),
                        _ => t("#app_col_name"),
                    };
                    if ui.add(egui::Button::new(label).frame(false)).clicked() {
                        state.toggle_sort(SortColumn::Name);
                    }
                });
                header.col(|ui| {
                    let label = match (state.sort_column, state.sort_ascending) {
                        (Some(SortColumn::Type), true) => format!("{} ⏶", t("#app_col_type")),
                        (Some(SortColumn::Type), false) => format!("{} ⏷", t("#app_col_type")),
                        _ => t("#app_col_type"),
                    };
                    if ui.add(egui::Button::new(label).frame(false)).clicked() {
                        state.toggle_sort(SortColumn::Type);
                    }
                });
                header.col(|ui| {
                    let label = match (state.sort_column, state.sort_ascending) {
                        (Some(SortColumn::Map), true) => format!("{} ⏶", t("#app_col_map")),
                        (Some(SortColumn::Map), false) => format!("{} ⏷", t("#app_col_map")),
                        _ => t("#app_col_map"),
                    };
                    if ui.add(egui::Button::new(label).frame(false)).clicked() {
                        state.toggle_sort(SortColumn::Map);
                    }
                });
                header.col(|ui| {
                    let label = match (state.sort_column, state.sort_ascending) {
                        (Some(SortColumn::Date), true) => format!("{} ⏶", t("#app_col_date")),
                        (Some(SortColumn::Date), false) => format!("{} ⏷", t("#app_col_date")),
                        _ => t("#app_col_date"),
                    };
                    if ui.add(egui::Button::new(label).frame(false)).clicked() {
                        state.toggle_sort(SortColumn::Date);
                    }
                });
            })
            .body(|mut body| {
                if state.desktop_files.is_empty() {
                    body.row(18.0, |mut row| {
                        row.col(|ui| {
                            ui.weak(t("#app_no_demos_found"));
                        });
                        row.col(|_| {});
                        row.col(|_| {});
                        row.col(|_| {});
                    });
                } else if display_files.is_empty() {
                    body.row(18.0, |mut row| {
                        row.col(|ui| {
                            ui.weak(t("#app_no_matching_demos"));
                        });
                        row.col(|_| {});
                        row.col(|_| {});
                        row.col(|_| {});
                    });
                } else {
                    for node in &state.visible_nodes {
                        match node {
                            VisibleNode::VirtualFolder { name, id } => {
                                let is_folder_selected = state.selected_folder_id.as_ref() == Some(id);
                                body.row(18.0, |mut row| {
                                    row.set_selected(is_folder_selected);
                                    row.col(|ui| {
                                        let header_id = egui::Id::new(id);
                                        let mut collapsing_state = egui::collapsing_header::CollapsingState::load_with_default_open(
                                            ui.ctx(),
                                            header_id,
                                            false,
                                        );
                                        ui.horizontal(|ui| {
                                            let symbol = if collapsing_state.is_open() { "⏷" } else { "⏵" };
                                            if ui.selectable_label(is_folder_selected, symbol).clicked() {
                                                collapsing_state.toggle(ui);
                                                collapsing_state.store(ui.ctx());
                                            }
                                            let label_res = ui.selectable_label(is_folder_selected, name);
                                            if label_res.clicked() {
                                                state.selected_folder_id = Some(id.clone());
                                                state.selected_analysis_path = None;
                                            }
                                            if is_folder_selected && state.selection_changed_via_keyboard {
                                                label_res.scroll_to_me(Some(egui::Align::Center));
                                            }
                                        });
                                    });
                                    row.col(|_| {});
                                    row.col(|_| {});
                                    row.col(|_| {});
                                });
                            }
                            VisibleNode::DemoFile(item) => {
                                let path_str = item.path.to_string_lossy().into_owned();
                                let is_selected = selected_path.as_ref() == Some(&path_str);
                                let is_loading = state.loading_path.as_deref() == Some(path_str.as_str());

                                body.row(18.0, |mut row| {
                                    row.set_selected(is_selected);
                                    let mut response = None;
                                    row.col(|ui| {
                                        ui.horizontal(|ui| {
                                            if is_loading {
                                                ui.spinner();
                                            }
                                            let label_res = ui.selectable_label(
                                                is_selected,
                                                format!("  📄 {}", item.name),
                                            );
                                            if label_res.clicked() {
                                                if !is_selected {
                                                    *analyze_target_file = Some(item.path.clone());
                                                }
                                                state.selected_folder_id = None;
                                            }
                                            response = Some(label_res);
                                        });
                                    });
                                    row.col(|ui| {
                                        let demo_type = if let Some((_, analysis)) = state.analyses.get(&path_str) {
                                            analysis.demo_info.demo_type.as_str()
                                        } else if let Some(cached) = state.cache.demos.get(&path_str) {
                                            cached.demo_type.as_str()
                                        } else if item.name.to_lowercase().contains("hltv") {
                                            "HLTV"
                                        } else {
                                            "POV"
                                        };
                                        ui.label(demo_type);
                                    });
                                    row.col(|ui| {
                                        ui.label(&item.map_name);
                                    });
                                    row.col(|ui| {
                                        ui.label(&item.date);
                                    });

                                    if is_selected && state.selection_changed_via_keyboard {
                                        if let Some(resp) = response {
                                            resp.scroll_to_me(Some(egui::Align::Center));
                                        }
                                    }
                                });
                            }
                        }
                    }
                }
            });
    });
    state.selection_changed_via_keyboard = false;
}
