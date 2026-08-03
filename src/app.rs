use crate::model::Project;
use crate::ui::{editor, picker};

pub enum Screen {
    Picker(picker::PickerState),
    Editor(Box<editor::EditorState>),
}

/// Cross-screen navigation requests emitted by a screen's `show` function.
pub enum Action {
    OpenProject(Project),
    BackToPicker(Project),
}

pub struct CliMakerApp {
    screen: Screen,
}

impl Default for CliMakerApp {
    fn default() -> Self {
        Self::new()
    }
}

impl CliMakerApp {
    pub fn new() -> Self {
        Self {
            screen: Screen::Picker(picker::PickerState::new()),
        }
    }
}

impl eframe::App for CliMakerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let action = match &mut self.screen {
            Screen::Picker(state) => picker::show(ui, state),
            Screen::Editor(state) => editor::show(ui, state),
        };

        match action {
            Some(Action::OpenProject(project)) => {
                self.screen = Screen::Editor(Box::new(editor::EditorState::new(project)));
            }
            Some(Action::BackToPicker(project)) => {
                let _ = crate::storage::save_project(&project);
                self.screen = Screen::Picker(picker::PickerState::new());
            }
            None => {}
        }
    }
}
