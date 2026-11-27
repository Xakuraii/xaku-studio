import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { SpeakerRecord, VoicePreset } from "@xakustudio/shared";
import {
  Play,
  RefreshCw,
  Users,
  Volume2,
  Wand2,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  assignSpeakerVoice,
  getSegmentAudioUrls,
  getTranscript,
  isTauriRuntime,
  listProjects,
  listTtsVoices,
  previewTtsVoice,
  subscribeSchedulerEvents,
  synthesizeProjectTts,
  synthesizeSegmentTts,
} from "@/lib/tauri";
import { useAppStore } from "@/stores/appStore";

interface VoiceDraft {
  voiceId: string;
  speedRatio: number;
  pitchHzDelta: number;
}

function parseVoiceDraft(
  sp: SpeakerRecord,
  voices: VoicePreset[],
  targetLang: string
): VoiceDraft {
  let speedRatio = 1.0;
  let pitchHzDelta = 0;
  try {
    const parsed = JSON.parse(sp.voice_settings_json || "{}");
    if (typeof parsed.speed_ratio === "number") speedRatio = parsed.speed_ratio;
    if (typeof parsed.pitch_hz_delta === "number")
      pitchHzDelta = parsed.pitch_hz_delta;
  } catch {
    // ignore malformed json
  }

  let defaultVoice = sp.voice_id ?? "";
  if (!defaultVoice && voices.length > 0) {
    const preferredGender =
      sp.voice_gender_est === "masculine" ? "masculine" : "feminine";
    defaultVoice =
      voices.find(
        (v) =>
          v.language === targetLang && v.gender_presentation === preferredGender
      )?.voice_id ??
      voices.find((v) => v.language === targetLang)?.voice_id ??
      voices[0].voice_id;
  }

  return { voiceId: defaultVoice, speedRatio, pitchHzDelta };
}

export function CharactersVoicesView() {
  const inTauri = isTauriRuntime();
  const queryClient = useQueryClient();
  const activeProjectId = useAppStore((s) => s.activeProjectId);
  const setActiveProjectId = useAppStore((s) => s.setActiveProjectId);

  const [drafts, setDrafts] = useState<Record<string, VoiceDraft>>({});
  const [previewAudioUrl, setPreviewAudioUrl] = useState<string | null>(null);
  const [sampleText, setSampleText] = useState(
    "សួស្តី! នេះជាការសាកល្បងសំឡេងតួអង្គ។"
  );
  const [feedback, setFeedback] = useState<string | null>(null);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    subscribeSchedulerEvents(
      () => {
        if (activeProjectId) {
          queryClient.invalidateQueries({
            queryKey: ["transcript", activeProjectId],
          });
          queryClient.invalidateQueries({
            queryKey: ["tts-urls", activeProjectId],
          });
        }
      },
      () => {}
    ).then((fn) => {
      unsub = fn;
    });
    return () => {
      unsub?.();
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

  const voicesQuery = useQuery({
    queryKey: ["tts", "voices"],
    queryFn: listTtsVoices,
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

  const audioUrlsQuery = useQuery({
    queryKey: ["tts-urls", activeProjectId],
    queryFn: () => getSegmentAudioUrls(activeProjectId!),
    enabled: inTauri && Boolean(activeProjectId),
  });

  const projects = projectsQuery.data ?? [];
  const activeProject = projects.find((p) => p.id === activeProjectId);
  const targetLang = activeProject?.target_lang ?? "km";
  const voices = voicesQuery.data ?? [];
  const speakers = transcriptQuery.data?.speakers ?? [];
  const segments = transcriptQuery.data?.segments ?? [];
  const audioUrls = audioUrlsQuery.data ?? {};

  const assignMutation = useMutation({
    mutationFn: ({
      speakerId,
      draft,
    }: {
      speakerId: string;
      draft: VoiceDraft;
    }) =>
      assignSpeakerVoice({
        speakerId,
        voiceProviderId: "edge-neural-tts",
        voiceId: draft.voiceId,
        voiceSettingsJson: JSON.stringify({
          speed_ratio: draft.speedRatio,
          pitch_hz_delta: draft.pitchHzDelta,
        }),
      }),
    onSuccess: () => {
      setFeedback("Character voice assignment saved.");
      queryClient.invalidateQueries({
        queryKey: ["transcript", activeProjectId],
      });
    },
    onError: (err) => setFeedback(String(err)),
  });

  const previewMutation = useMutation({
    mutationFn: (draft: VoiceDraft) =>
      previewTtsVoice({
        text: sampleText,
        voiceId: draft.voiceId,
        speedRatio: draft.speedRatio,
        pitchHzDelta: draft.pitchHzDelta,
      }),
    onSuccess: (res) => {
      setFeedback(null);
      setPreviewAudioUrl(res.preview_url);
    },
    onError: (err) => setFeedback(String(err)),
  });

  const synthAllMutation = useMutation({
    mutationFn: () => synthesizeProjectTts(activeProjectId!),
    onSuccess: () => {
      setFeedback("Queued batch Khmer speech generation.");
      queryClient.invalidateQueries({
        queryKey: ["transcript", activeProjectId],
      });
    },
    onError: (err) => setFeedback(String(err)),
  });

  const synthLineMutation = useMutation({
    mutationFn: synthesizeSegmentTts,
    onSuccess: () => {
      setFeedback(null);
      queryClient.invalidateQueries({
        queryKey: ["transcript", activeProjectId],
      });
      queryClient.invalidateQueries({
        queryKey: ["tts-urls", activeProjectId],
      });
    },
    onError: (err) => setFeedback(String(err)),
  });

  function getDraft(sp: SpeakerRecord): VoiceDraft {
    return drafts[sp.id] ?? parseVoiceDraft(sp, voices, targetLang);
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold tracking-tight">
            Characters & Voices
          </h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Assign distinct Khmer neural voices to characters, audition speaking
            speed & pitch, and synthesize dialogue per character or line.
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

          <Button
            size="sm"
            disabled={
              !inTauri ||
              !activeProjectId ||
              segments.length === 0 ||
              synthAllMutation.isPending
            }
            onClick={() => synthAllMutation.mutate()}
          >
            <Wand2 className="h-3.5 w-3.5" />
            Generate All Dialogue
          </Button>
        </div>
      </div>

      {feedback && (
        <Card className="border-primary/40 bg-primary/5">
          <CardContent className="pt-4 text-xs text-foreground">
            {feedback}
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader className="flex-row flex-wrap items-center justify-between gap-3">
          <CardTitle className="flex items-center gap-2">
            <Volume2 className="h-4 w-4 text-primary" />
            Voice Audition Sample
          </CardTitle>
          {previewAudioUrl && (
            <audio key={previewAudioUrl} src={previewAudioUrl} controls autoPlay className="h-8" />
          )}
        </CardHeader>
        <CardContent className="text-xs">
          <input
            type="text"
            value={sampleText}
            onChange={(e) => setSampleText(e.target.value)}
            className="w-full rounded border border-border bg-surface-elevated px-2.5 py-1.5 text-sm"
            placeholder="Enter sample Khmer or English phrase to preview character voices"
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Users className="h-4 w-4 text-primary" />
            Character Voice Profiles ({speakers.length})
          </CardTitle>
        </CardHeader>
        <CardContent className="grid gap-4 md:grid-cols-2">
          {speakers.length === 0 ? (
            <p className="text-xs text-muted-foreground">
              No speakers found for this project. Run transcription + diarization
              or add speakers under <strong>Auto-Dubbing</strong>.
            </p>
          ) : (
            speakers.map((sp) => {
              const draft = getDraft(sp);
              return (
                <div
                  key={sp.id}
                  className="space-y-3 rounded-md border border-border bg-surface-elevated/40 p-3 text-xs"
                >
                  <div className="flex items-center justify-between gap-2">
                    <div>
                      <div className="text-sm font-semibold">
                        {sp.display_name}
                      </div>
                      <div className="text-[11px] text-muted-foreground">
                        Presentation: {sp.voice_gender_est}
                        {sp.pitch_hz_avg != null
                          ? ` · ${sp.pitch_hz_avg.toFixed(0)} Hz`
                          : ""}
                      </div>
                    </div>
                    <Badge tone={sp.voice_id ? "success" : "warning"}>
                      {sp.voice_id ?? "Unassigned"}
                    </Badge>
                  </div>

                  <div className="space-y-1">
                    <label className="text-muted-foreground">Target Voice</label>
                    <select
                      value={draft.voiceId}
                      onChange={(e) =>
                        setDrafts((prev) => ({
                          ...prev,
                          [sp.id]: { ...draft, voiceId: e.target.value },
                        }))
                      }
                      className="w-full rounded border border-border bg-surface px-2 py-1.5"
                    >
                      {voices.map((v) => (
                        <option key={v.voice_id} value={v.voice_id}>
                          {v.display_name} ({v.language})
                        </option>
                      ))}
                    </select>
                  </div>

                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <div className="flex justify-between text-muted-foreground">
                        <span>Speed</span>
                        <span className="font-mono">
                          {draft.speedRatio.toFixed(2)}x
                        </span>
                      </div>
                      <input
                        type="range"
                        min={0.75}
                        max={1.4}
                        step={0.05}
                        value={draft.speedRatio}
                        onChange={(e) =>
                          setDrafts((prev) => ({
                            ...prev,
                            [sp.id]: {
                              ...draft,
                              speedRatio: parseFloat(e.target.value),
                            },
                          }))
                        }
                        className="w-full accent-primary"
                      />
                    </div>

                    <div>
                      <div className="flex justify-between text-muted-foreground">
                        <span>Pitch</span>
                        <span className="font-mono">
                          {draft.pitchHzDelta >= 0 ? "+" : ""}
                          {draft.pitchHzDelta} Hz
                        </span>
                      </div>
                      <input
                        type="range"
                        min={-25}
                        max={25}
                        step={1}
                        value={draft.pitchHzDelta}
                        onChange={(e) =>
                          setDrafts((prev) => ({
                            ...prev,
                            [sp.id]: {
                              ...draft,
                              pitchHzDelta: parseInt(e.target.value, 10),
                            },
                          }))
                        }
                        className="w-full accent-primary"
                      />
                    </div>
                  </div>

                  <div className="flex items-center gap-2">
                    <Button
                      size="sm"
                      disabled={!inTauri || !draft.voiceId || assignMutation.isPending}
                      onClick={() =>
                        assignMutation.mutate({ speakerId: sp.id, draft })
                      }
                    >
                      Save Voice
                    </Button>
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={
                        !inTauri ||
                        !draft.voiceId ||
                        !sampleText.trim() ||
                        previewMutation.isPending
                      }
                      onClick={() => previewMutation.mutate(draft)}
                    >
                      <Play className="h-3 w-3" />
                      Preview Voice
                    </Button>
                  </div>
                </div>
              );
            })
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Synthesized Dialogue Lines ({segments.length})</CardTitle>
        </CardHeader>
        <CardContent className="space-y-2.5 text-xs">
          {segments.length === 0 ? (
            <p className="text-muted-foreground">
              No dialogue lines yet. Transcribe and translate lines first.
            </p>
          ) : (
            segments.map((seg) => {
              const targetDur = Math.max(1, seg.end_ms - seg.start_ms);
              const url = audioUrls[seg.id];
              const speaker = speakers.find((s) => s.id === seg.speaker_id);
              const ratio =
                seg.tts_duration_ms != null
                  ? seg.tts_duration_ms / targetDur
                  : null;
              return (
                <div
                  key={seg.id}
                  className="flex flex-col gap-2 rounded border border-border bg-surface-elevated/40 p-3 md:flex-row md:items-center md:justify-between"
                >
                  <div className="min-w-0 flex-1 space-y-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="font-mono text-[11px] text-muted-foreground">
                        #{seg.segment_index + 1}
                      </span>
                      <Badge tone="neutral">
                        {speaker?.display_name ?? "Unassigned"}
                      </Badge>
                      {seg.tts_voice_id && (
                        <Badge tone="success">{seg.tts_voice_id}</Badge>
                      )}
                      {ratio != null && (
                        <Badge tone={ratio > 1.35 ? "warning" : "neutral"}>
                          TTS {(seg.tts_duration_ms! / 1000).toFixed(2)}s / Slot{" "}
                          {(targetDur / 1000).toFixed(2)}s
                        </Badge>
                      )}
                    </div>
                    <div className="text-sm text-foreground">
                      {seg.translated_text || (
                        <span className="italic text-muted-foreground">
                          Not translated yet
                        </span>
                      )}
                    </div>
                  </div>

                  <div className="flex flex-wrap items-center gap-2 shrink-0">
                    {url && <audio key={url} src={url} controls className="h-8" />}
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={
                        !inTauri ||
                        !seg.translated_text.trim() ||
                        !speaker?.voice_id ||
                        synthLineMutation.isPending
                      }
                      onClick={() => synthLineMutation.mutate(seg.id)}
                    >
                      <RefreshCw className="h-3 w-3" />
                      {url ? "Regenerate" : "Synthesize"}
                    </Button>
                  </div>
                </div>
              );
            })
          )}
        </CardContent>
      </Card>
    </div>
  );
}
