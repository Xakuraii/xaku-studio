import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { DialogueSegmentRecord, SpeakerRecord } from "@xakustudio/shared";
import {
  AlertTriangle,
  BookOpen,
  Languages,
  Plus,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  deleteGlossaryEntry,
  isTauriRuntime,
  listGlossary,
  listProvidersStatus,
  startTranslation,
  updateSegmentTranslation,
  upsertGlossaryEntry,
} from "@/lib/tauri";
import { useAppStore } from "@/stores/appStore";

interface TranslationPanelProps {
  projectId: string | null;
  segments: DialogueSegmentRecord[];
  speakers: SpeakerRecord[];
  isRunning: boolean;
}

export function TranslationPanel({
  projectId,
  segments,
  speakers,
  isRunning,
}: TranslationPanelProps) {
  const inTauri = isTauriRuntime();
  const queryClient = useQueryClient();
  const setActiveSection = useAppStore((s) => s.setActiveSection);

  const [sceneContext, setSceneContext] = useState("");
  const [model, setModel] = useState("gemini-4-hydrogen");
  const [sourceTerm, setSourceTerm] = useState("");
  const [targetTerm, setTargetTerm] = useState("");
  const [termNotes, setTermNotes] = useState("");
  const [draftTranslations, setDraftTranslations] = useState<
    Record<string, string>
  >({});
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const providersQuery = useQuery({
    queryKey: ["providers", "catalog"],
    queryFn: listProvidersStatus,
    enabled: inTauri,
  });

  const glossaryQuery = useQuery({
    queryKey: ["glossary", projectId],
    queryFn: () => listGlossary(projectId ?? undefined),
    enabled: inTauri,
  });

  const translateMutation = useMutation({
    mutationFn: (segmentIds?: string[]) =>
      startTranslation({
        projectId: projectId!,
        segmentIds,
        sceneContext,
        model,
      }),
    onSuccess: () => {
      setErrorMsg(null);
      queryClient.invalidateQueries({ queryKey: ["transcript", projectId] });
    },
    onError: (err) => setErrorMsg(String(err)),
  });

  const saveLineMutation = useMutation({
    mutationFn: ({ id, text }: { id: string; text: string }) =>
      updateSegmentTranslation(id, text),
    onSuccess: () => {
      setErrorMsg(null);
      queryClient.invalidateQueries({ queryKey: ["transcript", projectId] });
    },
    onError: (err) => setErrorMsg(String(err)),
  });

  const addGlossaryMutation = useMutation({
    mutationFn: () =>
      upsertGlossaryEntry({
        projectId,
        sourceTerm,
        targetTerm,
        notes: termNotes,
      }),
    onSuccess: () => {
      setSourceTerm("");
      setTargetTerm("");
      setTermNotes("");
      setErrorMsg(null);
      queryClient.invalidateQueries({ queryKey: ["glossary", projectId] });
    },
    onError: (err) => setErrorMsg(String(err)),
  });

  const deleteGlossaryMutation = useMutation({
    mutationFn: deleteGlossaryEntry,
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ["glossary", projectId] }),
  });

  const geminiProvider = providersQuery.data?.find(
    (p) => p.provider_id === "gemini-translation"
  );
  const geminiMissing = geminiProvider
    ? !geminiProvider.credential_configured
    : false;
  const speakerById = new Map(speakers.map((s) => [s.id, s.display_name]));
  const glossary = glossaryQuery.data ?? [];

  return (
    <div className="space-y-4">
      {geminiMissing && (
        <Card className="border-amber-500/40 bg-amber-500/5">
          <CardContent className="flex flex-wrap items-center justify-between gap-3 pt-4 text-xs text-amber-200">
            <div className="flex items-center gap-2">
              <AlertTriangle className="h-4 w-4 shrink-0 text-amber-400" />
              <span>
                <strong>GEMINI_API_KEY</strong> is not configured. Add your
                Gemini key in Windows Credential Manager to run automated
                Chinese → Khmer translation, or edit lines manually below.
              </span>
            </div>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => setActiveSection("models")}
            >
              Configure GEMINI_API_KEY
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

      <div className="grid gap-6 lg:grid-cols-12">
        <Card className="lg:col-span-4">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <BookOpen className="h-4 w-4 text-primary" />
              Context & Glossary ({glossary.length})
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4 text-xs">
            <div className="space-y-1.5">
              <label htmlFor="trans-model" className="text-muted-foreground">
                Translation Model
              </label>
              <select
                id="trans-model"
                value={model}
                onChange={(e) => setModel(e.target.value)}
                className="w-full rounded border border-border bg-surface-elevated px-2 py-1.5"
              >
                <option value="gemini-4-hydrogen">Gemini 4 Hydrogen (Default)</option>
                <option value="gemini-4-argon">Gemini 4 Argon</option>
                <option value="gemini-3.5-flash-lite">Gemini 3.5 Flash-Lite</option>
              </select>
            </div>

            <div className="space-y-1.5">
              <label htmlFor="scene-ctx" className="text-muted-foreground">
                Scene & Relationship Context
              </label>
              <textarea
                id="scene-ctx"
                rows={3}
                value={sceneContext}
                onChange={(e) => setSceneContext(e.target.value)}
                placeholder="e.g. Modern CEO drama; Speaker 1 is the younger sister speaking respectfully to her brother."
                className="w-full rounded border border-border bg-surface-elevated px-2.5 py-1.5 text-xs"
              />
            </div>

            <Button
              className="w-full"
              size="sm"
              disabled={
                !inTauri ||
                !projectId ||
                segments.length === 0 ||
                isRunning ||
                translateMutation.isPending
              }
              onClick={() => translateMutation.mutate(undefined)}
            >
              <Languages className="h-3.5 w-3.5" />
              {isRunning ? "Translating..." : "Translate All Dialogue"}
            </Button>

            <div className="space-y-2 border-t border-border pt-3">
              <div className="font-medium text-foreground">
                Character & Term Glossary
              </div>
              <div className="grid grid-cols-2 gap-1.5">
                <input
                  type="text"
                  value={sourceTerm}
                  onChange={(e) => setSourceTerm(e.target.value)}
                  placeholder="Source (e.g. 总裁)"
                  className="rounded border border-border bg-surface-elevated px-2 py-1"
                />
                <input
                  type="text"
                  value={targetTerm}
                  onChange={(e) => setTargetTerm(e.target.value)}
                  placeholder="Target (e.g. លោកប្រធាន)"
                  className="rounded border border-border bg-surface-elevated px-2 py-1"
                />
              </div>
              <div className="flex gap-1.5">
                <input
                  type="text"
                  value={termNotes}
                  onChange={(e) => setTermNotes(e.target.value)}
                  placeholder="Notes (optional)"
                  className="min-w-0 flex-1 rounded border border-border bg-surface-elevated px-2 py-1"
                />
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={
                    !inTauri ||
                    !sourceTerm.trim() ||
                    !targetTerm.trim() ||
                    addGlossaryMutation.isPending
                  }
                  onClick={() => addGlossaryMutation.mutate()}
                >
                  <Plus className="h-3.5 w-3.5" />
                  Add
                </Button>
              </div>

              <div className="max-h-56 space-y-1.5 overflow-y-auto pt-1">
                {glossary.map((g) => (
                  <div
                    key={g.id}
                    className="flex items-center justify-between gap-2 rounded border border-border bg-surface-elevated/40 px-2 py-1.5"
                  >
                    <div className="min-w-0">
                      <div className="truncate font-medium">
                        {g.source_term} → {g.target_term}
                      </div>
                      {g.notes && (
                        <div className="truncate text-[10px] text-muted-foreground">
                          {g.notes}
                        </div>
                      )}
                    </div>
                    <Button
                      size="icon"
                      variant="ghost"
                      onClick={() => deleteGlossaryMutation.mutate(g.id)}
                    >
                      <Trash2 className="h-3 w-3 text-red-400" />
                    </Button>
                  </div>
                ))}
              </div>
            </div>
          </CardContent>
        </Card>

        <Card className="lg:col-span-8">
          <CardHeader>
            <CardTitle>Localized Dialogue ({segments.length} lines)</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 text-xs">
            {segments.length === 0 ? (
              <div className="rounded-md border border-dashed border-border py-12 text-center text-muted-foreground">
                Transcribe dialogue first in the <strong>Transcript & Speakers</strong>{" "}
                tab before translating.
              </div>
            ) : (
              segments.map((seg) => {
                const draft = draftTranslations[seg.id] ?? seg.translated_text;
                const speakerName = seg.speaker_id
                  ? speakerById.get(seg.speaker_id) ?? "Speaker"
                  : "Unassigned";
                return (
                  <div
                    key={seg.id}
                    className="space-y-2 rounded-md border border-border bg-surface-elevated/40 p-3"
                  >
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <div className="flex items-center gap-2">
                        <span className="font-mono text-[11px] text-muted-foreground">
                          #{seg.segment_index + 1}
                        </span>
                        <Badge tone="neutral">{speakerName}</Badge>
                        <Badge
                          tone={
                            seg.translation_status === "edited"
                              ? "primary"
                              : seg.translation_status === "auto"
                                ? "success"
                                : "neutral"
                          }
                        >
                          {seg.translation_status}
                        </Badge>
                      </div>

                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={!inTauri || isRunning || translateMutation.isPending}
                        onClick={() => translateMutation.mutate([seg.id])}
                      >
                        <RefreshCw className="h-3 w-3" />
                        Regenerate Line
                      </Button>
                    </div>

                    <div className="rounded bg-surface px-2.5 py-1.5 text-muted-foreground">
                      {seg.source_text}
                    </div>

                    <textarea
                      rows={2}
                      value={draft}
                      placeholder="Enter or generate Khmer translation..."
                      onChange={(e) =>
                        setDraftTranslations((prev) => ({
                          ...prev,
                          [seg.id]: e.target.value,
                        }))
                      }
                      onBlur={() => {
                        if (draft !== seg.translated_text) {
                          saveLineMutation.mutate({ id: seg.id, text: draft });
                        }
                      }}
                      className="w-full rounded border border-border bg-surface px-2.5 py-1.5 text-sm text-foreground focus:border-primary focus:outline-none"
                    />
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
