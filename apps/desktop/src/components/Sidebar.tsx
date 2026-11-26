import {
  Activity,
  Captions,
  Cpu,
  FolderKanban,
  Home,
  Moon,
  Settings,
  Sliders,
  Sparkles,
  Sun,
  Users,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { useAppStore, type NavSection } from "@/stores/appStore";
import { Button } from "@/components/ui/button";

export interface NavItem {
  id: NavSection;
  label: string;
  icon: typeof Home;
  stageBadge?: string;
}

export const NAV_ITEMS: NavItem[] = [
  { id: "home", label: "Home / Dashboard", icon: Home },
  { id: "projects", label: "Projects", icon: FolderKanban },
  { id: "auto-dubbing", label: "Auto-Dubbing", icon: Sparkles, stageBadge: "S4+" },
  { id: "manual-studio", label: "Manual Studio", icon: Sliders, stageBadge: "S6" },
  { id: "characters", label: "Characters & Voices", icon: Users, stageBadge: "S5" },
  { id: "subtitles", label: "Subtitle Studio", icon: Captions, stageBadge: "S6" },
  { id: "models", label: "Models & Providers", icon: Cpu, stageBadge: "S3" },
  { id: "settings", label: "Settings", icon: Settings },
];

export function Sidebar() {
  const activeSection = useAppStore((s) => s.activeSection);
  const setActiveSection = useAppStore((s) => s.setActiveSection);
  const theme = useAppStore((s) => s.theme);
  const toggleTheme = useAppStore((s) => s.toggleTheme);

  return (
    <aside className="flex h-full w-60 shrink-0 flex-col border-r border-border bg-surface">
      <div className="flex items-center gap-2.5 border-b border-border px-4 py-3.5">
        <div className="flex h-8 w-8 items-center justify-center rounded-md bg-primary text-primary-foreground shadow-xs">
          <Activity className="h-4 w-4" />
        </div>
        <div className="min-w-0">
          <div className="truncate text-sm font-semibold tracking-tight">
            XakuStudio
          </div>
          <div className="truncate text-[11px] text-muted-foreground">
            Video Localization Studio
          </div>
        </div>
      </div>

      <nav className="flex-1 space-y-1 overflow-y-auto p-2.5" aria-label="Primary">
        {NAV_ITEMS.map((item) => {
          const Icon = item.icon;
          const active = activeSection === item.id;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => setActiveSection(item.id)}
              className={cn(
                "flex w-full items-center justify-between rounded-md px-2.5 py-2 text-left text-xs font-medium transition-colors cursor-pointer",
                active
                  ? "bg-primary/15 text-primary"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground"
              )}
            >
              <span className="flex items-center gap-2.5">
                <Icon className="h-4 w-4 shrink-0" />
                <span>{item.label}</span>
              </span>
              {item.stageBadge && (
                <span className="rounded bg-surface-elevated px-1.5 py-0.5 text-[10px] text-muted-foreground border border-border">
                  {item.stageBadge}
                </span>
              )}
            </button>
          );
        })}
      </nav>

      <div className="flex items-center justify-between border-t border-border px-3 py-2.5">
        <span className="text-[11px] text-muted-foreground">v0.1.0 · Stage 2</span>
        <Button
          variant="ghost"
          size="icon"
          onClick={toggleTheme}
          aria-label="Toggle color theme"
          title={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
        >
          {theme === "dark" ? (
            <Sun className="h-4 w-4" />
          ) : (
            <Moon className="h-4 w-4" />
          )}
        </Button>
      </div>
    </aside>
  );
}
