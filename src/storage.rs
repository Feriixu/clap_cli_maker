use crate::model::Project;
use std::fs;
use std::io;
use std::path::PathBuf;

pub fn projects_dir() -> PathBuf {
    let base = dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("clap_cli_maker").join("projects")
}

pub fn generated_dir() -> PathBuf {
    projects_dir().join("generated")
}

fn ensure_dir(dir: &PathBuf) -> io::Result<()> {
    fs::create_dir_all(dir)
}

pub fn list_projects() -> Vec<Project> {
    let dir = projects_dir();
    if ensure_dir(&dir).is_err() {
        return Vec::new();
    }
    let mut projects = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json")
                && let Ok(text) = fs::read_to_string(&path)
                    && let Ok(project) = serde_json::from_str::<Project>(&text) {
                        projects.push(project);
                    }
        }
    }
    projects.sort_by_key(|p| p.name.to_lowercase());
    projects
}

pub fn project_path(project: &Project) -> PathBuf {
    projects_dir().join(format!("{}.json", project.id))
}

pub fn save_project(project: &Project) -> io::Result<()> {
    ensure_dir(&projects_dir())?;
    let text = serde_json::to_string_pretty(project)
        .map_err(io::Error::other)?;
    fs::write(project_path(project), text)
}

pub fn delete_project(project: &Project) -> io::Result<()> {
    let path = project_path(project);
    if path.exists() {
        fs::remove_file(path)
    } else {
        Ok(())
    }
}

pub fn save_generated_code(project: &Project, code: &str) -> io::Result<PathBuf> {
    let dir = generated_dir();
    ensure_dir(&dir)?;
    let filename = format!("{}.rs", project.root.name.replace(['/', '\\', ' '], "_"));
    let path = dir.join(filename);
    fs::write(&path, code)?;
    Ok(path)
}

/// Opens the projects folder in the OS's file manager.
pub fn open_projects_folder() -> io::Result<()> {
    let dir = projects_dir();
    ensure_dir(&dir)?;
    opener::open(&dir).map_err(io::Error::other)
}
