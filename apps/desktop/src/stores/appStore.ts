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
  theme: ThemeMode;
  latestProgress: TaskProgressEvent | null;
  setActiveSection: (section: NavSection) => void;
  toggleTheme: () => void;
  setTheme: (theme: ThemeMode) => void;
  setLatestProgress: (progress: TaskProgressEvent | null) => void;
}

export const useAppStore = create<AppStoreState>((set) => ({
  activeSection: "home",
  theme: "dark",
  latestProgress: null,
  setActiveSection: (activeSection) => set({ activeSection }),
  toggleTheme: () =>
    set((state) => ({ theme: state.theme === "dark" ? "light" : "dark" })),
  setTheme: (theme) => set({ theme }),
  setLatestProgress: (latestProgress) => set({ latestProgress }),
}));
