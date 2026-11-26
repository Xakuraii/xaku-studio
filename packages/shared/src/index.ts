export type WorkerConnectionState =
  | "stopped"
  | "starting"
  | "ready"
  | "error";

export interface ProviderMetadata {
  provider_id: string;
  name: string;
  version: string;
  capability: string;
  supported_languages: string[];
  supported_features: string[];
  required_hardware: string;
  is_local: boolean;
  requires_credential: boolean;
  license: string;
  available: boolean;
  unavailable_reason: string | null;
}

export interface WorkerCapabilities {
  python_version: string;
  python_executable: string;
  platform: string;
  machine: string;
  cpu_count: number;
  torch_installed: boolean;
  cuda_available: boolean;
  gpu_name: string | null;
  vram_mb: number | null;
  providers: ProviderMetadata[];
}

export interface WorkerStatus {
  state: WorkerConnectionState;
  protocol_version: string | null;
  python_version: string | null;
  pid: number | null;
  uptime_seconds: number | null;
  active_tasks: string[];
  loaded_models: string[];
  capabilities: WorkerCapabilities | null;
  last_error: string | null;
}

export interface TaskProgressEvent {
  task_id: string;
  percent: number;
  stage: string;
  message: string;
  eta_seconds?: number;
}

export interface SelfTestResult {
  task_id: string;
  status: string;
  steps_completed: number;
  python_version: string;
}
