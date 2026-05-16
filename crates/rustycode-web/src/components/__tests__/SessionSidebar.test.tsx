import { describe, it, expect, afterEach, vi, beforeAll } from "vitest";
import { render, screen, cleanup, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SessionSidebar } from "../SessionSidebar";

afterEach(cleanup);

const mockSessions = [
  { id: "abc-123-def", created_at: new Date().toISOString(), last_active_at: new Date().toISOString(), message_count: 5, client_count: 1 },
  { id: "xyz-789-uvw", created_at: new Date().toISOString(), last_active_at: new Date(Date.now() - 3600000).toISOString(), message_count: 0, client_count: 0 },
];

beforeAll(() => {
  vi.stubGlobal("fetch", vi.fn());
});

function mockFetchSessions(sessions = mockSessions) {
  (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValue({
    ok: true,
    json: () => Promise.resolve(sessions),
  });
}

function mockFetchError() {
  (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValue({
    ok: false,
    json: () => Promise.resolve({}),
  });
}

describe("SessionSidebar", () => {
  it("renders nothing when closed", () => {
    mockFetchSessions();
    render(<SessionSidebar currentSessionId={null} onSelectSession={vi.fn()} onNewSession={vi.fn()} open={false} onClose={vi.fn()} />);
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
  });

  it("renders sidebar when open", async () => {
    mockFetchSessions();
    render(<SessionSidebar currentSessionId={null} onSelectSession={vi.fn()} onNewSession={vi.fn()} open={true} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByRole("complementary")).toBeInTheDocument();
    });
  });

  it("shows Sessions header", async () => {
    mockFetchSessions();
    render(<SessionSidebar currentSessionId={null} onSelectSession={vi.fn()} onNewSession={vi.fn()} open={true} onClose={vi.fn()} />);
    expect(screen.getByText("Sessions")).toBeInTheDocument();
  });

  it("fetches and displays sessions", async () => {
    mockFetchSessions();
    render(<SessionSidebar currentSessionId={null} onSelectSession={vi.fn()} onNewSession={vi.fn()} open={true} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText("5 messages")).toBeInTheDocument();
      expect(screen.getByText("New session")).toBeInTheDocument();
    });
  });

  it("shows session IDs (truncated)", async () => {
    mockFetchSessions();
    render(<SessionSidebar currentSessionId={null} onSelectSession={vi.fn()} onNewSession={vi.fn()} open={true} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText("abc-123-")).toBeInTheDocument();
    });
  });

  it("shows live indicator for connected sessions", async () => {
    mockFetchSessions();
    render(<SessionSidebar currentSessionId={null} onSelectSession={vi.fn()} onNewSession={vi.fn()} open={true} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByLabelText("connected")).toBeInTheDocument();
    });
  });

  it("shows aria-selected for current session", async () => {
    mockFetchSessions();
    render(<SessionSidebar currentSessionId="abc-123-def" onSelectSession={vi.fn()} onNewSession={vi.fn()} open={true} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText("5 messages")).toBeInTheDocument();
    });
    const activeItem = screen.getByRole("option", { selected: true });
    expect(activeItem).toBeInTheDocument();
  });

  it("shows empty state when no sessions", async () => {
    mockFetchSessions([]);
    render(<SessionSidebar currentSessionId={null} onSelectSession={vi.fn()} onNewSession={vi.fn()} open={true} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText("No sessions yet")).toBeInTheDocument();
    });
  });

  it("shows error state when fetch fails", async () => {
    mockFetchError();
    render(<SessionSidebar currentSessionId={null} onSelectSession={vi.fn()} onNewSession={vi.fn()} open={true} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText("Failed to load sessions")).toBeInTheDocument();
      expect(screen.getByText("Retry")).toBeInTheDocument();
    });
  });

  it("calls onNewSession when new button clicked", async () => {
    mockFetchSessions();
    const onNew = vi.fn();
    render(<SessionSidebar currentSessionId={null} onSelectSession={vi.fn()} onNewSession={onNew} open={true} onClose={vi.fn()} />);
    await userEvent.click(screen.getByLabelText("New session"));
    expect(onNew).toHaveBeenCalledTimes(1);
  });

  it("calls onClose when close button clicked", async () => {
    mockFetchSessions();
    const onClose = vi.fn();
    render(<SessionSidebar currentSessionId={null} onSelectSession={vi.fn()} onNewSession={vi.fn()} open={true} onClose={onClose} />);
    await userEvent.click(screen.getByLabelText("Close sidebar"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("calls onSelectSession and onClose when session clicked", async () => {
    mockFetchSessions();
    const onSelect = vi.fn();
    const onClose = vi.fn();
    render(<SessionSidebar currentSessionId={null} onSelectSession={onSelect} onNewSession={vi.fn()} open={true} onClose={onClose} />);
    await waitFor(() => {
      expect(screen.getByText("5 messages")).toBeInTheDocument();
    });
    await userEvent.click(screen.getByText("5 messages"));
    expect(onSelect).toHaveBeenCalledWith("abc-123-def");
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("shows delete button for sessions", async () => {
    mockFetchSessions();
    render(<SessionSidebar currentSessionId={null} onSelectSession={vi.fn()} onNewSession={vi.fn()} open={true} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByLabelText("Delete session abc-123-")).toBeInTheDocument();
    });
  });

  it("has role=listbox on session list", async () => {
    mockFetchSessions();
    render(<SessionSidebar currentSessionId={null} onSelectSession={vi.fn()} onNewSession={vi.fn()} open={true} onClose={vi.fn()} />);
    expect(screen.getByRole("listbox")).toBeInTheDocument();
  });

  it("has aria-label on sidebar", async () => {
    mockFetchSessions();
    render(<SessionSidebar currentSessionId={null} onSelectSession={vi.fn()} onNewSession={vi.fn()} open={true} onClose={vi.fn()} />);
    expect(screen.getByRole("complementary")).toHaveAttribute("aria-label", "Sessions");
  });
});
