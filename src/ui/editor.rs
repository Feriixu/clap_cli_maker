use crate::app::Action;
use crate::codegen;
use crate::model::{ArgDef, ArgKind, CommandNode, Project, RustType};
use crate::storage;
use heck::ToSnakeCase;
use uuid::Uuid;

pub struct EditorState {
    project: Project,
    selected: Uuid,
    last_saved_json: String,
    last_raw_code: String,
    cached_code: String,
    status: Option<String>,
}

impl EditorState {
    pub fn new(project: Project) -> Self {
        let selected = project.root.id;
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
                .show(ui, |ui| {
                    let root_id = state.project.root.id;
                    let mut ops: Vec<TreeOp> = Vec::new();
                    render_tree(ui, &state.project.root, state.selected, 0, root_id, &mut ops);
                    apply_tree_ops(state, ops);
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
                    ui.add(
                        egui::TextEdit::multiline(&mut display)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .interactive(false),
                    );
                });
        });

    egui::CentralPanel::default().show(ui, |ui| {
        let selected = state.selected;
        egui::ScrollArea::vertical()
            .id_salt("props_scroll")
            .show(ui, |ui| {
                let is_root = selected == state.project.root.id;
                if let Some(node) = state.project.root.find_mut(selected) {
                    render_properties(ui, node, is_root);
                } else {
                    ui.label("Select a command from the tree on the left.");
                }
            });
    });

    action
}

fn render_tree(
    ui: &mut egui::Ui,
    node: &CommandNode,
    selected: Uuid,
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
        if ui.selectable_label(selected == node.id, label).clicked() {
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
            TreeOp::Select(id) => state.selected = id,
            TreeOp::AddChild(parent_id) => {
                if let Some(parent) = state.project.root.find_mut(parent_id) {
                    let new_node = CommandNode::new_sub("new-command");
                    let new_id = new_node.id;
                    parent.subcommands.push(new_node);
                    state.selected = new_id;
                }
            }
            TreeOp::Delete(id) => {
                if state.selected == id {
                    state.selected = state.project.root.id;
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

fn render_properties(ui: &mut egui::Ui, node: &mut CommandNode, is_root: bool) {
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

    ui.separator();
    ui.heading("Arguments & options");

    let mut seen = std::collections::HashSet::new();
    let mut dupes = std::collections::HashSet::new();
    for arg in &node.args {
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

    let mut remove_idx = None;
    for i in 0..node.args.len() {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            render_arg(ui, &mut node.args[i]);
            if ui.small_button("Remove this argument").clicked() {
                remove_idx = Some(i);
            }
        });
        ui.add_space(4.0);
    }
    if let Some(i) = remove_idx {
        node.args.remove(i);
    }
    ui.horizontal(|ui| {
        if ui.button("+ Add option/flag").clicked() {
            let n = node.args.len() + 1;
            node.args.push(ArgDef::new_named(&format!("option_{n}")));
        }
        if ui.button("+ Add positional").clicked() {
            let n = node.args.len() + 1;
            node.args.push(ArgDef::new_positional(&format!("arg_{n}")));
        }
    });
}

fn render_arg(ui: &mut egui::Ui, arg: &mut ArgDef) {
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
    });
}
