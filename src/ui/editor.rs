use crate::app::Action;
use crate::codegen;
use crate::model::{ArgDef, ArgKind, CommandNode, FlattenGroup, FlattenRef, Project, RustType};
use crate::storage;
use heck::ToSnakeCase;
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Selection {
    Command(Uuid),
    Group(Uuid),
}

pub struct EditorState {
    project: Project,
    selected: Selection,
    last_saved_json: String,
    last_raw_code: String,
    cached_code: String,
    status: Option<String>,
}

impl EditorState {
    pub fn new(project: Project) -> Self {
        let selected = Selection::Command(project.root.id);
        let last_saved_json = serde_json::to_string(&project).unwrap_or_default();
        Self {
            project,
            selected,
            last_saved_json,
            last_raw_code: String::new(),
            cached_code: String::new(),
            status: None,
        }
    }

    fn is_dirty(&self) -> bool {
        serde_json::to_string(&self.project).unwrap_or_default() != self.last_saved_json
    }

    fn save(&mut self) {
        match storage::save_project(&self.project) {
            Ok(()) => {
                self.last_saved_json = serde_json::to_string(&self.project).unwrap_or_default();
                self.status = Some("Saved.".to_string());
            }
            Err(e) => self.status = Some(format!("Save failed: {e}")),
        }
    }
}

enum TreeOp {
    Select(Uuid),
    AddChild(Uuid),
    Delete(Uuid),
    MoveUp(Uuid),
    MoveDown(Uuid),
}

enum GroupOp {
    Select(Uuid),
    New,
    Delete(Uuid),
}

pub fn show(ui: &mut egui::Ui, state: &mut EditorState) -> Option<Action> {
    let mut action = None;

    egui::Panel::top("editor_top").show(ui, |ui| {
        ui.horizontal(|ui| {
            if ui.button("< Projects").clicked() {
                action = Some(Action::BackToPicker(state.project.clone()));
            }
            ui.separator();
            ui.heading(&state.project.name);
            if state.is_dirty() {
                ui.colored_label(egui::Color32::from_rgb(230, 180, 40), "unsaved changes");
            }
            if ui.button("Save").clicked() {
                state.save();
            }
            if let Some(status) = state.status.clone() {
                ui.label(egui::RichText::new(status).weak());
            }
        });
    });

    if action.is_some() {
        return action;
    }

    egui::Panel::left("editor_tree")
        .min_size(260.0)
        .show(ui, |ui| {
            ui.heading("Commands");
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("tree_scroll")
                .max_height(ui.available_height() * 0.55)
                .show(ui, |ui| {
                    let root_id = state.project.root.id;
                    let selected_command = match state.selected {
                        Selection::Command(id) => Some(id),
                        Selection::Group(_) => None,
                    };
                    let mut ops: Vec<TreeOp> = Vec::new();
                    render_tree(ui, &state.project.root, selected_command, 0, root_id, &mut ops);
                    apply_tree_ops(state, ops);
                });

            ui.separator();
            ui.heading("Shared arg groups");
            egui::ScrollArea::vertical()
                .id_salt("groups_scroll")
                .show(ui, |ui| {
                    let selected_group = match state.selected {
                        Selection::Group(id) => Some(id),
                        Selection::Command(_) => None,
                    };
                    let mut ops: Vec<GroupOp> = Vec::new();
                    for group in &state.project.flatten_groups {
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(
                                    selected_group == Some(group.id),
                                    group.display_ident(),
                                )
                                .clicked()
                            {
                                ops.push(GroupOp::Select(group.id));
                            }
                            if ui.small_button("delete").clicked() {
                                ops.push(GroupOp::Delete(group.id));
                            }
                        });
                    }
                    if ui.button("+ New group").clicked() {
                        ops.push(GroupOp::New);
                    }
                    apply_group_ops(state, ops);
                });
        });

    egui::Panel::right("editor_code")
        .min_size(440.0)
        .show(ui, |ui| {
            ui.heading("Generated Code");
            let raw = codegen::generate_source(&state.project);
            if raw != state.last_raw_code {
                state.cached_code = codegen::format_source(&raw);
                state.last_raw_code = raw;
            }
            ui.horizontal(|ui| {
                if ui.button("Copy to clipboard").clicked() {
                    ui.ctx().copy_text(state.cached_code.clone());
                }
                if ui.button("Save to file").clicked() {
                    match storage::save_generated_code(&state.project, &state.cached_code) {
                        Ok(path) => state.status = Some(format!("Wrote {}", path.display())),
                        Err(e) => state.status = Some(format!("Failed to save code: {e}")),
                    }
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("code_scroll")
                .show(ui, |ui| {
                    let mut display = state.cached_code.clone();
                    let theme = egui_code_editor::ColorTheme::GITHUB_DARK;
                    theme.modify_style(ui, 13.0);
                    let highlighter = egui_code_editor::CodeEditor::default()
                        .with_theme(theme)
                        .with_fontsize(13.0);
                    let syntax = egui_code_editor::Syntax::rust();
                    let egui_ctx = ui.ctx().clone();
                    let mut layouter = move |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                        let (mut job, _links) = egui_code_editor::highlighting::highlight(
                            &egui_ctx,
                            &highlighter,
                            text.as_str(),
                            &syntax,
                        );
                        job.wrap = egui::text::TextWrapping::wrap_at_width(wrap_width);
                        ui.fonts_mut(|f| f.layout_job(job))
                    };
                    ui.add(
                        egui::TextEdit::multiline(&mut display)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .interactive(false)
                            .layouter(&mut layouter),
                    );
                });
        });

    egui::CentralPanel::default().show(ui, |ui| {
        let selected = state.selected;
        egui::ScrollArea::vertical()
            .id_salt("props_scroll")
            .show(ui, |ui| match selected {
                Selection::Command(id) => {
                    let groups_summary: Vec<(Uuid, String)> = state
                        .project
                        .flatten_groups
                        .iter()
                        .map(|g| (g.id, g.display_ident()))
                        .collect();
                    let is_root = id == state.project.root.id;
                    if let Some(node) = state.project.root.find_mut(id) {
                        render_properties(ui, node, is_root, &groups_summary);
                    } else {
                        ui.label("Select a command from the tree on the left.");
                    }
                }
                Selection::Group(id) => {
                    if let Some(group) =
                        state.project.flatten_groups.iter_mut().find(|g| g.id == id)
                    {
                        render_group_properties(ui, group);
                    } else {
                        ui.label("Select a shared arg group on the left.");
                    }
                }
            });
    });

    action
}

fn render_tree(
    ui: &mut egui::Ui,
    node: &CommandNode,
    selected: Option<Uuid>,
    depth: usize,
    root_id: Uuid,
    ops: &mut Vec<TreeOp>,
) {
    let is_root = node.id == root_id;
    ui.horizontal(|ui| {
        ui.add_space(depth as f32 * 16.0);
        let label = if is_root {
            format!("{} (root)", node.name)
        } else {
            node.name.clone()
        };
        if ui.selectable_label(selected == Some(node.id), label).clicked() {
            ops.push(TreeOp::Select(node.id));
        }
    });
    ui.horizontal(|ui| {
        ui.add_space(depth as f32 * 16.0 + 16.0);
        if ui.small_button("+ sub").clicked() {
            ops.push(TreeOp::AddChild(node.id));
        }
        if !is_root {
            if ui.small_button("up").clicked() {
                ops.push(TreeOp::MoveUp(node.id));
            }
            if ui.small_button("down").clicked() {
                ops.push(TreeOp::MoveDown(node.id));
            }
            if ui.small_button("delete").clicked() {
                ops.push(TreeOp::Delete(node.id));
            }
        }
    });
    for child in &node.subcommands {
        render_tree(ui, child, selected, depth + 1, root_id, ops);
    }
}

fn apply_tree_ops(state: &mut EditorState, ops: Vec<TreeOp>) {
    for op in ops {
        match op {
            TreeOp::Select(id) => state.selected = Selection::Command(id),
            TreeOp::AddChild(parent_id) => {
                if let Some(parent) = state.project.root.find_mut(parent_id) {
                    let new_node = CommandNode::new_sub("new-command");
                    let new_id = new_node.id;
                    parent.subcommands.push(new_node);
                    state.selected = Selection::Command(new_id);
                }
            }
            TreeOp::Delete(id) => {
                if state.selected == Selection::Command(id) {
                    state.selected = Selection::Command(state.project.root.id);
                }
                state.project.root.remove_child(id);
            }
            TreeOp::MoveUp(id) => {
                state.project.root.move_child(id, -1);
            }
            TreeOp::MoveDown(id) => {
                state.project.root.move_child(id, 1);
            }
        }
    }
}

fn apply_group_ops(state: &mut EditorState, ops: Vec<GroupOp>) {
    for op in ops {
        match op {
            GroupOp::Select(id) => state.selected = Selection::Group(id),
            GroupOp::New => {
                let n = state.project.flatten_groups.len() + 1;
                let group = FlattenGroup::new(&format!("Shared Args {n}"));
                let new_id = group.id;
                state.project.flatten_groups.push(group);
                state.selected = Selection::Group(new_id);
            }
            GroupOp::Delete(id) => {
                if state.selected == Selection::Group(id) {
                    state.selected = Selection::Command(state.project.root.id);
                }
                state.project.remove_flatten_group(id);
            }
        }
    }
}

fn render_properties(
    ui: &mut egui::Ui,
    node: &mut CommandNode,
    is_root: bool,
    groups: &[(Uuid, String)],
) {
    ui.heading(if is_root { "Root command" } else { "Subcommand" });

    egui::Grid::new("node_meta")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label(if is_root { "Binary name" } else { "Name" });
            ui.text_edit_singleline(&mut node.name);
            ui.end_row();

            ui.label("Ident override");
            let mut ident_text = node.ident_override.clone().unwrap_or_default();
            if ui.text_edit_singleline(&mut ident_text).changed() {
                node.ident_override = if ident_text.trim().is_empty() {
                    None
                } else {
                    Some(ident_text)
                };
            }
            ui.end_row();

            ui.label("About");
            ui.text_edit_multiline(&mut node.about);
            ui.end_row();
        });

    ui.separator();
    ui.checkbox(
        &mut node.require_subcommand,
        "A subcommand is required (only applies once this command has subcommands)",
    );
    ui.checkbox(
        &mut node.args_conflicts_with_subcommands,
        "This command's own args/flatten groups don't conflict with its subcommand \
         (args_conflicts_with_subcommands)",
    );
    ui.checkbox(
        &mut node.subcommand_negates_reqs,
        "Giving a subcommand relaxes this command's required args/flatten groups \
         (subcommand_negates_reqs)",
    );

    render_args_list(ui, &mut node.args);
    render_flatten_refs(ui, &mut node.flattens, groups);
}

fn render_group_properties(ui: &mut egui::Ui, group: &mut FlattenGroup) {
    ui.heading("Shared arg group");

    egui::Grid::new("group_meta")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut group.name);
            ui.end_row();

            ui.label("Ident override");
            let mut ident_text = group.ident_override.clone().unwrap_or_default();
            if ui.text_edit_singleline(&mut ident_text).changed() {
                group.ident_override = if ident_text.trim().is_empty() {
                    None
                } else {
                    Some(ident_text)
                };
            }
            ui.end_row();
        });

    render_args_list(ui, &mut group.args);
}

fn render_flatten_refs(ui: &mut egui::Ui, flattens: &mut Vec<FlattenRef>, groups: &[(Uuid, String)]) {
    ui.separator();
    ui.heading("Flattened arg groups");
    if groups.is_empty() {
        ui.label("No shared arg groups yet — create one in the left panel first.");
    }

    let mut remove_idx = None;
    for (i, fref) in flattens.iter_mut().enumerate() {
        ui.push_id(fref.id, |ui| {
            ui.horizontal(|ui| {
                let current_label = groups
                    .iter()
                    .find(|(id, _)| *id == fref.group_id)
                    .map(|(_, name)| name.clone())
                    .unwrap_or_else(|| "<deleted group>".to_string());
                egui::ComboBox::from_id_salt("group")
                    .selected_text(current_label)
                    .show_ui(ui, |ui| {
                        for (id, name) in groups {
                            ui.selectable_value(&mut fref.group_id, *id, name);
                        }
                    });
                ui.checkbox(&mut fref.optional, "optional");
                ui.label("field name:");
                let mut field_text = fref.field_name_override.clone().unwrap_or_default();
                if ui
                    .add(egui::TextEdit::singleline(&mut field_text).desired_width(100.0))
                    .changed()
                {
                    fref.field_name_override = if field_text.trim().is_empty() {
                        None
                    } else {
                        Some(field_text)
                    };
                }
                if ui.small_button("remove").clicked() {
                    remove_idx = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove_idx {
        flattens.remove(i);
    }
    if ui
        .add_enabled(!groups.is_empty(), egui::Button::new("+ Add flatten"))
        .clicked()
    {
        flattens.push(FlattenRef::new(groups[0].0));
    }
}

fn render_args_list(ui: &mut egui::Ui, args: &mut Vec<ArgDef>) {
    ui.separator();
    ui.heading("Arguments & options");

    let mut seen = std::collections::HashSet::new();
    let mut dupes = std::collections::HashSet::new();
    for arg in args.iter() {
        let normalized = arg.name.to_snake_case();
        if !seen.insert(normalized.clone()) {
            dupes.insert(normalized);
        }
    }
    if !dupes.is_empty() {
        ui.colored_label(
            egui::Color32::from_rgb(220, 80, 80),
            format!(
                "Duplicate field name(s): {} — generated code won't compile.",
                dupes.into_iter().collect::<Vec<_>>().join(", ")
            ),
        );
    }

    let siblings: Vec<(Uuid, String)> = args.iter().map(|a| (a.id, a.name.clone())).collect();

    let mut remove_idx = None;
    for (i, arg) in args.iter_mut().enumerate() {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            render_arg(ui, arg, &siblings);
            if ui.small_button("Remove this argument").clicked() {
                remove_idx = Some(i);
            }
        });
        ui.add_space(4.0);
    }
    if let Some(i) = remove_idx {
        let removed_id = args[i].id;
        args.remove(i);
        for arg in args.iter_mut() {
            arg.conflicts_with.retain(|id| *id != removed_id);
        }
    }
    ui.horizontal(|ui| {
        if ui.button("+ Add option/flag").clicked() {
            let n = args.len() + 1;
            args.push(ArgDef::new_named(&format!("option_{n}")));
        }
        if ui.button("+ Add positional").clicked() {
            let n = args.len() + 1;
            args.push(ArgDef::new_positional(&format!("arg_{n}")));
        }
    });
}

fn render_arg(ui: &mut egui::Ui, arg: &mut ArgDef, siblings: &[(Uuid, String)]) {
    ui.push_id(arg.id, |ui| {
        egui::Grid::new("arg_grid")
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label("Field name");
                ui.text_edit_singleline(&mut arg.name);
                ui.end_row();

                ui.label("Kind");
                egui::ComboBox::from_id_salt("kind")
                    .selected_text(match arg.kind {
                        ArgKind::Positional => "Positional",
                        ArgKind::Named => "Option/Flag",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut arg.kind, ArgKind::Positional, "Positional");
                        ui.selectable_value(&mut arg.kind, ArgKind::Named, "Option/Flag");
                    });
                ui.end_row();

                ui.label("Type");
                let basics = RustType::basic_types();
                egui::ComboBox::from_id_salt("type")
                    .selected_text(arg.ty.label())
                    .show_ui(ui, |ui| {
                        for t in &basics {
                            if ui.selectable_label(arg.ty == *t, t.label()).clicked() {
                                arg.ty = t.clone();
                            }
                        }
                        let is_custom = matches!(arg.ty, RustType::Custom(_));
                        if ui.selectable_label(is_custom, "Custom...").clicked() && !is_custom {
                            arg.ty = RustType::Custom(String::new());
                        }
                    });
                ui.end_row();

                if let RustType::Custom(custom) = &mut arg.ty {
                    ui.label("Custom type name");
                    ui.text_edit_singleline(custom);
                    ui.end_row();
                }

                ui.label("Help text");
                ui.text_edit_singleline(&mut arg.help);
                ui.end_row();

                let is_plain_bool = arg.is_plain_bool_flag();

                ui.label("Required");
                ui.add_enabled(
                    !is_plain_bool && arg.default_value.is_none(),
                    egui::Checkbox::new(&mut arg.required, ""),
                );
                ui.end_row();

                ui.label("Multiple values (Vec<T>)");
                ui.add_enabled(!is_plain_bool, egui::Checkbox::new(&mut arg.multiple, ""));
                ui.end_row();

                ui.label("Default value");
                let mut default_text = arg.default_value.clone().unwrap_or_default();
                ui.add_enabled_ui(!is_plain_bool, |ui| {
                    if ui.text_edit_singleline(&mut default_text).changed() {
                        arg.default_value = if default_text.is_empty() {
                            None
                        } else {
                            Some(default_text.clone())
                        };
                    }
                });
                ui.end_row();

                if matches!(arg.kind, ArgKind::Named) {
                    ui.label("Short flag");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut arg.short, "");
                        if arg.short {
                            let mut short_text =
                                arg.short_char.map(|c| c.to_string()).unwrap_or_default();
                            if ui
                                .add(egui::TextEdit::singleline(&mut short_text).desired_width(20.0))
                                .changed()
                            {
                                arg.short_char = short_text.chars().next();
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("Long flag");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut arg.long, "");
                        if arg.long {
                            let mut long_text = arg.long_name.clone().unwrap_or_default();
                            if ui.text_edit_singleline(&mut long_text).changed() {
                                arg.long_name = if long_text.is_empty() {
                                    None
                                } else {
                                    Some(long_text)
                                };
                            }
                        }
                    });
                    ui.end_row();
                }

                ui.label("Env var");
                let mut env_text = arg.env.clone().unwrap_or_default();
                if ui.text_edit_singleline(&mut env_text).changed() {
                    arg.env = if env_text.trim().is_empty() {
                        None
                    } else {
                        Some(env_text)
                    };
                }
                ui.end_row();
            });

        ui.collapsing("Choices (generates a ValueEnum type)", |ui| {
            let mut remove_choice = None;
            for (ci, choice) in arg.choices.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(choice);
                    if ui.small_button("x").clicked() {
                        remove_choice = Some(ci);
                    }
                });
            }
            if let Some(ci) = remove_choice {
                arg.choices.remove(ci);
            }
            if ui.small_button("+ choice").clicked() {
                arg.choices.push(String::new());
            }
        });

        let others: Vec<&(Uuid, String)> = siblings.iter().filter(|(id, _)| *id != arg.id).collect();
        ui.collapsing("Conflicts with", |ui| {
            if others.is_empty() {
                ui.label("No other args in this list yet.");
            }
            for (id, name) in &others {
                let mut checked = arg.conflicts_with.contains(id);
                if ui.checkbox(&mut checked, name.as_str()).changed() {
                    if checked {
                        arg.conflicts_with.push(*id);
                    } else {
                        arg.conflicts_with.retain(|c| c != id);
                    }
                }
            }
        });
    });
}
