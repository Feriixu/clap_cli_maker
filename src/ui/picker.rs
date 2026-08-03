use crate::app::Action;
use crate::model::Project;
use crate::storage;
use uuid::Uuid;

pub struct PickerState {
    projects: Vec<Project>,
    new_project_name: String,
    pending_delete: Option<Uuid>,
    error: Option<String>,
}

impl Default for PickerState {
    fn default() -> Self {
        Self::new()
    }
}

impl PickerState {
    pub fn new() -> Self {
        Self {
            projects: storage::list_projects(),
            new_project_name: String::new(),
            pending_delete: None,
            error: None,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut PickerState) -> Option<Action> {
    let mut action = None;

    egui::CentralPanel::default().show(ui, |ui| {
        ui.heading("Clap CLI Maker");
        ui.label("Pick a project to edit, or create a new one.");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.button("Open projects folder").clicked()
                && let Err(e) = storage::open_projects_folder() {
                    state.error = Some(format!("Couldn't open folder: {e}"));
                }
            if ui.button("Refresh").clicked() {
                state.projects = storage::list_projects();
                state.error = None;
            }
        });
        ui.label(
            egui::RichText::new(format!("{}", storage::projects_dir().display()))
                .weak()
                .small(),
        );

        ui.separator();

        ui.horizontal(|ui| {
            ui.label("New project name:");
            ui.text_edit_singleline(&mut state.new_project_name);
            let can_create = !state.new_project_name.trim().is_empty();
            if ui
                .add_enabled(can_create, egui::Button::new("Create"))
                .clicked()
            {
                let project = Project::new(state.new_project_name.trim().to_string());
                match storage::save_project(&project) {
                    Ok(()) => {
                        state.new_project_name.clear();
                        action = Some(Action::OpenProject(project));
                    }
                    Err(e) => state.error = Some(format!("Failed to save project: {e}")),
                }
            }
        });

        if let Some(err) = &state.error {
            ui.colored_label(egui::Color32::from_rgb(220, 80, 80), err);
        }

        ui.separator();
        ui.heading("Projects");

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut refresh = false;
            for project in &state.projects {
                ui.horizontal(|ui| {
                    if ui.button(&project.name).clicked() {
                        action = Some(Action::OpenProject(project.clone()));
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "bin: {}  ·  {} subcommand(s)",
                            project.root.name,
                            project.root.subcommands.len()
                        ))
                        .weak(),
                    );

                    if state.pending_delete == Some(project.id) {
                        if ui.button("Confirm delete").clicked() {
                            let _ = storage::delete_project(project);
                            refresh = true;
                        }
                        if ui.button("Cancel").clicked() {
                            state.pending_delete = None;
                        }
                    } else if ui.button("Delete").clicked() {
                        state.pending_delete = Some(project.id);
                    }
                });
            }
            if refresh {
                state.projects = storage::list_projects();
                state.pending_delete = None;
            }
            if state.projects.is_empty() {
                ui.label("No projects yet — create one above.");
            }
        });
    });

    action
}
