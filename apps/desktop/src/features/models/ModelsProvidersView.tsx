import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Cpu,
  Download,
  KeyRound,
  Layers,
  Pause,
  Play,
  RotateCcw,
  ShieldCheck,
  Trash2,
  XCircle,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  cancelAssetDownload,
  cancelScheduledTask,
  deleteProviderCredential,
  enqueueDiagnosticDag,
  getSystemResources,
  isTauriRuntime,
  listDownloads,
  listProvidersStatus,
  listScheduledTasks,
  pauseAssetDownload,
  resumeAssetDownload,
  retryScheduledTask,
  setProviderCredential,
  subscribeSchedulerEvents,
} from "@/lib/tauri";

function statusTone(status: string) {
  switch (status) {
    case "completed":
      return "success" as const;
    case "running":
    case "downloading":
      return "primary" as const;
    case "queued":
    case "paused":
    case "interrupted":
      return "warning" as const;
    case "failed":
    case "cancelled":
      return "danger" as const;
    default:
      return "neutral" as const;
  }
}

export function ModelsProvidersView() {
  const inTauri = isTauriRuntime();
  const queryClient = useQueryClient();
  const [editingKey, setEditingKey] = useState<string | null>(null);
  const [secretInput, setSecretInput] = useState("");
  const [feedback, setFeedback] = useState<string | null>(null);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    subscribeSchedulerEvents(
      () => {
        queryClient.invalidateQueries({ queryKey: ["scheduler", "tasks"] });
      },
      () => {
        queryClient.invalidateQueries({ queryKey: ["downloads", "list"] });
      }
    ).then((fn) => {
      unsub = fn;
    });
    return () => {
      unsub?.();
    };
  }, [queryClient]);

  const resourcesQuery = useQuery({
    queryKey: ["system", "resources"],
    queryFn: getSystemResources,
    enabled: inTauri,
    refetchInterval: 10_000,
  });

  const providersQuery = useQuery({
    queryKey: ["providers", "catalog"],
    queryFn: listProvidersStatus,
    enabled: inTauri,
  });

  const tasksQuery = useQuery({
    queryKey: ["scheduler", "tasks"],
    queryFn: () => listScheduledTasks(),
    enabled: inTauri,
    refetchInterval: 4_000,
  });

  const downloadsQuery = useQuery({
    queryKey: ["downloads", "list"],
    queryFn: listDownloads,
    enabled: inTauri,
    refetchInterval: 4_000,
  });

  const saveCredMutation = useMutation({
    mutationFn: ({ keyName, secret }: { keyName: string; secret: string }) =>
      setProviderCredential(keyName, secret),
    onSuccess: () => {
      setEditingKey(null);
      setSecretInput("");
      setFeedback("Credential saved to Windows Credential Manager.");
      queryClient.invalidateQueries({ queryKey: ["providers", "catalog"] });
    },
    onError: (err) => setFeedback(String(err)),
  });

  const deleteCredMutation = useMutation({
    mutationFn: deleteProviderCredential,
    onSuccess: () => {
      setFeedback("Credential removed from Windows Credential Manager.");
      queryClient.invalidateQueries({ queryKey: ["providers", "catalog"] });
    },
    onError: (err) => setFeedback(String(err)),
  });

  const runDagMutation = useMutation({
    mutationFn: () => enqueueDiagnosticDag(),
    onSuccess: () => {
      setFeedback("Enqueued 4-node diagnostic DAG (1 GPU slot enforced).");
      queryClient.invalidateQueries({ queryKey: ["scheduler", "tasks"] });
    },
    onError: (err) => setFeedback(String(err)),
  });

  const cancelTaskMutation = useMutation({
    mutationFn: cancelScheduledTask,
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ["scheduler", "tasks"] }),
  });

  const retryTaskMutation = useMutation({
    mutationFn: retryScheduledTask,
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ["scheduler", "tasks"] }),
  });

  const pauseDlMutation = useMutation({
    mutationFn: pauseAssetDownload,
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ["downloads", "list"] }),
  });

  const resumeDlMutation = useMutation({
    mutationFn: resumeAssetDownload,
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ["downloads", "list"] }),
  });

  const cancelDlMutation = useMutation({
    mutationFn: cancelAssetDownload,
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ["downloads", "list"] }),
  });

  const res = resourcesQuery.data;
  const providers = providersQuery.data ?? [];
  const tasks = tasksQuery.data ?? [];
  const downloads = downloadsQuery.data ?? [];

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold tracking-tight">
            Models, Providers & Task Queue
          </h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Hardware-aware concurrency limits, OS-backed API credentials, AI
            provider status, and download/scheduler queues.
          </p>
        </div>

        <Button
          size="sm"
          disabled={!inTauri || runDagMutation.isPending}
          onClick={() => runDagMutation.mutate()}
        >
          <Play className="h-3.5 w-3.5" />
          Run Diagnostic DAG
        </Button>
      </div>

      {feedback && (
        <Card className="border-primary/40 bg-primary/5">
          <CardContent className="pt-4 text-xs text-foreground">
            {feedback}
          </CardContent>
        </Card>
      )}

      <div className="grid gap-4 md:grid-cols-4">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Cpu className="h-4 w-4 text-primary" />
              GPU & VRAM
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-1 text-xs">
            <div className="truncate font-mono">{res?.gpu_name ?? "—"}</div>
            <div className="text-muted-foreground">
              {res?.vram_total_mb != null
                ? `${res.vram_used_mb ?? 0} / ${res.vram_total_mb} MiB used`
                : "No NVIDIA GPU probed"}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Concurrency Slots</CardTitle>
          </CardHeader>
          <CardContent className="space-y-1 text-xs font-mono">
            <div>GPU Heavy: {res?.gpu_heavy_slots ?? 1}</div>
            <div>CPU Light: {res?.cpu_light_slots ?? 4}</div>
            <div>Network I/O: {res?.network_io_slots ?? 3}</div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Recommended Preset</CardTitle>
          </CardHeader>
          <CardContent className="space-y-1 text-xs">
            <Badge tone="primary">{res?.recommended_preset ?? "balanced"}</Badge>
            <div className="text-muted-foreground">
              {res?.cpu_threads ?? "—"} logical CPU threads
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <ShieldCheck className="h-4 w-4 text-primary" />
              Credential Store
            </CardTitle>
          </CardHeader>
          <CardContent className="text-xs text-muted-foreground">
            Secrets are stored exclusively in Windows Credential Manager—never in
            SQLite or <code className="font-mono">.xaku</code> files.
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>AI Provider Catalog ({providers.length})</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {providers.length === 0 ? (
            <p className="text-xs text-muted-foreground">
              {inTauri
                ? "Loading provider catalog..."
                : "Launch the Tauri desktop app to inspect providers and credentials."}
            </p>
          ) : (
            providers.map((p) => (
              <div
                key={p.provider_id}
                className="flex flex-col gap-3 rounded-md border border-border bg-surface-elevated/40 p-3 text-xs md:flex-row md:items-center md:justify-between"
              >
                <div className="space-y-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-semibold text-foreground">
                      {p.name}
                    </span>
                    <Badge tone="neutral">{p.capability.toUpperCase()}</Badge>
                    <Badge tone={p.is_local ? "primary" : "neutral"}>
                      {p.is_local ? "Local" : "Cloud"}
                    </Badge>
                    <Badge tone={p.available ? "success" : "warning"}>
                      {p.available ? "Ready" : p.version}
                    </Badge>
                  </div>
                  <div className="text-muted-foreground">{p.notes}</div>
                  <div className="font-mono text-[11px] text-muted-foreground">
                    Languages: {p.supported_languages.join(", ")} · Hardware:{" "}
                    {p.required_hardware} · License: {p.license}
                  </div>
                </div>

                {p.credential_key && (
                  <div className="flex flex-col items-end gap-2 shrink-0">
                    {editingKey === p.credential_key ? (
                      <div className="flex items-center gap-1.5">
                        <input
                          type="password"
                          value={secretInput}
                          onChange={(e) => setSecretInput(e.target.value)}
                          placeholder={`Enter ${p.credential_key}`}
                          className="w-48 rounded border border-border bg-surface px-2 py-1 text-xs"
                        />
                        <Button
                          size="sm"
                          disabled={!secretInput.trim() || saveCredMutation.isPending}
                          onClick={() =>
                            saveCredMutation.mutate({
                              keyName: p.credential_key!,
                              secret: secretInput.trim(),
                            })
                          }
                        >
                          Save
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => {
                            setEditingKey(null);
                            setSecretInput("");
                          }}
                        >
                          Cancel
                        </Button>
                      </div>
                    ) : (
                      <div className="flex items-center gap-2">
                        <Badge tone={p.credential_configured ? "success" : "warning"}>
                          <KeyRound className="h-3 w-3" />
                          {p.credential_key}:{" "}
                          {p.credential_configured ? "Configured" : "Not Set"}
                        </Badge>
                        <Button
                          size="sm"
                          variant="secondary"
                          onClick={() => {
                            setEditingKey(p.credential_key);
                            setSecretInput("");
                          }}
                        >
                          {p.credential_configured ? "Update" : "Set Key"}
                        </Button>
                        {p.credential_configured && (
                          <Button
                            size="icon"
                            variant="ghost"
                            title="Delete credential"
                            onClick={() =>
                              deleteCredMutation.mutate(p.credential_key!)
                            }
                          >
                            <Trash2 className="h-3.5 w-3.5 text-red-400" />
                          </Button>
                        )}
                      </div>
                    )}
                  </div>
                )}
              </div>
            ))
          )}
        </CardContent>
      </Card>

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Layers className="h-4 w-4 text-primary" />
              Scheduled Tasks ({tasks.length})
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 text-xs">
            {tasks.length === 0 ? (
              <p className="text-muted-foreground">
                No tasks scheduled yet. Click <strong>Run Diagnostic DAG</strong>{" "}
                above to verify dependency-aware GPU/CPU scheduling.
              </p>
            ) : (
              tasks.slice(0, 12).map((t) => (
                <div
                  key={t.id}
                  className="flex items-center justify-between gap-2 rounded border border-border bg-surface-elevated/40 px-2.5 py-2"
                >
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="font-mono font-medium text-foreground">
                        {t.task_type}
                      </span>
                      <Badge tone={statusTone(t.status)}>{t.status}</Badge>
                      <span className="font-mono text-[10px] text-muted-foreground">
                        {t.resource_class}
                      </span>
                    </div>
                    <div className="truncate text-muted-foreground">
                      {t.error_message || t.stage_message}
                    </div>
                  </div>

                  <div className="flex items-center gap-1 shrink-0">
                    {(t.status === "queued" || t.status === "running") && (
                      <Button
                        size="icon"
                        variant="ghost"
                        title="Cancel task"
                        onClick={() => cancelTaskMutation.mutate(t.id)}
                      >
                        <XCircle className="h-3.5 w-3.5 text-red-400" />
                      </Button>
                    )}
                    {(t.status === "failed" ||
                      t.status === "cancelled" ||
                      t.status === "interrupted") && (
                      <Button
                        size="icon"
                        variant="ghost"
                        title="Retry task"
                        onClick={() => retryTaskMutation.mutate(t.id)}
                      >
                        <RotateCcw className="h-3.5 w-3.5" />
                      </Button>
                    )}
                  </div>
                </div>
              ))
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Download className="h-4 w-4 text-primary" />
              Asset Downloads ({downloads.length})
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 text-xs">
            {downloads.length === 0 ? (
              <p className="text-muted-foreground">
                No model or font downloads queued. Model downloads will use HTTP
                Range resume and SHA-256 verification.
              </p>
            ) : (
              downloads.map((d) => (
                <div
                  key={d.id}
                  className="flex items-center justify-between gap-2 rounded border border-border bg-surface-elevated/40 px-2.5 py-2"
                >
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="truncate font-mono font-medium">
                        {d.resource_id}
                      </span>
                      <Badge tone={statusTone(d.status)}>{d.status}</Badge>
                    </div>
                    <div className="truncate text-muted-foreground">
                      {d.downloaded_bytes}
                      {d.total_bytes ? ` / ${d.total_bytes} bytes` : " bytes"}
                      {d.error_message ? ` · ${d.error_message}` : ""}
                    </div>
                  </div>
                  <div className="flex items-center gap-1 shrink-0">
                    {d.status === "downloading" && (
                      <Button
                        size="icon"
                        variant="ghost"
                        title="Pause download"
                        onClick={() => pauseDlMutation.mutate(d.id)}
                      >
                        <Pause className="h-3.5 w-3.5" />
                      </Button>
                    )}
                    {(d.status === "paused" || d.status === "failed") && (
                      <Button
                        size="icon"
                        variant="ghost"
                        title="Resume download"
                        onClick={() => resumeDlMutation.mutate(d.id)}
                      >
                        <Play className="h-3.5 w-3.5" />
                      </Button>
                    )}
                    {d.status !== "completed" && d.status !== "cancelled" && (
                      <Button
                        size="icon"
                        variant="ghost"
                        title="Cancel download"
                        onClick={() => cancelDlMutation.mutate(d.id)}
                      >
                        <XCircle className="h-3.5 w-3.5 text-red-400" />
                      </Button>
                    )}
                  </div>
                </div>
              ))
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
