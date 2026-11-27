use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::database::{DownloadRecord, TaskRecord};
use crate::downloads::DownloadManager;
use crate::resources::credentials::CredentialStore;
use crate::resources::{
    list_provider_catalog, probe_system_resources, ProviderCatalogEntry, SystemResourceInfo,
};
use crate::scheduler::{NewTaskSpec, ResourceClass, TaskScheduler};

#[tauri::command]
pub async fn get_system_resources() -> Result<SystemResourceInfo, String> {
    Ok(probe_system_resources().await)
}

#[tauri::command]
pub async fn list_providers_status() -> Result<Vec<ProviderCatalogEntry>, String> {
    Ok(list_provider_catalog())
}

#[tauri::command]
pub async fn set_provider_credential(key_name: String, secret: String) -> Result<(), String> {
    CredentialStore::set_secret(&key_name, &secret)
}

#[tauri::command]
pub async fn delete_provider_credential(key_name: String) -> Result<(), String> {
    CredentialStore::delete_secret(&key_name)
}

#[tauri::command]
pub async fn list_scheduled_tasks(
    scheduler: State<'_, Arc<TaskScheduler>>,
    project_id: Option<String>,
) -> Result<Vec<TaskRecord>, String> {
    scheduler.list_tasks(project_id.as_deref()).await
}

#[tauri::command]
pub async fn enqueue_diagnostic_dag(
    scheduler: State<'_, Arc<TaskScheduler>>,
    project_id: Option<String>,
) -> Result<Vec<TaskRecord>, String> {
    let run_id = Uuid::new_v4().to_string();
    let root_id = Uuid::new_v4().to_string();
    let gpu_a_id = Uuid::new_v4().to_string();
    let gpu_b_id = Uuid::new_v4().to_string();
    let merge_id = Uuid::new_v4().to_string();

    let nodes = vec![
        NewTaskSpec {
            id: root_id.clone(),
            task_type: "system.self_test".to_string(),
            resource_class: ResourceClass::CpuLight,
            input_json: json!({ "steps": 3, "step_delay_ms": 120 }),
            depends_on: vec![],
            max_retries: 1,
        },
        NewTaskSpec {
            id: gpu_a_id.clone(),
            task_type: "system.self_test".to_string(),
            resource_class: ResourceClass::GpuHeavy,
            input_json: json!({ "steps": 4, "step_delay_ms": 140 }),
            depends_on: vec![root_id.clone()],
            max_retries: 1,
        },
        NewTaskSpec {
            id: gpu_b_id.clone(),
            task_type: "system.self_test".to_string(),
            resource_class: ResourceClass::GpuHeavy,
            input_json: json!({ "steps": 4, "step_delay_ms": 140 }),
            depends_on: vec![root_id],
            max_retries: 1,
        },
        NewTaskSpec {
            id: merge_id,
            task_type: "system.self_test".to_string(),
            resource_class: ResourceClass::CpuLight,
            input_json: json!({ "steps": 2, "step_delay_ms": 100 }),
            depends_on: vec![gpu_a_id, gpu_b_id],
            max_retries: 1,
        },
    ];

    scheduler.enqueue_dag(project_id, Some(run_id), nodes).await
}

#[tauri::command]
pub async fn cancel_scheduled_task(
    scheduler: State<'_, Arc<TaskScheduler>>,
    task_id: String,
) -> Result<bool, String> {
    scheduler.cancel_task(&task_id).await
}

#[tauri::command]
pub async fn retry_scheduled_task(
    scheduler: State<'_, Arc<TaskScheduler>>,
    task_id: String,
) -> Result<TaskRecord, String> {
    scheduler.retry_task(&task_id).await
}

#[tauri::command]
pub async fn list_downloads(
    downloads: State<'_, Arc<DownloadManager>>,
) -> Result<Vec<DownloadRecord>, String> {
    downloads.list_downloads().await
}

#[tauri::command]
pub async fn start_asset_download(
    app: AppHandle,
    downloads: State<'_, Arc<DownloadManager>>,
    resource_type: String,
    resource_id: String,
    url: String,
    file_name: String,
    sha256_expected: Option<String>,
) -> Result<DownloadRecord, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {e}"))?;
    let safe_name = PathBuf::from(&file_name)
        .file_name()
        .map(PathBuf::from)
        .ok_or_else(|| "Invalid download file name".to_string())?;
    let dest = app_data.join("downloads").join(safe_name);
    downloads
        .enqueue_download(&resource_type, &resource_id, &url, &dest, sha256_expected)
        .await
}

#[tauri::command]
pub async fn pause_asset_download(
    downloads: State<'_, Arc<DownloadManager>>,
    download_id: String,
) -> Result<bool, String> {
    downloads.pause_download(&download_id).await
}

#[tauri::command]
pub async fn resume_asset_download(
    downloads: State<'_, Arc<DownloadManager>>,
    download_id: String,
) -> Result<DownloadRecord, String> {
    downloads.resume_download(&download_id).await
}

#[tauri::command]
pub async fn cancel_asset_download(
    downloads: State<'_, Arc<DownloadManager>>,
    download_id: String,
) -> Result<bool, String> {
    downloads.cancel_download(&download_id).await
}
