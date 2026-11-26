import { Cpu, Play, RefreshCw, Server, Terminal, Wifi } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useWorker } from "@/hooks/useWorker";
import { useAppStore } from "@/stores/appStore";

function stateTone(state?: string) {
  switch (state) {
    case "ready":
      return "success" as const;
    case "starting":
      return "warning" as const;
    case "error":
      return "danger" as const;
    default:
      return "neutral" as const;
  }
}

export function HomeDashboard() {
  const {
    inTauri,
    statusQuery,
    pingMutation,
    restartMutation,
    selfTestMutation,
  } = useWorker();
  const latestProgress = useAppStore((s) => s.latestProgress);
  const status = statusQuery.data;
  const caps = status?.capabilities;

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold tracking-tight">
            Workstation Foundation
          </h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Chinese → Khmer AI video localization engine. Stage 1 verifies the
            persistent Rust–Python 3.12.10 JSON-RPC worker and hardware probe.
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <Button
            variant="secondary"
            size="sm"
            disabled={!inTauri || pingMutation.isPending}
            onClick={() => pingMutation.mutate()}
          >
            <Wifi className="h-3.5 w-3.5" />
            Ping Worker
          </Button>
          <Button
            variant="secondary"
            size="sm"
            disabled={!inTauri || restartMutation.isPending}
            onClick={() => restartMutation.mutate()}
          >
            <RefreshCw className="h-3.5 w-3.5" />
            Restart Worker
          </Button>
          <Button
            size="sm"
            disabled={!inTauri || selfTestMutation.isPending}
            onClick={() => selfTestMutation.mutate()}
          >
            <Play className="h-3.5 w-3.5" />
            {selfTestMutation.isPending ? "Running Self-Test..." : "Run Worker Self-Test"}
          </Button>
        </div>
      </div>

      {!inTauri && (
        <Card className="border-amber-500/40 bg-amber-500/5">
          <CardContent className="pt-4 text-xs text-amber-300">
            Running outside the Tauri desktop shell (browser preview mode). Launch via{" "}
            <code className="rounded bg-surface-elevated px-1.5 py-0.5 font-mono">
              npm run dev
            </code>{" "}
            to connect to the Rust core and Python 3.12.10 worker.
          </CardContent>
        </Card>
      )}

      <div className="grid gap-4 md:grid-cols-3">
        <Card>
          <CardHeader className="flex-row items-center justify-between">
            <CardTitle className="flex items-center gap-2">
              <Server className="h-4 w-4 text-primary" />
              Python Worker
            </CardTitle>
            <Badge tone={stateTone(status?.state)}>
              {status?.state ?? (inTauri ? "connecting" : "unavailable")}
            </Badge>
          </CardHeader>
          <CardContent className="space-y-1.5 text-xs">
            <div className="flex justify-between">
              <span className="text-muted-foreground">Python Runtime</span>
              <span className="font-mono">{status?.python_version ?? "—"}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Protocol Version</span>
              <span className="font-mono">{status?.protocol_version ?? "—"}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Process PID</span>
              <span className="font-mono">{status?.pid ?? "—"}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Uptime</span>
              <span className="font-mono">
                {status?.uptime_seconds != null
                  ? `${status.uptime_seconds.toFixed(1)}s`
                  : "—"}
              </span>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex-row items-center justify-between">
            <CardTitle className="flex items-center gap-2">
              <Cpu className="h-4 w-4 text-primary" />
              Hardware Profile
            </CardTitle>
            <Badge tone={caps?.gpu_name ? "primary" : "neutral"}>
              {caps?.cuda_available ? "CUDA Ready" : caps?.gpu_name ? "GPU Detected" : "CPU Mode"}
            </Badge>
          </CardHeader>
          <CardContent className="space-y-1.5 text-xs">
            <div className="flex justify-between gap-2">
              <span className="text-muted-foreground">GPU</span>
              <span className="truncate font-mono">{caps?.gpu_name ?? "—"}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">VRAM</span>
              <span className="font-mono">
                {caps?.vram_mb != null ? `${caps.vram_mb} MiB` : "—"}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">CPU Threads</span>
              <span className="font-mono">{caps?.cpu_count ?? "—"}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">PyTorch Loaded</span>
              <span className="font-mono">
                {caps ? (caps.torch_installed ? "yes" : "not yet (Stage 3+)") : "—"}
              </span>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex-row items-center justify-between">
            <CardTitle className="flex items-center gap-2">
              <Terminal className="h-4 w-4 text-primary" />
              Worker Self-Test
            </CardTitle>
            <Badge
              tone={
                selfTestMutation.isSuccess
                  ? "success"
                  : selfTestMutation.isError
                    ? "danger"
                    : "neutral"
              }
            >
              {selfTestMutation.isPending
                ? "Running"
                : selfTestMutation.isSuccess
                  ? "Passed"
                  : selfTestMutation.isError
                    ? "Failed"
                    : "Idle"}
            </Badge>
          </CardHeader>
          <CardContent className="space-y-3 text-xs">
            <div className="space-y-1.5">
              <div className="flex justify-between text-muted-foreground">
                <span>{latestProgress?.message ?? "Ready to test JSON-RPC streaming"}</span>
                <span className="font-mono">
                  {latestProgress ? `${latestProgress.percent.toFixed(0)}%` : "0%"}
                </span>
              </div>
              <div className="h-2 w-full overflow-hidden rounded-full bg-surface-elevated">
                <div
                  className="h-full bg-primary transition-all duration-150"
                  style={{ width: `${latestProgress?.percent ?? 0}%` }}
                />
              </div>
            </div>

            {selfTestMutation.data && (
              <div className="rounded border border-border bg-surface-elevated p-2 font-mono text-[11px]">
                Task {selfTestMutation.data.task_id.slice(0, 8)}: {selfTestMutation.data.steps_completed} steps ({selfTestMutation.data.python_version})
              </div>
            )}

            {(status?.last_error || selfTestMutation.error) && (
              <div className="rounded border border-red-500/40 bg-red-500/10 p-2 text-red-400">
                {String(selfTestMutation.error ?? status?.last_error)}
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
