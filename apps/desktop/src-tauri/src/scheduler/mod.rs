use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tokio::sync::{Mutex, Notify, RwLock, Semaphore};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::database::{AppDatabase, TaskDependencyRecord, TaskRecord};
use crate::resources::credentials::CredentialStore;
use crate::worker::WorkerManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceClass {
    GpuHeavy,
    CpuLight,
    MediaRender,
    NetworkIo,
}

impl ResourceClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GpuHeavy => "gpu_heavy",
            Self::CpuLight => "cpu_light",
            Self::MediaRender => "media_render",
            Self::NetworkIo => "network_io",
        }
    }

    pub fn from_str(raw: &str) -> Self {
        match raw {
            "gpu_heavy" => Self::GpuHeavy,
            "media_render" => Self::MediaRender,
            "network_io" => Self::NetworkIo,
            _ => Self::CpuLight,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewTaskSpec {
    pub id: String,
    pub task_type: String,
    pub resource_class: ResourceClass,
    pub input_json: Value,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default = "default_max_retries")]
    pub max_retries: i64,
}

fn default_max_retries() -> i64 {
    2
}

pub type TaskEventCallback = Arc<dyn Fn(&TaskRecord) + Send + Sync>;

pub type NativeTaskHandler = Arc<
    dyn Fn(TaskRecord) -> Pin<Box<dyn Future<Output = Result<Value, (String, String)>> + Send>>
        + Send
        + Sync,
>;

pub struct TaskScheduler {
    db: AppDatabase,
    worker: Arc<WorkerManager>,
    gpu_heavy_sem: Arc<Semaphore>,
    cpu_light_sem: Arc<Semaphore>,
    media_render_sem: Arc<Semaphore>,
    network_io_sem: Arc<Semaphore>,
    active_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
    native_handlers: Arc<RwLock<HashMap<String, NativeTaskHandler>>>,
    notify: Arc<Notify>,
    on_task_update: TaskEventCallback,
}

impl TaskScheduler {
    pub async fn start(
        db: AppDatabase,
        worker: Arc<WorkerManager>,
        gpu_heavy_slots: usize,
        cpu_light_slots: usize,
        network_io_slots: usize,
        on_task_update: TaskEventCallback,
    ) -> Result<Arc<Self>, String> {
        let recovered = db.mark_running_tasks_interrupted().await?;
        if recovered > 0 {
            info!(
                interrupted_count = recovered,
                "Recovered previously running tasks as interrupted"
            );
        }

        let scheduler = Arc::new(Self {
            db,
            worker,
            gpu_heavy_sem: Arc::new(Semaphore::new(gpu_heavy_slots.max(1))),
            cpu_light_sem: Arc::new(Semaphore::new(cpu_light_slots.max(1))),
            media_render_sem: Arc::new(Semaphore::new(1)),
            network_io_sem: Arc::new(Semaphore::new(network_io_slots.max(1))),
            active_tokens: Arc::new(Mutex::new(HashMap::new())),
            native_handlers: Arc::new(RwLock::new(HashMap::new())),
            notify: Arc::new(Notify::new()),
            on_task_update,
        });

        scheduler.spawn_loop();
        scheduler.notify.notify_one();
        Ok(scheduler)
    }

    pub async fn register_native_handler(
        &self,
        task_type: impl Into<String>,
        handler: NativeTaskHandler,
    ) {
        self.native_handlers
            .write()
            .await
            .insert(task_type.into(), handler);
    }

    fn spawn_loop(self: &Arc<Self>) {
        let sched = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                if let Err(err) = sched.dispatch_ready_tasks().await {
                    warn!("Task scheduler dispatch error: {err}");
                }
                sched.notify.notified().await;
            }
        });
    }

    pub async fn list_tasks(&self, project_id: Option<&str>) -> Result<Vec<TaskRecord>, String> {
        self.db.list_tasks(project_id).await
    }

    pub async fn enqueue_dag(
        &self,
        project_id: Option<String>,
        pipeline_run_id: Option<String>,
        nodes: Vec<NewTaskSpec>,
    ) -> Result<Vec<TaskRecord>, String> {
        let run_id = pipeline_run_id.or_else(|| Some(Uuid::new_v4().to_string()));
        let mut records = Vec::with_capacity(nodes.len());
        let mut deps = Vec::new();

        for node in nodes {
            records.push(TaskRecord {
                id: node.id.clone(),
                project_id: project_id.clone(),
                pipeline_run_id: run_id.clone(),
                task_type: node.task_type,
                resource_class: node.resource_class.as_str().to_string(),
                status: "queued".to_string(),
                progress: 0.0,
                stage_message: "Queued".to_string(),
                input_json: node.input_json.to_string(),
                output_json: "{}".to_string(),
                error_code: None,
                error_message: None,
                retry_count: 0,
                max_retries: node.max_retries,
                started_at: None,
                completed_at: None,
                created_at: String::new(),
            });
            for parent_id in node.depends_on {
                deps.push(TaskDependencyRecord {
                    task_id: node.id.clone(),
                    depends_on_task_id: parent_id,
                });
            }
        }

        self.db.insert_task_dag(&records, &deps).await?;
        for r in &records {
            (self.on_task_update)(r);
        }
        self.notify.notify_one();
        Ok(records)
    }

    pub async fn cancel_task(&self, task_id: &str) -> Result<bool, String> {
        let token = {
            let guard = self.active_tokens.lock().await;
            guard.get(task_id).cloned()
        };
        if let Some(t) = token {
            t.cancel();
            return Ok(true);
        }

        if let Some(task) = self.db.get_task(task_id).await? {
            if task.status == "queued" {
                self.db
                    .update_task_status(
                        task_id,
                        "cancelled",
                        task.progress,
                        "Cancelled before execution",
                        None,
                        Some("Cancelled"),
                        Some("Task cancelled by user"),
                    )
                    .await?;
                if let Some(updated) = self.db.get_task(task_id).await? {
                    (self.on_task_update)(&updated);
                }
                self.notify.notify_one();
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub async fn retry_task(&self, task_id: &str) -> Result<TaskRecord, String> {
        self.db.reset_task_for_retry(task_id).await?;
        let updated = self
            .db
            .get_task(task_id)
            .await?
            .ok_or_else(|| format!("Task '{task_id}' not found"))?;
        (self.on_task_update)(&updated);
        self.notify.notify_one();
        Ok(updated)
    }

    async fn dispatch_ready_tasks(self: &Arc<Self>) -> Result<(), String> {
        let queued = self.db.list_queued_tasks().await?;
        for task in queued {
            {
                let guard = self.active_tokens.lock().await;
                if guard.contains_key(&task.id) {
                    continue;
                }
            }

            let dep_ids = self.db.list_task_dependencies(&task.id).await?;
            let mut failed_dep: Option<String> = None;
            let mut all_completed = true;

            for dep_id in dep_ids {
                match self.db.get_task(&dep_id).await? {
                    Some(dep) if dep.status == "completed" => {}
                    Some(dep) if matches!(dep.status.as_str(), "failed" | "cancelled" | "interrupted") => {
                        failed_dep = Some(dep_id);
                        all_completed = false;
                        break;
                    }
                    Some(_) => {
                        all_completed = false;
                    }
                    None => {
                        failed_dep = Some(dep_id);
                        all_completed = false;
                        break;
                    }
                }
            }

            if let Some(dep_id) = failed_dep {
                self.db
                    .update_task_status(
                        &task.id,
                        "failed",
                        0.0,
                        "Blocked by failed dependency",
                        None,
                        Some("DependencyFailed"),
                        Some(&format!("Upstream task '{dep_id}' did not complete")),
                    )
                    .await?;
                if let Some(updated) = self.db.get_task(&task.id).await? {
                    (self.on_task_update)(&updated);
                }
                continue;
            }

            if all_completed {
                let token = CancellationToken::new();
                {
                    let mut guard = self.active_tokens.lock().await;
                    if guard.contains_key(&task.id) {
                        continue;
                    }
                    guard.insert(task.id.clone(), token.clone());
                }
                let sched = Arc::clone(self);
                tokio::spawn(async move {
                    sched.run_task(task, token).await;
                });
            }
        }
        Ok(())
    }

    fn semaphore_for(&self, class: ResourceClass) -> &Arc<Semaphore> {
        match class {
            ResourceClass::GpuHeavy => &self.gpu_heavy_sem,
            ResourceClass::CpuLight => &self.cpu_light_sem,
            ResourceClass::MediaRender => &self.media_render_sem,
            ResourceClass::NetworkIo => &self.network_io_sem,
        }
    }

    async fn run_task(&self, task: TaskRecord, token: CancellationToken) {
        let class = ResourceClass::from_str(&task.resource_class);
        let sem = Arc::clone(self.semaphore_for(class));

        let permit = tokio::select! {
            p = sem.acquire_owned() => match p {
                Ok(permit) => permit,
                Err(_) => {
                    self.active_tokens.lock().await.remove(&task.id);
                    return;
                }
            },
            _ = token.cancelled() => {
                self.finish_cancelled(&task.id, task.progress).await;
                return;
            }
        };

        let _ = self
            .db
            .update_task_status(&task.id, "running", 5.0, "Running", None, None, None)
            .await;
        if let Some(updated) = self.db.get_task(&task.id).await.ok().flatten() {
            (self.on_task_update)(&updated);
        }

        let input: Value = serde_json::from_str(&task.input_json).unwrap_or_else(|_| json!({}));
        let mode = input.get("mode").and_then(Value::as_str).unwrap_or("worker");
        let native_handler = self.native_handlers.read().await.get(&task.task_type).cloned();

        let outcome: Result<Value, (String, String)> = if let Some(handler) = native_handler {
            let fut = handler(task.clone());
            tokio::select! {
                res = fut => res,
                _ = token.cancelled() => Err(("Cancelled".to_string(), "Task cancelled by user".to_string())),
            }
        } else if mode == "local_sleep" {
            let duration_ms = input.get("duration_ms").and_then(Value::as_u64).unwrap_or(100);
            let fail = input.get("fail").and_then(Value::as_bool).unwrap_or(false);
            let mut elapsed = 0u64;
            let mut was_cancelled = false;
            while elapsed < duration_ms {
                let slice = (duration_ms - elapsed).min(20);
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(slice)) => {
                        elapsed += slice;
                    }
                    _ = token.cancelled() => {
                        was_cancelled = true;
                        break;
                    }
                }
            }
            if was_cancelled {
                Err(("Cancelled".to_string(), "Task cancelled by user".to_string()))
            } else if fail {
                Err(("SimulatedFailure".to_string(), "Simulated failure requested".to_string()))
            } else {
                Ok(json!({ "status": "passed", "elapsed_ms": elapsed }))
            }
        } else {
            let mut payload = input.clone();
            if let Some(obj) = payload.as_object_mut() {
                if let Some(keys_val) = obj.remove("credential_keys") {
                    if let Some(keys) = keys_val.as_array() {
                        let mut creds = Map::new();
                        for k in keys {
                            if let Some(key_name) = k.as_str() {
                                if let Ok(Some(secret)) = CredentialStore::get_secret(key_name) {
                                    if !secret.is_empty() {
                                        creds.insert(key_name.to_string(), Value::String(secret));
                                    }
                                }
                            }
                        }
                        if !creds.is_empty() {
                            obj.insert("credentials".to_string(), Value::Object(creds));
                        }
                    }
                }
            }

            let worker = Arc::clone(&self.worker);
            let task_id = task.id.clone();
            let task_type = task.task_type.clone();
            let worker_fut = worker.call_rpc_with_timeout(
                "task.run",
                json!({
                    "task_id": task_id,
                    "task_type": task_type,
                    "payload": payload,
                }),
                None,
            );
            tokio::pin!(worker_fut);

            tokio::select! {
                res = &mut worker_fut => match res {
                    Ok(v) => Ok(v),
                    Err(err) => {
                        let code = err
                            .data
                            .as_ref()
                            .and_then(|d| d.get("error_type"))
                            .and_then(Value::as_str)
                            .unwrap_or("WorkerError")
                            .to_string();
                        Err((code, err.message))
                    }
                },
                _ = token.cancelled() => {
                    let _ = self.worker.call("task.cancel", json!({ "task_id": task.id })).await;
                    Err(("Cancelled".to_string(), "Task cancelled by user".to_string()))
                }
            }
        };

        drop(permit);
        self.active_tokens.lock().await.remove(&task.id);

        match outcome {
            Ok(out_val) => {
                let out_str = out_val.to_string();
                let _ = self
                    .db
                    .update_task_status(
                        &task.id,
                        "completed",
                        100.0,
                        "Completed",
                        Some(&out_str),
                        None,
                        None,
                    )
                    .await;
            }
            Err((code, msg)) if code == "Cancelled" => {
                let _ = self
                    .db
                    .update_task_status(
                        &task.id,
                        "cancelled",
                        task.progress,
                        "Cancelled",
                        None,
                        Some(&code),
                        Some(&msg),
                    )
                    .await;
            }
            Err((code, msg)) => {
                let _ = self
                    .db
                    .update_task_status(
                        &task.id,
                        "failed",
                        task.progress,
                        "Failed",
                        None,
                        Some(&code),
                        Some(&msg),
                    )
                    .await;
            }
        }

        if let Some(updated) = self.db.get_task(&task.id).await.ok().flatten() {
            (self.on_task_update)(&updated);
        }
        self.notify.notify_one();
    }

    async fn finish_cancelled(&self, task_id: &str, progress: f64) {
        self.active_tokens.lock().await.remove(task_id);
        let _ = self
            .db
            .update_task_status(
                task_id,
                "cancelled",
                progress,
                "Cancelled",
                None,
                Some("Cancelled"),
                Some("Task cancelled before acquiring resource slot"),
            )
            .await;
        if let Some(updated) = self.db.get_task(task_id).await.ok().flatten() {
            (self.on_task_update)(&updated);
        }
        self.notify.notify_one();
    }
}
