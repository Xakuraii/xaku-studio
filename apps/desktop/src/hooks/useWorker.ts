import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { WorkerStatus } from "@xakustudio/shared";
import {
  getWorkerStatus,
  isTauriRuntime,
  pingWorker,
  restartWorker,
  runWorkerSelfTest,
  subscribeWorkerEvents,
} from "@/lib/tauri";
import { useAppStore } from "@/stores/appStore";

const WORKER_STATUS_KEY = ["worker", "status"] as const;

export function useWorker() {
  const queryClient = useQueryClient();
  const setLatestProgress = useAppStore((s) => s.setLatestProgress);
  const inTauri = isTauriRuntime();

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    subscribeWorkerEvents(
      (status) => {
        queryClient.setQueryData<WorkerStatus>(WORKER_STATUS_KEY, status);
      },
      (progress) => {
        setLatestProgress(progress);
      }
    ).then((unlisten) => {
      cleanup = unlisten;
    });

    return () => {
      cleanup?.();
    };
  }, [queryClient, setLatestProgress]);

  const statusQuery = useQuery({
    queryKey: WORKER_STATUS_KEY,
    queryFn: getWorkerStatus,
    enabled: inTauri,
    refetchInterval: 10_000,
  });

  const pingMutation = useMutation({
    mutationFn: pingWorker,
    onSuccess: (status) => {
      queryClient.setQueryData(WORKER_STATUS_KEY, status);
    },
  });

  const restartMutation = useMutation({
    mutationFn: restartWorker,
    onSuccess: (status) => {
      queryClient.setQueryData(WORKER_STATUS_KEY, status);
    },
  });

  const selfTestMutation = useMutation({
    mutationFn: () => runWorkerSelfTest(6, 150),
    onMutate: () => {
      setLatestProgress(null);
    },
  });

  return {
    inTauri,
    statusQuery,
    pingMutation,
    restartMutation,
    selfTestMutation,
  };
}
