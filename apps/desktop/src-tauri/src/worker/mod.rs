pub mod protocol;
pub mod win_job;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::{oneshot, Mutex, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

use protocol::{
    IncomingFrame, JsonRpcError, JsonRpcRequest, TaskProgressEvent, WorkerCapabilities,
    WorkerConnectionState, WorkerStatus, PROTOCOL_VERSION,
};
use win_job::WinJobObject;

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub python_executable: PathBuf,
    pub working_dir: PathBuf,
    pub request_timeout: Duration,
    pub env: Vec<(String, String)>,
}

impl WorkerConfig {
    /// Discover the pinned `.venv` Python 3.12.10 interpreter and repository root in dev mode.
    pub fn discover_default() -> Self {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(cwd) = std::env::current_dir() {
            candidates.push(cwd);
        }
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        candidates.push(manifest_dir);

        let repo_root = candidates
            .into_iter()
            .find_map(|start| find_repo_root(&start))
            .unwrap_or_else(|| PathBuf::from("."));

        let python_executable = if let Ok(explicit) = std::env::var("XAKUSTUDIO_PYTHON") {
            PathBuf::from(explicit)
        } else {
            let venv_win = repo_root.join(".venv").join("Scripts").join("python.exe");
            let venv_unix = repo_root.join(".venv").join("bin").join("python");
            if venv_win.exists() {
                venv_win
            } else if venv_unix.exists() {
                venv_unix
            } else {
                PathBuf::from("python")
            }
        };

        Self {
            python_executable,
            working_dir: repo_root,
            request_timeout: Duration::from_secs(30),
            env: Vec::new(),
        }
    }
}

fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join("pyproject.toml").exists() && dir.join("engine").join("python").exists() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

pub trait WorkerEventSink: Send + Sync + 'static {
    fn on_status_changed(&self, status: &WorkerStatus);
    fn on_task_progress(&self, progress: &TaskProgressEvent);
}

pub struct NoopEventSink;

impl WorkerEventSink for NoopEventSink {
    fn on_status_changed(&self, _status: &WorkerStatus) {}
    fn on_task_progress(&self, _progress: &TaskProgressEvent) {}
}

type PendingMap = HashMap<String, oneshot::Sender<Result<Value, JsonRpcError>>>;

struct ProcessHandle {
    child: Child,
    stdin: ChildStdin,
    _job: Option<WinJobObject>,
}

pub struct WorkerManager {
    config: WorkerConfig,
    event_sink: Arc<dyn WorkerEventSink>,
    process: Mutex<Option<ProcessHandle>>,
    pending: Arc<Mutex<PendingMap>>,
    status: Arc<RwLock<WorkerStatus>>,
}

impl WorkerManager {
    pub fn new(config: WorkerConfig, event_sink: Arc<dyn WorkerEventSink>) -> Arc<Self> {
        Arc::new(Self {
            config,
            event_sink,
            process: Mutex::new(None),
            pending: Arc::new(Mutex::new(HashMap::new())),
            status: Arc::new(RwLock::new(WorkerStatus {
                state: WorkerConnectionState::Stopped,
                protocol_version: None,
                python_version: None,
                pid: None,
                uptime_seconds: None,
                active_tasks: Vec::new(),
                loaded_models: Vec::new(),
                capabilities: None,
                last_error: None,
            })),
        })
    }

    pub async fn status(&self) -> WorkerStatus {
        self.status.read().await.clone()
    }

    async fn update_status(&self, mutate: impl FnOnce(&mut WorkerStatus)) -> WorkerStatus {
        let snapshot = {
            let mut guard = self.status.write().await;
            mutate(&mut guard);
            guard.clone()
        };
        self.event_sink.on_status_changed(&snapshot);
        snapshot
    }

    pub async fn start(self: &Arc<Self>) -> Result<WorkerStatus, String> {
        self.stop().await;
        self.update_status(|s| {
            s.state = WorkerConnectionState::Starting;
            s.last_error = None;
        })
        .await;

        let mut cmd = tokio::process::Command::new(&self.config.python_executable);
        cmd.arg("-m")
            .arg("xakustudio.worker")
            .current_dir(&self.config.working_dir)
            .env("PYTHONUNBUFFERED", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (k, v) in &self.config.env {
            cmd.env(k, v);
        }

        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn().map_err(|e| {
            let msg = format!(
                "Failed to spawn Python worker ({}): {e}",
                self.config.python_executable.display()
            );
            msg
        })?;

        let pid = child.id();
        let job = match WinJobObject::new() {
            Ok(job) => {
                if let Some(pid_val) = pid {
                    if let Err(err) = job.assign_pid(pid_val) {
                        warn!("Could not assign Python worker to Windows Job Object: {err}");
                    }
                }
                Some(job)
            }
            Err(err) => {
                warn!("Could not create Windows Job Object: {err}");
                None
            }
        };

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Worker stdin pipe missing".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Worker stdout pipe missing".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "Worker stderr pipe missing".to_string())?;

        {
            let mut proc_guard = self.process.lock().await;
            *proc_guard = Some(ProcessHandle {
                child,
                stdin,
                _job: job,
            });
        }

        self.spawn_stdout_reader(stdout);
        self.spawn_stderr_reader(stderr);

        let handshake_res = self
            .call(
                "system.handshake",
                json!({
                    "protocol_version": PROTOCOL_VERSION,
                    "app_version": env!("CARGO_PKG_VERSION"),
                }),
            )
            .await;

        match handshake_res {
            Ok(val) => {
                let protocol_version = val
                    .get("protocol_version")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                let python_version = val
                    .get("python_version")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                let capabilities: Option<WorkerCapabilities> = val
                    .get("capabilities")
                    .cloned()
                    .and_then(|c| serde_json::from_value(c).ok());

                info!(
                    pid = ?pid,
                    python_version = ?python_version,
                    protocol_version = ?protocol_version,
                    "Python worker handshake succeeded"
                );

                Ok(self
                    .update_status(|s| {
                        s.state = WorkerConnectionState::Ready;
                        s.protocol_version = protocol_version;
                        s.python_version = python_version;
                        s.pid = pid;
                        s.capabilities = capabilities;
                        s.last_error = None;
                    })
                    .await)
            }
            Err(err) => {
                self.update_status(|s| {
                    s.state = WorkerConnectionState::Error;
                    s.last_error = Some(err.clone());
                })
                .await;
                self.stop().await;
                Err(err)
            }
        }
    }

    pub async fn stop(&self) {
        let maybe_handle = {
            let mut guard = self.process.lock().await;
            guard.take()
        };

        if let Some(mut handle) = maybe_handle {
            let req = JsonRpcRequest::new(
                Uuid::new_v4().to_string(),
                "system.shutdown",
                json!({}),
            );
            if let Ok(line) = serde_json::to_string(&req) {
                let _ = handle.stdin.write_all(format!("{line}\n").as_bytes()).await;
                let _ = handle.stdin.flush().await;
            }
            drop(handle.stdin);

            match tokio::time::timeout(Duration::from_secs(2), handle.child.wait()).await {
                Ok(_) => {}
                Err(_) => {
                    let _ = handle.child.kill().await;
                }
            }
        }

        // Fail any inflight requests cleanly.
        let mut pending = self.pending.lock().await;
        for (_, tx) in pending.drain() {
            let _ = tx.send(Err(JsonRpcError {
                code: -32000,
                message: "Worker stopped".to_string(),
                data: None,
            }));
        }

        self.update_status(|s| {
            s.state = WorkerConnectionState::Stopped;
            s.pid = None;
            s.active_tasks.clear();
        })
        .await;
    }

    pub async fn health(&self) -> Result<WorkerStatus, String> {
        let res = self.call("system.health", json!({})).await?;
        let uptime_seconds = res.get("uptime_seconds").and_then(Value::as_f64);
        let active_tasks = res
            .get("active_tasks")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let loaded_models = res
            .get("loaded_models")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default();

        Ok(self
            .update_status(|s| {
                s.state = WorkerConnectionState::Ready;
                s.uptime_seconds = uptime_seconds;
                s.active_tasks = active_tasks;
                s.loaded_models = loaded_models;
                s.last_error = None;
            })
            .await)
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.call_rpc(method, params)
            .await
            .map_err(|e| format!("[{}] {}", e.code, e.message))
    }

    pub async fn call_rpc(&self, method: &str, params: Value) -> Result<Value, JsonRpcError> {
        self.call_rpc_with_timeout(method, params, Some(self.config.request_timeout))
            .await
    }

    pub async fn call_rpc_with_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: Option<Duration>,
    ) -> Result<Value, JsonRpcError> {
        let req_id = Uuid::new_v4().to_string();
        let req = JsonRpcRequest::new(req_id.clone(), method, params);
        let line = serde_json::to_string(&req).map_err(|e| JsonRpcError {
            code: -32603,
            message: format!("Serialize error: {e}"),
            data: None,
        })?;

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(req_id.clone(), tx);
        }

        let write_result = {
            let mut proc_guard = self.process.lock().await;
            match proc_guard.as_mut() {
                Some(handle) => {
                    let res = handle.stdin.write_all(format!("{line}\n").as_bytes()).await;
                    if res.is_ok() {
                        handle.stdin.flush().await
                    } else {
                        res
                    }
                }
                None => Err(std::io::Error::new(
                    std::io::ErrorKind::NotConnected,
                    "Python worker is not running",
                )),
            }
        };

        if let Err(err) = write_result {
            let mut pending = self.pending.lock().await;
            pending.remove(&req_id);
            return Err(JsonRpcError {
                code: -32000,
                message: format!("Worker write failed: {err}"),
                data: None,
            });
        }

        match timeout {
            Some(dur) => match tokio::time::timeout(dur, rx).await {
                Ok(Ok(res)) => res,
                Ok(Err(_)) => Err(JsonRpcError {
                    code: -32000,
                    message: "Worker response channel closed".to_string(),
                    data: None,
                }),
                Err(_) => {
                    let mut pending = self.pending.lock().await;
                    pending.remove(&req_id);
                    Err(JsonRpcError {
                        code: -32000,
                        message: format!("Worker request '{method}' timed out"),
                        data: None,
                    })
                }
            },
            None => match rx.await {
                Ok(res) => res,
                Err(_) => Err(JsonRpcError {
                    code: -32000,
                    message: "Worker response channel closed".to_string(),
                    data: None,
                }),
            },
        }
    }

    fn spawn_stdout_reader(self: &Arc<Self>, stdout: tokio::process::ChildStdout) {
        let pending = Arc::clone(&self.pending);
        let event_sink = Arc::clone(&self.event_sink);
        let status = Arc::clone(&self.status);

        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<IncomingFrame>(&line) {
                    Ok(frame) => {
                        if let Some(req_id) = frame.id {
                            let tx = {
                                let mut map = pending.lock().await;
                                map.remove(&req_id)
                            };
                            if let Some(tx) = tx {
                                if let Some(err) = frame.error {
                                    let _ = tx.send(Err(err));
                                } else {
                                    let _ = tx.send(Ok(frame.result.unwrap_or(Value::Null)));
                                }
                            }
                        } else if frame.method.as_deref() == Some("task.progress") {
                            if let Some(params) = frame.params {
                                if let Ok(progress) =
                                    serde_json::from_value::<TaskProgressEvent>(params)
                                {
                                    event_sink.on_task_progress(&progress);
                                }
                            }
                        }
                    }
                    Err(err) => {
                        warn!("Invalid JSON frame from Python worker: {err}; raw={line}");
                    }
                }
            }

            // Stream ended: reject pending callers.
            let mut map = pending.lock().await;
            for (_, tx) in map.drain() {
                let _ = tx.send(Err(JsonRpcError {
                    code: -32000,
                    message: "Python worker stdout closed".to_string(),
                    data: None,
                }));
            }
            drop(map);

            let snapshot = {
                let mut guard = status.write().await;
                if matches!(guard.state, WorkerConnectionState::Ready | WorkerConnectionState::Starting) {
                    guard.state = WorkerConnectionState::Stopped;
                    guard.pid = None;
                }
                guard.clone()
            };
            event_sink.on_status_changed(&snapshot);
        });
    }

    fn spawn_stderr_reader(&self, stderr: tokio::process::ChildStderr) {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(parsed) = serde_json::from_str::<Value>(&line) {
                    let level = parsed.get("level").and_then(Value::as_str).unwrap_or("INFO");
                    let msg = parsed
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or(&line);
                    match level {
                        "ERROR" => error!(target: "python_worker", "{msg} | {parsed}"),
                        "WARN" => warn!(target: "python_worker", "{msg} | {parsed}"),
                        _ => info!(target: "python_worker", "{msg} | {parsed}"),
                    }
                } else {
                    info!(target: "python_worker", "{line}");
                }
            }
        });
    }
}
