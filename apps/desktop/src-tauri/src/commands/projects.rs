use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Manager, State};

use crate::core::projects::{ProjectManager, ProjectSummary};
use crate::database::ProjectRecord;
use crate::media::ffmpeg::probe_media_engine;
use crate::media::MediaEngineStatus;

#[tauri::command]
pub async fn get_default_projects_dir(app: AppHandle) -> Result<String, String> {
    let base = app
        .path()
        .document_dir()
        .or_else(|_| app.path().app_data_dir())
        .map_err(|e| format!("Unable to resolve default projects folder: {e}"))?;
    Ok(base.join("XakuStudioProjects").to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn get_media_engine_status() -> Result<MediaEngineStatus, String> {
    Ok(probe_media_engine().await)
}

#[tauri::command]
pub async fn list_projects(
    projects: State<'_, Arc<ProjectManager>>,
) -> Result<Vec<ProjectRecord>, String> {
    projects.list_projects().await
}

#[tauri::command]
pub async fn create_project(
    projects: State<'_, Arc<ProjectManager>>,
    parent_dir: String,
    name: String,
    source_lang: String,
    target_lang: String,
    preset: String,
) -> Result<ProjectSummary, String> {
    projects
        .create_project(
            &PathBuf::from(parent_dir),
            &name,
            &source_lang,
            &target_lang,
            &preset,
        )
        .await
}

#[tauri::command]
pub async fn open_project_manifest(
    projects: State<'_, Arc<ProjectManager>>,
    xaku_path: String,
) -> Result<ProjectSummary, String> {
    projects.open_project_manifest(&PathBuf::from(xaku_path)).await
}

#[tauri::command]
pub async fn get_project_summary(
    projects: State<'_, Arc<ProjectManager>>,
    project_id: String,
) -> Result<ProjectSummary, String> {
    projects.get_project_summary(&project_id).await
}

#[tauri::command]
pub async fn rename_project(
    projects: State<'_, Arc<ProjectManager>>,
    project_id: String,
    new_name: String,
) -> Result<ProjectRecord, String> {
    projects.rename_project(&project_id, &new_name).await
}

#[tauri::command]
pub async fn delete_project(
    projects: State<'_, Arc<ProjectManager>>,
    project_id: String,
    delete_files: bool,
) -> Result<(), String> {
    projects.delete_project(&project_id, delete_files).await
}

#[tauri::command]
pub async fn import_project_media(
    projects: State<'_, Arc<ProjectManager>>,
    project_id: String,
    source_file: String,
) -> Result<ProjectSummary, String> {
    projects
        .import_media(&project_id, &PathBuf::from(source_file))
        .await
}
