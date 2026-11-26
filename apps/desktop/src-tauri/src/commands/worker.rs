use std::sync::Arc;

use serde_json::json;
use tauri::State;
use uuid::Uuid;

use crate::worker::protocol::{SelfTestResult, WorkerStatus};
use crate::worker::WorkerManager;

#[tauri::command]
pub async fn get_worker_status(worker: State<'_, Arc<WorkerManager>>) -> Result<WorkerStatus, String> {
    Ok(worker.status().await)
}

#[tauri::command]
pub async fn ping_worker(worker: State<'_, Arc<WorkerManager>>) -> Result<WorkerStatus, String> {
    worker.health().await
}

#[tauri::command]
pub async fn restart_worker(worker: State<'_, Arc<WorkerManager>>) -> Result<WorkerStatus, String> {
    worker.start().await
}

#[tauri::command]
pub async fn run_worker_self_test(
    worker: State<'_, Arc<WorkerManager>>,
    steps: Option<u32>,
    step_delay_ms: Option<u64>,
) -> Result<SelfTestResult, String> {
    let task_id = Uuid::new_v4().to_string();
    let res = worker
        .call(
            "task.run",
            json!({
                "task_id": task_id,
                "task_type": "system.self_test",
                "payload": {
                    "steps": steps.unwrap_or(5),
                    "step_delay_ms": step_delay_ms.unwrap_or(180),
                }
            }),
        )
        .await?;

    let returned_id = res
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or(&task_id)
        .to_string();
    let output = res.get("output").cloned().unwrap_or_default();

    Ok(SelfTestResult {
        task_id: returned_id,
        status: output
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        steps_completed: output
            .get("steps_completed")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        python_version: output
            .get("python_version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
    })
}

#[tauri::command]
pub async fn cancel_worker_task(
    worker: State<'_, Arc<WorkerManager>>,
    task_id: String,
) -> Result<bool, String> {
    let res = worker
        .call("task.cancel", json!({ "task_id": task_id }))
        .await?;
    Ok(res
        .get("cancelled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false))
}
