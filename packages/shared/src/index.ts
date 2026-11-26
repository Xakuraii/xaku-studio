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

export interface MediaEngineStatus {
  ffmpeg_available: boolean;
  ffprobe_available: boolean;
  ffmpeg_version: string | null;
  has_nvenc: boolean;
  has_libx264: boolean;
  has_libass: boolean;
  has_rubberband: boolean;
  error: string | null;
}

export interface ProjectRecord {
  id: string;
  name: string;
  root_path: string;
  source_lang: string;
  target_lang: string;
  preset: string;
  settings_json: string;
  status: string;
  created_at: string;
  updated_at: string;
}

export interface MediaAssetRecord {
  id: string;
  project_id: string;
  role: string;
  file_path: string;
  sha256: string | null;
  duration_ms: number | null;
  width: number | null;
  height: number | null;
  fps_num: number | null;
  fps_den: number | null;
  audio_channels: number | null;
  sample_rate: number | null;
  metadata_json: string;
  created_at: string;
}

export interface VideoStreamInfo {
  index: number;
  codec: string;
  width: number;
  height: number;
  fps_num: number;
  fps_den: number;
}

export interface AudioStreamInfo {
  index: number;
  codec: string;
  channels: number;
  sample_rate: number;
  language: string | null;
}

export interface SubtitleStreamInfo {
  index: number;
  codec: string;
  language: string | null;
  title: string | null;
}

export interface MediaInspection {
  format_name: string;
  duration_ms: number;
  size_bytes: number;
  video: VideoStreamInfo | null;
  audio_streams: AudioStreamInfo[];
  subtitle_streams: SubtitleStreamInfo[];
}

export interface ProjectSummary {
  project: ProjectRecord;
  assets: MediaAssetRecord[];
  source_preview_url: string | null;
  inspection: MediaInspection | null;
  waveform_peaks: number[] | null;
}
