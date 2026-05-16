import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { StatusBar } from "../StatusBar";

afterEach(cleanup);

const defaultProps = {
  toolIterationCount: 0,
  pending: false,
  inputTokens: 0,
  outputTokens: 0,
  cacheReadTokens: 0,
  cacheCreationTokens: 0,
  onToggleSidebar: vi.fn(),
  onOpenSettings: vi.fn(),
  onModelSwitch: vi.fn(),
  provider: "openai",
  model: "gpt-4o",
  connectionStatus: "connected" as const,
};

describe("StatusBar", () => {
  it("renders RustyCode title", () => {
    render(<StatusBar {...defaultProps} />);
    expect(screen.getByText("RustyCode")).toBeInTheDocument();
  });

  it("renders connection status dot with connected class", () => {
    render(<StatusBar {...defaultProps} connectionStatus="connected" />);
    expect(document.querySelector(".status-dot-connected")).toBeInTheDocument();
  });

  it("renders connection status dot with disconnected class", () => {
    render(<StatusBar {...defaultProps} connectionStatus="disconnected" />);
    expect(document.querySelector(".status-dot-disconnected")).toBeInTheDocument();
  });

  it("renders pending indicator when pending is true", () => {
    render(<StatusBar {...defaultProps} pending={true} />);
    expect(screen.getByText("Generating")).toBeInTheDocument();
    expect(screen.getByRole("status")).toBeInTheDocument();
  });

  it("hides pending indicator when pending is false", () => {
    render(<StatusBar {...defaultProps} pending={false} />);
    expect(screen.queryByText("Generating")).not.toBeInTheDocument();
  });

  it("shows tool count when greater than zero", () => {
    render(<StatusBar {...defaultProps} toolIterationCount={5} />);
    expect(screen.getByText(/Tools: 5/)).toBeInTheDocument();
  });

  it("hides tool count when zero", () => {
    render(<StatusBar {...defaultProps} toolIterationCount={0} />);
    expect(screen.queryByText(/Tools:/)).not.toBeInTheDocument();
  });

  it("shows token count when tokens are present", () => {
    render(<StatusBar {...defaultProps} inputTokens={1500} outputTokens={500} />);
    expect(screen.getByText("2.0k tokens")).toBeInTheDocument();
  });

  it("hides token count when zero", () => {
    render(<StatusBar {...defaultProps} inputTokens={0} outputTokens={0} />);
    expect(screen.queryByText(/tokens/)).not.toBeInTheDocument();
  });

  it("shows cache info when cache tokens are present", () => {
    render(<StatusBar {...defaultProps} cacheReadTokens={10000} cacheCreationTokens={5000} />);
    expect(screen.getByText(/10.0k cached/)).toBeInTheDocument();
  });

  it("hides cache info when zero", () => {
    render(<StatusBar {...defaultProps} cacheReadTokens={0} />);
    expect(screen.queryByText(/cached/)).not.toBeInTheDocument();
  });

  it("calls onToggleSidebar on menu button click", async () => {
    const onToggle = vi.fn();
    render(<StatusBar {...defaultProps} onToggleSidebar={onToggle} />);
    await userEvent.click(screen.getByRole("button", { name: /Toggle session sidebar/ }));
    expect(onToggle).toHaveBeenCalledTimes(1);
  });

  it("calls onOpenSettings on settings button click", async () => {
    const onOpen = vi.fn();
    render(<StatusBar {...defaultProps} onOpenSettings={onOpen} />);
    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it("has banner role on header", () => {
    render(<StatusBar {...defaultProps} />);
    expect(screen.getByRole("banner")).toBeInTheDocument();
  });

  it("formats large token counts with M suffix", () => {
    render(<StatusBar {...defaultProps} inputTokens={1_500_000} outputTokens={500_000} />);
    expect(screen.getByText("2.0M tokens")).toBeInTheDocument();
  });
});
