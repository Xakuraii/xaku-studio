import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import {
  FileAudio,
  Film,
  FolderOpen,
  Pencil,
  Plus,
  Trash2,
  Upload,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  createProject,
  deleteProject,
  getDefaultProjectsDir,
  getProjectSummary,
  importProjectMedia,
  isTauriRuntime,
  listProjects,
  openProjectManifest,
  renameProject,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/stores/appStore";

function formatDuration(ms?: number | null): string {
  if (ms == null || ms <= 0) return "—";
  const totalSec = Math.floor(ms / 1000);
  const min = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  return `${min}:${sec.toString().padStart(2, "0")}`;
}

function formatBytes(bytes?: number | null): string {
  if (!bytes) return "—";
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

export function ProjectsView() {
  const inTauri = isTauriRuntime();
  const queryClient = useQueryClient();
  const activeProjectId = useAppStore((s) => s.activeProjectId);
  const setActiveProjectId = useAppStore((s) => s.setActiveProjectId);

  const [newName, setNewName] = useState("");
  const [parentDir, setParentDir] = useState("");
  const [sourceLang, setSourceLang] = useState("zh");
  const [targetLang, setTargetLang] = useState("km");
  const [preset, setPreset] = useState("balanced");
  const [renameDraft, setRenameDraft] = useState("");
  const [isRenaming, setIsRenaming] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  const defaultDirQuery = useQuery({
    queryKey: ["projects", "default-dir"],
    queryFn: getDefaultProjectsDir,
    enabled: inTauri,
  });

  useEffect(() => {
    if (defaultDirQuery.data && !parentDir) {
      setParentDir(defaultDirQuery.data);
    }
  }, [defaultDirQuery.data, parentDir]);

  const projectsQuery = useQuery({
    queryKey: ["projects", "list"],
    queryFn: listProjects,
    enabled: inTauri,
  });

  useEffect(() => {
    if (!activeProjectId && projectsQuery.data && projectsQuery.data.length > 0) {
      setActiveProjectId(projectsQuery.data[0].id);
    }
  }, [activeProjectId, projectsQuery.data, setActiveProjectId]);

  const summaryQuery = useQuery({
    queryKey: ["projects", "summary", activeProjectId],
    queryFn: () => getProjectSummary(activeProjectId!),
    enabled: inTauri && Boolean(activeProjectId),
  });

  const createMutation = useMutation({
    mutationFn: createProject,
    onSuccess: (summary) => {
      setActionError(null);
      setNewName("");
      setActiveProjectId(summary.project.id);
      queryClient.invalidateQueries({ queryKey: ["projects", "list"] });
      queryClient.setQueryData(["projects", "summary", summary.project.id], summary);
    },
    onError: (err) => setActionError(String(err)),
  });

  const openManifestMutation = useMutation({
    mutationFn: openProjectManifest,
    onSuccess: (summary) => {
      setActionError(null);
      setActiveProjectId(summary.project.id);
      queryClient.invalidateQueries({ queryKey: ["projects", "list"] });
      queryClient.setQueryData(["projects", "summary", summary.project.id], summary);
    },
    onError: (err) => setActionError(String(err)),
  });

  const renameMutation = useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) =>
      renameProject(id, name),
    onSuccess: () => {
      setActionError(null);
      setIsRenaming(false);
      queryClient.invalidateQueries({ queryKey: ["projects"] });
    },
    onError: (err) => setActionError(String(err)),
  });

  const deleteMutation = useMutation({
    mutationFn: ({ id, deleteFiles }: { id: string; deleteFiles: boolean }) =>
      deleteProject(id, deleteFiles),
    onSuccess: () => {
      setActionError(null);
      setActiveProjectId(null);
      queryClient.invalidateQueries({ queryKey: ["projects"] });
    },
    onError: (err) => setActionError(String(err)),
  });

  const importMutation = useMutation({
    mutationFn: ({ id, file }: { id: string; file: string }) =>
      importProjectMedia(id, file),
    onSuccess: (summary) => {
      setActionError(null);
      queryClient.setQueryData(["projects", "summary", summary.project.id], summary);
      queryClient.invalidateQueries({ queryKey: ["projects", "list"] });
    },
    onError: (err) => setActionError(String(err)),
  });

  async function handleChooseFolder() {
    if (!inTauri) return;
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Select Parent Folder for Project",
    });
    if (typeof selected === "string") {
      setParentDir(selected);
    }
  }

  async function handleOpenExistingXaku() {
    if (!inTauri) return;
    const selected = await open({
      multiple: false,
      title: "Open project.xaku",
      filters: [{ name: "XakuStudio Project", extensions: ["xaku"] }],
    });
    if (typeof selected === "string") {
      openManifestMutation.mutate(selected);
    }
  }

  async function handleImportVideo() {
    if (!inTauri || !activeProjectId) return;
    const selected = await open({
      multiple: false,
      title: "Import Source Video or Audio",
      filters: [
        {
          name: "Media Files",
          extensions: ["mp4", "mkv", "mov", "webm", "avi", "wav", "mp3", "m4a"],
        },
      ],
    });
    if (typeof selected === "string") {
      importMutation.mutate({ id: activeProjectId, file: selected });
    }
  }

  const projects = projectsQuery.data ?? [];
  const summary = summaryQuery.data;
  const inspection = summary?.inspection;
  const peaks = summary?.waveform_peaks;

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold tracking-tight">Projects</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Create localization projects, import source dramas non-destructively,
            and inspect media streams via FFprobe.
          </p>
        </div>

        <Button
          variant="secondary"
          size="sm"
          disabled={!inTauri || openManifestMutation.isPending}
          onClick={handleOpenExistingXaku}
        >
          <FolderOpen className="h-3.5 w-3.5" />
          Open .xaku Project
        </Button>
      </div>

      {!inTauri && (
        <Card className="border-amber-500/40 bg-amber-500/5">
          <CardContent className="pt-4 text-xs text-amber-300">
            Project persistence and FFmpeg media import require the Tauri desktop
            runtime (<code className="font-mono">npm run dev</code>).
          </CardContent>
        </Card>
      )}

      {actionError && (
        <Card className="border-red-500/40 bg-red-500/10">
          <CardContent className="pt-4 text-xs text-red-400">
            {actionError}
          </CardContent>
        </Card>
      )}

      <div className="grid gap-6 lg:grid-cols-12">
        <div className="space-y-4 lg:col-span-4">
          <Card>
            <CardHeader>
              <CardTitle>New Project</CardTitle>
            </CardHeader>
            <CardContent className="space-y-3 text-xs">
              <div className="space-y-1">
                <label htmlFor="project-name" className="text-muted-foreground">
                  Project Name
                </label>
                <input
                  id="project-name"
                  type="text"
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  placeholder="e.g. Moonlight Drama Ep 01"
                  className="w-full rounded-md border border-border bg-surface-elevated px-2.5 py-1.5 text-sm text-foreground focus:border-primary focus:outline-none"
                />
              </div>

              <div className="grid grid-cols-2 gap-2">
                <div className="space-y-1">
                  <label htmlFor="source-lang" className="text-muted-foreground">
                    Source
                  </label>
                  <select
                    id="source-lang"
                    value={sourceLang}
                    onChange={(e) => setSourceLang(e.target.value)}
                    className="w-full rounded-md border border-border bg-surface-elevated px-2 py-1.5 text-xs text-foreground"
                  >
                    <option value="zh">Chinese (zh)</option>
                    <option value="en">English (en)</option>
                    <option value="km">Khmer (km)</option>
                  </select>
                </div>

                <div className="space-y-1">
                  <label htmlFor="target-lang" className="text-muted-foreground">
                    Target
                  </label>
                  <select
                    id="target-lang"
                    value={targetLang}
                    onChange={(e) => setTargetLang(e.target.value)}
                    className="w-full rounded-md border border-border bg-surface-elevated px-2 py-1.5 text-xs text-foreground"
                  >
                    <option value="km">Khmer (km)</option>
                    <option value="en">English (en)</option>
                  </select>
                </div>
              </div>

              <div className="space-y-1">
                <label htmlFor="preset" className="text-muted-foreground">
                  Hardware Preset
                </label>
                <select
                  id="preset"
                  value={preset}
                  onChange={(e) => setPreset(e.target.value)}
                  className="w-full rounded-md border border-border bg-surface-elevated px-2 py-1.5 text-xs text-foreground"
                >
                  <option value="performance">Performance (Low VRAM)</option>
                  <option value="balanced">Balanced (Default)</option>
                  <option value="quality">Quality</option>
                </select>
              </div>

              <div className="space-y-1">
                <span className="text-muted-foreground">Location</span>
                <div className="flex gap-1.5">
                  <input
                    type="text"
                    value={parentDir}
                    onChange={(e) => setParentDir(e.target.value)}
                    placeholder="Select parent directory"
                    className="min-w-0 flex-1 rounded-md border border-border bg-surface-elevated px-2 py-1.5 font-mono text-[11px] text-foreground"
                  />
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    disabled={!inTauri}
                    onClick={handleChooseFolder}
                  >
                    Browse
                  </Button>
                </div>
              </div>

              <Button
                className="w-full"
                size="sm"
                disabled={
                  !inTauri ||
                  !newName.trim() ||
                  !parentDir.trim() ||
                  createMutation.isPending
                }
                onClick={() =>
                  createMutation.mutate({
                    parentDir,
                    name: newName,
                    sourceLang,
                    targetLang,
                    preset,
                  })
                }
              >
                <Plus className="h-3.5 w-3.5" />
                {createMutation.isPending ? "Creating..." : "Create Project"}
              </Button>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Saved Projects ({projects.length})</CardTitle>
            </CardHeader>
            <CardContent className="space-y-1.5">
              {projects.length === 0 ? (
                <p className="text-xs text-muted-foreground">
                  No projects yet. Create one above or open an existing{" "}
                  <code className="font-mono">project.xaku</code> folder.
                </p>
              ) : (
                projects.map((proj) => {
                  const selected = proj.id === activeProjectId;
                  return (
                    <button
                      key={proj.id}
                      type="button"
                      onClick={() => {
                        setIsRenaming(false);
                        setActiveProjectId(proj.id);
                      }}
                      className={cn(
                        "flex w-full items-center justify-between rounded-md border px-2.5 py-2 text-left text-xs transition-colors cursor-pointer",
                        selected
                          ? "border-primary/50 bg-primary/10 text-foreground"
                          : "border-border bg-surface-elevated/40 text-muted-foreground hover:text-foreground"
                      )}
                    >
                      <div className="min-w-0">
                        <div className="truncate font-medium text-foreground">
                          {proj.name}
                        </div>
                        <div className="truncate font-mono text-[10px] text-muted-foreground">
                          {proj.source_lang} → {proj.target_lang} · {proj.preset}
                        </div>
                      </div>
                    </button>
                  );
                })
              )}
            </CardContent>
          </Card>
        </div>

        <div className="space-y-4 lg:col-span-8">
          {!summary ? (
            <Card>
              <CardContent className="py-12 text-center text-sm text-muted-foreground">
                Select or create a project to import source media and inspect streams.
              </CardContent>
            </Card>
          ) : (
            <>
              <Card>
                <CardHeader className="flex-row flex-wrap items-center justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    {isRenaming ? (
                      <div className="flex items-center gap-2">
                        <input
                          type="text"
                          value={renameDraft}
                          onChange={(e) => setRenameDraft(e.target.value)}
                          className="rounded border border-border bg-surface-elevated px-2 py-1 text-sm"
                        />
                        <Button
                          size="sm"
                          onClick={() =>
                            renameMutation.mutate({
                              id: summary.project.id,
                              name: renameDraft,
                            })
                          }
                        >
                          Save
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => setIsRenaming(false)}
                        >
                          Cancel
                        </Button>
                      </div>
                    ) : (
                      <CardTitle className="flex items-center gap-2 text-base">
                        <span>{summary.project.name}</span>
                        <Badge tone="primary">
                          {summary.project.source_lang} → {summary.project.target_lang}
                        </Badge>
                      </CardTitle>
                    )}
                    <div className="mt-1 truncate font-mono text-[11px] text-muted-foreground">
                      {summary.project.root_path}
                    </div>
                  </div>

                  <div className="flex items-center gap-2">
                    <Button
                      size="sm"
                      disabled={importMutation.isPending}
                      onClick={handleImportVideo}
                    >
                      <Upload className="h-3.5 w-3.5" />
                      {importMutation.isPending ? "Importing..." : "Import Media"}
                    </Button>
                    <Button
                      size="icon"
                      variant="secondary"
                      title="Rename project"
                      onClick={() => {
                        setRenameDraft(summary.project.name);
                        setIsRenaming(true);
                      }}
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </Button>
                    <Button
                      size="icon"
                      variant="ghost"
                      title="Remove project from library (keeps files on disk)"
                      onClick={() =>
                        deleteMutation.mutate({
                          id: summary.project.id,
                          deleteFiles: false,
                        })
                      }
                    >
                      <Trash2 className="h-3.5 w-3.5 text-red-400" />
                    </Button>
                  </div>
                </CardHeader>
                <CardContent className="space-y-4">
                  {summary.source_preview_url ? (
                    <div className="overflow-hidden rounded-md border border-border bg-black">
                      <video
                        key={summary.source_preview_url}
                        src={summary.source_preview_url}
                        controls
                        className="max-h-80 w-full object-contain"
                      />
                    </div>
                  ) : (
                    <div className="rounded-md border border-dashed border-border py-10 text-center text-xs text-muted-foreground">
                      No media imported yet. Click <strong>Import Media</strong> to copy
                      a source video non-destructively into this project.
                    </div>
                  )}

                  {peaks && peaks.length > 0 && (
                    <div className="space-y-1.5">
                      <div className="flex items-center justify-between text-xs text-muted-foreground">
                        <span className="flex items-center gap-1.5">
                          <FileAudio className="h-3.5 w-3.5 text-primary" />
                          Extracted Audio Waveform (16 kHz AI Stem)
                        </span>
                        <span className="font-mono">{peaks.length} buckets</span>
                      </div>
                      <div className="flex h-14 items-center gap-[1px] rounded border border-border bg-surface-elevated px-2 py-1">
                        {peaks.slice(0, 160).map((peak, i) => (
                          <div
                            key={i}
                            className="flex-1 rounded-xs bg-primary/80"
                            style={{ height: `${Math.max(6, Math.round(peak * 100))}%` }}
                          />
                        ))}
                      </div>
                    </div>
                  )}
                </CardContent>
              </Card>

              {inspection && (
                <Card>
                  <CardHeader>
                    <CardTitle className="flex items-center gap-2">
                      <Film className="h-4 w-4 text-primary" />
                      Media Inspection (FFprobe)
                    </CardTitle>
                  </CardHeader>
                  <CardContent className="grid gap-4 text-xs sm:grid-cols-2">
                    <div className="space-y-1.5">
                      <div className="flex justify-between">
                        <span className="text-muted-foreground">Container</span>
                        <span className="font-mono">{inspection.format_name}</span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-muted-foreground">Duration</span>
                        <span className="font-mono">
                          {formatDuration(inspection.duration_ms)}
                        </span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-muted-foreground">Size</span>
                        <span className="font-mono">
                          {formatBytes(inspection.size_bytes)}
                        </span>
                      </div>
                      {inspection.video && (
                        <div className="flex justify-between">
                          <span className="text-muted-foreground">Video</span>
                          <span className="font-mono">
                            {inspection.video.codec} · {inspection.video.width}×
                            {inspection.video.height}
                          </span>
                        </div>
                      )}
                    </div>

                    <div className="space-y-1.5">
                      <div className="flex justify-between">
                        <span className="text-muted-foreground">Audio Streams</span>
                        <span className="font-mono">
                          {inspection.audio_streams.length > 0
                            ? inspection.audio_streams
                                .map((a) => `${a.codec} ${a.channels}ch ${a.sample_rate}Hz`)
                                .join(", ")
                            : "None"}
                        </span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-muted-foreground">
                          Embedded Subtitles
                        </span>
                        <span className="font-mono">
                          {inspection.subtitle_streams.length > 0
                            ? inspection.subtitle_streams
                                .map((s) => `${s.codec}${s.language ? ` (${s.language})` : ""}`)
                                .join(", ")
                            : "None detected"}
                        </span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-muted-foreground">Project Assets</span>
                        <span className="font-mono">{summary.assets.length} files</span>
                      </div>
                    </div>
                  </CardContent>
                </Card>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
