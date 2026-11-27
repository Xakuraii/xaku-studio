import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import App from "@/App";
import { useAppStore } from "@/stores/appStore";

describe("XakuStudio workstation shell", () => {
  beforeEach(() => {
    useAppStore.setState({
      activeSection: "home",
      activeProjectId: null,
      theme: "dark",
      latestProgress: null,
    });
  });

  it("renders workstation dashboard, navigates across Stage 5 views, and switches themes", () => {
    render(<App />);

    expect(screen.getByText("XakuStudio")).toBeInTheDocument();
    expect(screen.getByText("Workstation Foundation")).toBeInTheDocument();
    expect(screen.getByText("FFmpeg Engine")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /^Auto-Dubbing$/i }));
    expect(screen.getByText("Transcription & Speakers")).toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", { name: /Translation & Glossary/i })
    );
    expect(screen.getByText(/Context & Glossary/i)).toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", { name: /Characters & Voices/i })
    );
    expect(screen.getByText("Voice Audition Sample")).toBeInTheDocument();
    expect(screen.getByText(/Character Voice Profiles/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /^Projects$/i }));
    expect(screen.getByText("New Project")).toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", { name: /Models & Providers/i })
    );
    expect(
      screen.getByText("Models, Providers & Task Queue")
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /Settings/i }));
    expect(screen.getByText("Appearance")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Light" }));
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });
});
