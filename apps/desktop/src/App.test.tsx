import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import App from "@/App";
import { useAppStore } from "@/stores/appStore";

describe("XakuStudio workstation shell", () => {
  beforeEach(() => {
    useAppStore.setState({
      activeSection: "home",
      theme: "dark",
      latestProgress: null,
    });
  });

  it("renders workstation dashboard and switches sections", () => {
    render(<App />);

    expect(screen.getByText("XakuStudio")).toBeInTheDocument();
    expect(screen.getByText("Workstation Foundation")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /Settings/i }));
    expect(screen.getByText("Appearance")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Light" }));
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });
});
