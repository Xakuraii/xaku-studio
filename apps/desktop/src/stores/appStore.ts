import { create } from "zustand";
import type { TaskProgressEvent } from "@xakustudio/shared";

export type NavSection =
  | "home"
  | "projects"
  | "auto-dubbing"
  | "manual-studio"
  | "characters"
  | "subtitles"
  | "models"
  | "settings";

export type ThemeMode = "dark" | "light";

interface AppStoreState {
  activeSection: NavSection;
  activeProjectId: string | null;
  theme: ThemeMode;
  latestProgress: TaskProgressEvent | null;
  setActiveSection: (section: NavSection) => void;
  setActiveProjectId: (projectId: string | null) => void;
  toggleTheme: () => void;
  setTheme: (theme: ThemeMode) => void;
  setLatestProgress: (progress: TaskProgressEvent | null) => void;
}

export const useAppStore = create<AppStoreState>((set) => ({
  activeSection: "home",
  activeProjectId: null,
  theme: "dark",
  latestProgress: null,
  setActiveSection: (activeSection) => set({ activeSection }),
  setActiveProjectId: (activeProjectId) => set({ activeProjectId }),
  toggleTheme: () =>
    set((state) => ({ theme: state.theme === "dark" ? "light" : "dark" })),
  setTheme: (theme) => set({ theme }),
  setLatestProgress: (latestProgress) => set({ latestProgress }),
}));
