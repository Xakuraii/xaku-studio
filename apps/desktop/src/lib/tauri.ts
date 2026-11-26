import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  SelfTestResult,
  TaskProgressEvent,
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
