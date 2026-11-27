import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  DownloadRecord,
  MediaEngineStatus,
  ProjectRecord,
  ProjectSummary,
  ProviderCatalogEntry,
  SelfTestResult,
  SystemResourceInfo,
  TaskProgressEvent,
  TaskRecord,
  WorkerStatus,
} from "@xakustudio/shared";

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function getWorkerStatus(): Promise<WorkerStatus> {
  return invoke<WorkerStatus>("get_worker_status");
}

export async function pingWorker(): Promise<WorkerStatus> {
  return invoke<WorkerStatus>("ping_worker");
}

export async function restartWorker(): Promise<WorkerStatus> {
  return invoke<WorkerStatus>("restart_worker");
}

export async function runWorkerSelfTest(
  steps = 6,
  stepDelayMs = 150
): Promise<SelfTestResult> {
  return invoke<SelfTestResult>("run_worker_self_test", {
    steps,
    stepDelayMs,
  });
}

export async function cancelWorkerTask(taskId: string): Promise<boolean> {
  return invoke<boolean>("cancel_worker_task", { taskId });
}

export async function getDefaultProjectsDir(): Promise<string> {
  return invoke<string>("get_default_projects_dir");
}

export async function getMediaEngineStatus(): Promise<MediaEngineStatus> {
  return invoke<MediaEngineStatus>("get_media_engine_status");
}

export async function listProjects(): Promise<ProjectRecord[]> {
  return invoke<ProjectRecord[]>("list_projects");
}

export async function createProject(input: {
  parentDir: string;
  name: string;
  sourceLang: string;
  targetLang: string;
  preset: string;
}): Promise<ProjectSummary> {
  return invoke<ProjectSummary>("create_project", input);
}

export async function openProjectManifest(
  xakuPath: string
): Promise<ProjectSummary> {
  return invoke<ProjectSummary>("open_project_manifest", { xakuPath });
}

export async function getProjectSummary(
  projectId: string
): Promise<ProjectSummary> {
  return invoke<ProjectSummary>("get_project_summary", { projectId });
}

export async function renameProject(
  projectId: string,
  newName: string
): Promise<ProjectRecord> {
  return invoke<ProjectRecord>("rename_project", { projectId, newName });
}

export async function deleteProject(
  projectId: string,
  deleteFiles: boolean
): Promise<void> {
  return invoke<void>("delete_project", { projectId, deleteFiles });
}

export async function importProjectMedia(
  projectId: string,
  sourceFile: string
): Promise<ProjectSummary> {
  return invoke<ProjectSummary>("import_project_media", {
    projectId,
    sourceFile,
  });
}

export async function getSystemResources(): Promise<SystemResourceInfo> {
  return invoke<SystemResourceInfo>("get_system_resources");
}

export async function listProvidersStatus(): Promise<ProviderCatalogEntry[]> {
  return invoke<ProviderCatalogEntry[]>("list_providers_status");
}

export async function setProviderCredential(
  keyName: string,
  secret: string
): Promise<void> {
  return invoke<void>("set_provider_credential", { keyName, secret });
}

export async function deleteProviderCredential(keyName: string): Promise<void> {
  return invoke<void>("delete_provider_credential", { keyName });
}

export async function listScheduledTasks(
  projectId?: string
): Promise<TaskRecord[]> {
  return invoke<TaskRecord[]>("list_scheduled_tasks", {
    projectId: projectId ?? null,
  });
}

export async function enqueueDiagnosticDag(
  projectId?: string
): Promise<TaskRecord[]> {
  return invoke<TaskRecord[]>("enqueue_diagnostic_dag", {
    projectId: projectId ?? null,
  });
}

export async function cancelScheduledTask(taskId: string): Promise<boolean> {
  return invoke<boolean>("cancel_scheduled_task", { taskId });
}

export async function retryScheduledTask(taskId: string): Promise<TaskRecord> {
  return invoke<TaskRecord>("retry_scheduled_task", { taskId });
}

export async function listDownloads(): Promise<DownloadRecord[]> {
  return invoke<DownloadRecord[]>("list_downloads");
}

export async function startAssetDownload(input: {
  resourceType: string;
  resourceId: string;
  url: string;
  fileName: string;
  sha256Expected?: string | null;
}): Promise<DownloadRecord> {
  return invoke<DownloadRecord>("start_asset_download", {
    resourceType: input.resourceType,
    resourceId: input.resourceId,
    url: input.url,
    fileName: input.fileName,
    sha256Expected: input.sha256Expected ?? null,
  });
}

export async function pauseAssetDownload(downloadId: string): Promise<boolean> {
  return invoke<boolean>("pause_asset_download", { downloadId });
}

export async function resumeAssetDownload(
  downloadId: string
): Promise<DownloadRecord> {
  return invoke<DownloadRecord>("resume_asset_download", { downloadId });
}

export async function cancelAssetDownload(downloadId: string): Promise<boolean> {
  return invoke<boolean>("cancel_asset_download", { downloadId });
}

export async function subscribeSchedulerEvents(
  onTaskUpdated: (task: TaskRecord) => void,
  onDownloadUpdated: (download: DownloadRecord) => void
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) {
    return () => {};
  }
  const u1 = await listen<TaskRecord>("scheduler:task-updated", (e) => {
    onTaskUpdated(e.payload);
  });
  const u2 = await listen<DownloadRecord>("download:updated", (e) => {
    onDownloadUpdated(e.payload);
  });
  return () => {
    u1();
    u2();
  };
}

export async function subscribeWorkerEvents(
  onStatus: (status: WorkerStatus) => void,
  onProgress: (progress: TaskProgressEvent) => void
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) {
    return () => {};
  }
  const unlistenStatus = await listen<WorkerStatus>("worker:status", (e) => {
    onStatus(e.payload);
  });
  const unlistenProgress = await listen<TaskProgressEvent>(
    "worker:progress",
    (e) => {
      onProgress(e.payload);
    }
  );
  return () => {
    unlistenStatus();
    unlistenProgress();
  };
}
