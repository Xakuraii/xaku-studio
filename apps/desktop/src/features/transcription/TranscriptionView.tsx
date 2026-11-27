import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { DialogueSegmentRecord, SpeakerRecord } from "@xakustudio/shared";
import {
  AlertTriangle,
  GitMerge,
  Play,
  Plus,
  Scissors,
  Sparkles,
  UserCheck,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  createSpeaker,
  getTranscript,
  isTauriRuntime,
  listProjects,
  listProvidersStatus,
  mergeSpeakers,
  renameSpeaker,
  setSpeakerVoicePresentation,
  splitSegment,
  startTranscription,
  subscribeSchedulerEvents,
  subscribeWorkerEvents,
  updateSegmentSpeaker,
  updateSegmentText,
} from "@/lib/tauri";
import { useAppStore } from "@/stores/appStore";

function formatMs(ms: number): string {
  const totalSec = ms / 1000;
  const mins = Math.floor(totalSec / 60);
  const secs = (totalSec % 60).toFixed(2).padStart(5, "0");
  return `${mins}:${secs}`;
}

function taskTone(status: string) {
  switch (status) {
    case "completed":
      return "success" as const;
    case "running":
      return "primary" as const;
    case "queued":
      return "warning" as const;
    case "failed":
    case "cancelled":
    case "interrupted":
      return "danger" as const;
    default:
      return "neutral" as const;
  }
}

export function TranscriptionView() {
  const inTauri = isTauriRuntime();
  const queryClient = useQueryClient();
  const activeProjectId = useAppStore((s) => s.activeProjectId);
  const setActiveProjectId = useAppStore((s) => s.setActiveProjectId);
  const setActiveSection = useAppStore((s) => s.setActiveSection);

  const [modelSize, setModelSize] = useState("auto");
  const [newSpeakerName, setNewSpeakerName] = useState("");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [draftTexts, setDraftTexts] = useState<Record<string, string>>({});
  const [mergeTargets, setMergeTargets] = useState<Record<string, string>>({});
  const textareaRefs = useRef<Record<string, HTMLTextAreaElement | null>>({});

  useEffect(() => {
    let unsubSched: (() => void) | undefined;
    let unsubWorker: (() => void) | undefined;

    subscribeSchedulerEvents(
      () => {
        if (activeProjectId) {
          queryClient.invalidateQueries({
            queryKey: ["transcript", activeProjectId],
          });
        }
      },
      () => {}
    ).then((fn) => {
      unsubSched = fn;
    });

    subscribeWorkerEvents(
      () => {},
      () => {
        if (activeProjectId) {
          queryClient.invalidateQueries({
            queryKey: ["transcript", activeProjectId],
          });
        }
      }
    ).then((fn) => {
      unsubWorker = fn;
    });

    return () => {
      unsubSched?.();
      unsubWorker?.();
    };
  }, [activeProjectId, queryClient]);

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

  const providersQuery = useQuery({
    queryKey: ["providers", "catalog"],
    queryFn: listProvidersStatus,
    enabled: inTauri,
  });

  const transcriptQuery = useQuery({
    queryKey: ["transcript", activeProjectId],
    queryFn: () => getTranscript(activeProjectId!),
    enabled: inTauri && Boolean(activeProjectId),
    refetchInterval: (q) => {
      const hasActive = q.state.data?.latest_run_tasks.some(
        (t) => t.status === "queued" || t.status === "running"
      );
      return hasActive ? 2_000 : false;
    },
  });

  useEffect(() => {
    if (transcriptQuery.data?.segments) {
      const next: Record<string, string> = {};
      for (const s of transcriptQuery.data.segments) {
        next[s.id] = s.source_text;
      }
      setDraftTexts(next);
    }
  }, [transcriptQuery.data?.segments]);

  const startMutation = useMutation({
    mutationFn: () => startTranscription(activeProjectId!, modelSize),
    onSuccess: () => {
      setErrorMsg(null);
      queryClient.invalidateQueries({
        queryKey: ["transcript", activeProjectId],
      });
    },
    onError: (err) => setErrorMsg(String(err)),
  });

  const saveTextMutation = useMutation({
    mutationFn: ({ id, text }: { id: string; text: string }) =>
      updateSegmentText(id, text),
    onSuccess: () => {
      setErrorMsg(null);
      queryClient.invalidateQueries({
        queryKey: ["transcript", activeProjectId],
      });
    },
    onError: (err) => setErrorMsg(String(err)),
  });

  const assignSpeakerMutation = useMutation({
    mutationFn: ({
      segmentId,
      speakerId,
    }: {
      segmentId: string;
      speakerId: string | null;
    }) => updateSegmentSpeaker(segmentId, speakerId),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["transcript", activeProjectId],
      });
    },
    onError: (err) => setErrorMsg(String(err)),
  });

  const createSpeakerMutation = useMutation({
    mutationFn: () => createSpeaker(activeProjectId!, newSpeakerName),
    onSuccess: () => {
      setNewSpeakerName("");
      setErrorMsg(null);
      queryClient.invalidateQueries({
        queryKey: ["transcript", activeProjectId],
      });
    },
    onError: (err) => setErrorMsg(String(err)),
  });

  const renameSpeakerMutation = useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) =>
      renameSpeaker(id, name),
    onSuccess: () =>
      queryClient.invalidateQueries({
        queryKey: ["transcript", activeProjectId],
      }),
    onError: (err) => setErrorMsg(String(err)),
  });

  const presentationMutation = useMutation({
    mutationFn: ({
      id,
      value,
    }: {
      id: string;
      value: "masculine" | "feminine" | "uncertain";
    }) => setSpeakerVoicePresentation(id, value),
    onSuccess: () =>
      queryClient.invalidateQueries({
        queryKey: ["transcript", activeProjectId],
      }),
    onError: (err) => setErrorMsg(String(err)),
  });

  const mergeMutation = useMutation({
    mutationFn: ({ sourceId, targetId }: { sourceId: string; targetId: string }) =>
      mergeSpeakers(sourceId, targetId),
    onSuccess: () => {
      setErrorMsg(null);
      queryClient.invalidateQueries({
        queryKey: ["transcript", activeProjectId],
      });
    },
    onError: (err) => setErrorMsg(String(err)),
  });

  const splitMutation = useMutation({
    mutationFn: ({ segmentId, charIndex }: { segmentId: string; charIndex: number }) =>
      splitSegment(segmentId, charIndex),
    onSuccess: () => {
      setErrorMsg(null);
      queryClient.invalidateQueries({
        queryKey: ["transcript", activeProjectId],
      });
    },
    onError: (err) => setErrorMsg(String(err)),
  });

  const projects = projectsQuery.data ?? [];
  const pyannoteProvider = providersQuery.data?.find(
    (p) => p.provider_id === "pyannote-diarization"
  );
  const hfMissing = pyannoteProvider ? !pyannoteProvider.credential_configured : false;

  const payload = transcriptQuery.data;
  const speakers: SpeakerRecord[] = payload?.speakers ?? [];
  const segments: DialogueSegmentRecord[] = payload?.segments ?? [];
  const runTasks = payload?.latest_run_tasks ?? [];
  const isRunning = runTasks.some(
    (t) => t.status === "queued" || t.status === "running"
  );

  function handleSplit(seg: DialogueSegmentRecord) {
    const el = textareaRefs.current[seg.id];
    const currentText = draftTexts[seg.id] ?? seg.source_text;
    const rawPos = el?.selectionStart ?? Math.floor(currentText.length / 2);
    const charIndex =
      rawPos > 0 && rawPos < currentText.length
        ? rawPos
        : Math.floor(currentText.length / 2);
    splitMutation.mutate({ segmentId: seg.id, charIndex });
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold tracking-tight">
            Transcription & Speakers
          </h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Transcribe Chinese dialogue with Faster-Whisper + Silero VAD,
            diarize speakers, review vocal presentation estimates, and edit
            segments before translation.
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <select
            aria-label="Active project"
            value={activeProjectId ?? ""}
            onChange={(e) => setActiveProjectId(e.target.value || null)}
            className="rounded-md border border-border bg-surface-elevated px-2.5 py-1.5 text-xs text-foreground"
          >
            {projects.length === 0 ? (
              <option value="">No projects</option>
            ) : (
              projects.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name} ({p.source_lang} → {p.target_lang})
                </option>
              ))
            )}
          </select>

          <select
            aria-label="Whisper model size"
            value={modelSize}
            onChange={(e) => setModelSize(e.target.value)}
            className="rounded-md border border-border bg-surface-elevated px-2.5 py-1.5 text-xs text-foreground"
          >
            <option value="auto">Model: Auto (Preset)</option>
            <option value="small">Model: small</option>
            <option value="medium">Model: medium</option>
            <option value="large-v3-turbo">Model: large-v3-turbo</option>
          </select>

          <Button
            size="sm"
            disabled={
              !inTauri || !activeProjectId || isRunning || startMutation.isPending
            }
            onClick={() => startMutation.mutate()}
          >
            <Play className="h-3.5 w-3.5" />
            {isRunning ? "Processing..." : "Transcribe & Detect Speakers"}
          </Button>
        </div>
      </div>

      {!inTauri && (
        <Card className="border-amber-500/40 bg-amber-500/5">
          <CardContent className="pt-4 text-xs text-amber-300">
            Transcription and speaker diarization require the Tauri desktop shell
            (<code className="font-mono">npm run dev</code>).
          </CardContent>
        </Card>
      )}

      {hfMissing && (
        <Card className="border-amber-500/40 bg-amber-500/5">
          <CardContent className="flex flex-wrap items-center justify-between gap-3 pt-4 text-xs text-amber-200">
            <div className="flex items-center gap-2">
              <AlertTriangle className="h-4 w-4 shrink-0 text-amber-400" />
              <span>
                <strong>HF_TOKEN</strong> is not configured. Transcription will
                still succeed, but automatic speaker diarization requires a
                Hugging Face token with{" "}
                <code className="font-mono">
                  pyannote/speaker-diarization-community-1
                </code>{" "}
                terms accepted.
              </span>
            </div>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => setActiveSection("models")}
            >
              Configure HF_TOKEN
            </Button>
          </CardContent>
        </Card>
      )}

      {errorMsg && (
        <Card className="border-red-500/40 bg-red-500/10">
          <CardContent className="pt-4 text-xs text-red-400">
            {errorMsg}
          </CardContent>
        </Card>
      )}

      {runTasks.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Sparkles className="h-4 w-4 text-primary" />
              Latest Pipeline Run
            </CardTitle>
          </CardHeader>
          <CardContent className="grid gap-2 sm:grid-cols-2 lg:grid-cols-5">
            {runTasks.map((t) => (
              <div
                key={t.id}
                className="space-y-1 rounded border border-border bg-surface-elevated/40 p-2.5 text-xs"
              >
                <div className="flex items-center justify-between gap-1">
                  <span className="truncate font-mono font-medium">
                    {t.task_type}
                  </span>
                  <Badge tone={taskTone(t.status)}>{t.status}</Badge>
                </div>
                <div className="truncate text-[11px] text-muted-foreground">
                  {t.error_message || t.stage_message}
                </div>
                <div className="h-1.5 w-full overflow-hidden rounded-full bg-surface">
                  <div
                    className="h-full bg-primary transition-all duration-150"
                    style={{ width: `${Math.max(0, Math.min(100, t.progress))}%` }}
                  />
                </div>
              </div>
            ))}
          </CardContent>
        </Card>
      )}

      <div className="grid gap-6 lg:grid-cols-12">
        <Card className="lg:col-span-4">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <UserCheck className="h-4 w-4 text-primary" />
              Speakers ({speakers.length})
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 text-xs">
            <div className="flex gap-1.5">
              <input
                type="text"
                value={newSpeakerName}
                onChange={(e) => setNewSpeakerName(e.target.value)}
                placeholder="Add character / speaker"
                className="min-w-0 flex-1 rounded border border-border bg-surface-elevated px-2 py-1.5 text-xs"
              />
              <Button
                size="sm"
                variant="secondary"
                disabled={
                  !inTauri ||
                  !activeProjectId ||
                  !newSpeakerName.trim() ||
                  createSpeakerMutation.isPending
                }
                onClick={() => createSpeakerMutation.mutate()}
              >
                <Plus className="h-3.5 w-3.5" />
                Add
              </Button>
            </div>

            {speakers.length === 0 ? (
              <p className="text-muted-foreground">
                No speakers yet. Run transcription + diarization or add speakers
                manually to assign lines.
              </p>
            ) : (
              speakers.map((sp) => {
                const otherSpeakers = speakers.filter((o) => o.id !== sp.id);
                const selectedMerge =
                  mergeTargets[sp.id] ?? otherSpeakers[0]?.id ?? "";
                return (
                  <div
                    key={sp.id}
                    className="space-y-2 rounded-md border border-border bg-surface-elevated/40 p-2.5"
                  >
                    <div className="flex items-center gap-1.5">
                      <input
                        type="text"
                        defaultValue={sp.display_name}
                        onBlur={(e) => {
                          const val = e.target.value.trim();
                          if (val && val !== sp.display_name) {
                            renameSpeakerMutation.mutate({ id: sp.id, name: val });
                          }
                        }}
                        className="min-w-0 flex-1 rounded border border-border bg-surface px-2 py-1 text-xs font-medium"
                      />
                      <Badge tone="neutral">
                        {sp.pitch_hz_avg != null ? `${sp.pitch_hz_avg.toFixed(0)} Hz` : "no pitch"}
                      </Badge>
                    </div>

                    <div className="flex items-center justify-between gap-2">
                      <span className="text-[11px] text-muted-foreground">
                        Voice presentation estimate (
                        {Math.round(sp.gender_confidence * 100)}%)
                        {sp.is_user_overridden ? " · overridden" : ""}
                      </span>
                      <select
                        value={sp.voice_gender_est}
                        onChange={(e) =>
                          presentationMutation.mutate({
                            id: sp.id,
                            value: e.target.value as
                              | "masculine"
                              | "feminine"
                              | "uncertain",
                          })
                        }
                        className="rounded border border-border bg-surface px-1.5 py-1 text-[11px]"
                      >
                        <option value="uncertain">uncertain</option>
                        <option value="masculine">masculine</option>
                        <option value="feminine">feminine</option>
                      </select>
                    </div>

                    {otherSpeakers.length > 0 && (
                      <div className="flex items-center gap-1.5 pt-1">
                        <select
                          value={selectedMerge}
                          onChange={(e) =>
                            setMergeTargets((prev) => ({
                              ...prev,
                              [sp.id]: e.target.value,
                            }))
                          }
                          className="min-w-0 flex-1 rounded border border-border bg-surface px-1.5 py-1 text-[11px]"
                        >
                          {otherSpeakers.map((other) => (
                            <option key={other.id} value={other.id}>
                              Merge into {other.display_name}
                            </option>
                          ))}
                        </select>
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={!selectedMerge || mergeMutation.isPending}
                          onClick={() =>
                            mergeMutation.mutate({
                              sourceId: sp.id,
                              targetId: selectedMerge,
                            })
                          }
                        >
                          <GitMerge className="h-3 w-3" />
                          Merge
                        </Button>
                      </div>
                    )}
                  </div>
                );
              })
            )}
          </CardContent>
        </Card>

        <Card className="lg:col-span-8">
          <CardHeader>
            <CardTitle>Dialogue Segments ({segments.length})</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 text-xs">
            {segments.length === 0 ? (
              <div className="rounded-md border border-dashed border-border py-12 text-center text-muted-foreground">
                No dialogue segments transcribed for this project yet. Import a
                video in <strong>Projects</strong> and click{" "}
                <strong>Transcribe & Detect Speakers</strong>.
              </div>
            ) : (
              segments.map((seg) => {
                const draft = draftTexts[seg.id] ?? seg.source_text;
                const isEdited = seg.source_text !== seg.source_text_original;
                return (
                  <div
                    key={seg.id}
                    className="space-y-2 rounded-md border border-border bg-surface-elevated/40 p-3"
                  >
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <div className="flex items-center gap-2 font-mono text-[11px] text-muted-foreground">
                        <span>#{seg.segment_index + 1}</span>
                        <span>
                          {formatMs(seg.start_ms)} → {formatMs(seg.end_ms)}
                        </span>
                        {isEdited && <Badge tone="primary">edited</Badge>}
                      </div>

                      <div className="flex items-center gap-2">
                        <select
                          aria-label={`Speaker for segment ${seg.segment_index + 1}`}
                          value={seg.speaker_id ?? ""}
                          onChange={(e) =>
                            assignSpeakerMutation.mutate({
                              segmentId: seg.id,
                              speakerId: e.target.value || null,
                            })
                          }
                          className="rounded border border-border bg-surface px-2 py-1 text-xs"
                        >
                          <option value="">Unassigned</option>
                          {speakers.map((sp) => (
                            <option key={sp.id} value={sp.id}>
                              {sp.display_name}
                            </option>
                          ))}
                        </select>

                        <Button
                          size="sm"
                          variant="secondary"
                          title="Split segment at cursor position"
                          disabled={splitMutation.isPending}
                          onClick={() => handleSplit(seg)}
                        >
                          <Scissors className="h-3 w-3" />
                          Split
                        </Button>
                      </div>
                    </div>

                    <textarea
                      ref={(el) => {
                        textareaRefs.current[seg.id] = el;
                      }}
                      rows={2}
                      value={draft}
                      onChange={(e) =>
                        setDraftTexts((prev) => ({
                          ...prev,
                          [seg.id]: e.target.value,
                        }))
                      }
                      onBlur={() => {
                        if (draft !== seg.source_text) {
                          saveTextMutation.mutate({ id: seg.id, text: draft });
                        }
                      }}
                      className="w-full rounded border border-border bg-surface px-2.5 py-1.5 text-sm text-foreground focus:border-primary focus:outline-none"
                    />

                    {isEdited && (
                      <div className="text-[11px] text-muted-foreground">
                        Original AI transcript:{" "}
                        <span className="italic">{seg.source_text_original}</span>
                      </div>
                    )}
                  </div>
                );
              })
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
