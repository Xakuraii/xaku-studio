import { useEffect } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Sidebar } from "@/components/Sidebar";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { HomeDashboard } from "@/features/home/HomeDashboard";
import { ModelsProvidersView } from "@/features/models/ModelsProvidersView";
import { ProjectsView } from "@/features/projects/ProjectsView";
import { SettingsView } from "@/features/settings/SettingsView";
import { TranscriptionView } from "@/features/transcription/TranscriptionView";
import { CharactersVoicesView } from "@/features/voices/CharactersVoicesView";
import { useAppStore, type NavSection } from "@/stores/appStore";

const queryClient = new QueryClient();

const STAGE_DETAILS: Record<
  Exclude<
    NavSection,
    | "home"
    | "projects"
    | "auto-dubbing"
    | "characters"
    | "models"
    | "settings"
  >,
  { title: string; stage: string; description: string }
> = {
  "manual-studio": {
    title: "Manual Studio & Timeline",
    stage: "Stage 6 — Dialogue Sync & Segment Editing",
    description:
      "Interactive audio waveform timeline, line-level timing adjustment, and single-segment regeneration.",
  },
  subtitles: {
    title: "Subtitle Studio",
    stage: "Stage 6 — Khmer Subtitle Styling & Export",
    description:
      "Customizable Khmer subtitle typography (Noto Sans Khmer), positioning, and SRT/VTT/ASS export.",
  },
};

function AppContent() {
  const activeSection = useAppStore((s) => s.activeSection);
  const theme = useAppStore((s) => s.theme);

  useEffect(() => {
    const root = document.documentElement;
    if (theme === "dark") {
      root.classList.add("dark");
    } else {
      root.classList.remove("dark");
    }
  }, [theme]);

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground">
      <Sidebar />
      <main className="flex-1 overflow-y-auto p-6">
        <div className="mx-auto max-w-6xl">
          {activeSection === "home" ? (
            <HomeDashboard />
          ) : activeSection === "projects" ? (
            <ProjectsView />
          ) : activeSection === "auto-dubbing" ? (
            <TranscriptionView />
          ) : activeSection === "characters" ? (
            <CharactersVoicesView />
          ) : activeSection === "models" ? (
            <ModelsProvidersView />
          ) : activeSection === "settings" ? (
            <SettingsView />
          ) : (
            <Card>
              <CardHeader>
                <CardTitle>{STAGE_DETAILS[activeSection].title}</CardTitle>
              </CardHeader>
              <CardContent className="space-y-2 text-sm text-muted-foreground">
                <div className="font-medium text-primary">
                  {STAGE_DETAILS[activeSection].stage}
                </div>
                <p>{STAGE_DETAILS[activeSection].description}</p>
              </CardContent>
            </Card>
          )}
        </div>
      </main>
    </div>
  );
}

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <AppContent />
    </QueryClientProvider>
  );
}

export default App;
