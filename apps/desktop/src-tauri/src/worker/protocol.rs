use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: &str = "1.0";

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: String,
    pub method: String,
    pub params: Value,
}

impl JsonRpcRequest {
    pub fn new(id: impl Into<String>, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id: id.into(),
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IncomingFrame {
    #[allow(dead_code)]
    pub jsonrpc: Option<String>,
    pub id: Option<String>,
    pub method: Option<String>,
    pub params: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkerConnectionState {
    Stopped,
    Starting,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetadata {
    pub provider_id: String,
    pub name: String,
    pub version: String,
    pub capability: String,
    pub supported_languages: Vec<String>,
    pub supported_features: Vec<String>,
    pub required_hardware: String,
    pub is_local: bool,
    pub requires_credential: bool,
    pub license: String,
    pub available: bool,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerCapabilities {
    pub python_version: String,
    pub python_executable: String,
    pub platform: String,
    pub machine: String,
    pub cpu_count: u32,
    pub torch_installed: bool,
    pub cuda_available: bool,
    pub gpu_name: Option<String>,
    pub vram_mb: Option<u64>,
    pub providers: Vec<ProviderMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerStatus {
    pub state: WorkerConnectionState,
    pub protocol_version: Option<String>,
    pub python_version: Option<String>,
    pub pid: Option<u32>,
    pub uptime_seconds: Option<f64>,
    pub active_tasks: Vec<String>,
    pub loaded_models: Vec<String>,
    pub capabilities: Option<WorkerCapabilities>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgressEvent {
    pub task_id: String,
    pub percent: f64,
    pub stage: String,
    pub message: String,
    pub eta_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfTestResult {
    pub task_id: String,
    pub status: String,
    pub steps_completed: u32,
    pub python_version: String,
}
