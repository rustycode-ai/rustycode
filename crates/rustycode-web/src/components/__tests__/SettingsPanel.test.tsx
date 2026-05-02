import { describe, it, expect, afterEach, vi, beforeAll } from "vitest";
import { render, screen, cleanup, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SettingsPanel } from "../SettingsPanel";

afterEach(cleanup);

const mockServers = {
  servers: [
    { name: "filesystem", command: "npx @anthropic/mcp-fs", args: [], status: "registered" },
    { name: "git", command: "npx @anthropic/mcp-git", args: [], status: "connected" },
  ],
};

beforeAll(() => {
  vi.stubGlobal("fetch", vi.fn());
});

function mockFetchServers() {
  (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValue({
    ok: true,
    json: () => Promise.resolve(mockServers),
  });
}

function mockFetchError() {
  (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValue({
    ok: false,
    json: () => Promise.resolve({}),
  });
}

describe("SettingsPanel", () => {
  it("renders nothing when closed", () => {
    mockFetchServers();
    render(<SettingsPanel provider="openai" model="glm-5.1" open={false} onClose={vi.fn()} />);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("renders dialog when open", async () => {
    mockFetchServers();
    render(<SettingsPanel provider="openai" model="glm-5.1" open={true} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByRole("dialog")).toBeInTheDocument();
    });
  });

  it("shows settings title", async () => {
    mockFetchServers();
    render(<SettingsPanel provider="openai" model="glm-5.1" open={true} onClose={vi.fn()} />);
    expect(screen.getByText("Settings")).toBeInTheDocument();
  });

  it("displays provider and model", async () => {
    mockFetchServers();
    render(<SettingsPanel provider="openai" model="glm-5.1" open={true} onClose={vi.fn()} />);
    expect(screen.getByText("openai")).toBeInTheDocument();
    expect(screen.getByText("glm-5.1")).toBeInTheDocument();
  });

  it("shows MCP servers section", async () => {
    mockFetchServers();
    render(<SettingsPanel provider="openai" model="glm-5.1" open={true} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText("MCP Servers")).toBeInTheDocument();
    });
  });

  it("lists MCP servers", async () => {
    mockFetchServers();
    render(<SettingsPanel provider="openai" model="glm-5.1" open={true} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText("filesystem")).toBeInTheDocument();
      expect(screen.getByText("git")).toBeInTheDocument();
    });
  });

  it("shows server commands", async () => {
    mockFetchServers();
    render(<SettingsPanel provider="openai" model="glm-5.1" open={true} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText("npx @anthropic/mcp-fs")).toBeInTheDocument();
    });
  });

  it("shows restart and remove buttons per server", async () => {
    mockFetchServers();
    render(<SettingsPanel provider="openai" model="glm-5.1" open={true} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByLabelText("Restart filesystem")).toBeInTheDocument();
      expect(screen.getByLabelText("Remove filesystem")).toBeInTheDocument();
    });
  });

  it("shows add server button", async () => {
    mockFetchServers();
    render(<SettingsPanel provider="openai" model="glm-5.1" open={true} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText("filesystem")).toBeInTheDocument();
    });
    expect(screen.getByLabelText("Add MCP server")).toBeInTheDocument();
  });

  it("shows keyboard shortcuts section", async () => {
    mockFetchServers();
    render(<SettingsPanel provider="openai" model="glm-5.1" open={true} onClose={vi.fn()} />);
    expect(screen.getByText("Keyboard Shortcuts")).toBeInTheDocument();
  });

  it("shows error state when fetch fails", async () => {
    mockFetchError();
    render(<SettingsPanel provider="openai" model="glm-5.1" open={true} onClose={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText("Failed to load MCP servers")).toBeInTheDocument();
      expect(screen.getByText("Retry")).toBeInTheDocument();
    });
  });

  it("closes on close button click", async () => {
    mockFetchServers();
    const onClose = vi.fn();
    render(<SettingsPanel provider="openai" model="glm-5.1" open={true} onClose={onClose} />);
    await userEvent.click(screen.getByLabelText("Close settings"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("closes on backdrop click", async () => {
    mockFetchServers();
    const onClose = vi.fn();
    render(<SettingsPanel provider="openai" model="glm-5.1" open={true} onClose={onClose} />);
    await userEvent.click(screen.getByRole("dialog"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("shows security section", async () => {
    mockFetchServers();
    render(<SettingsPanel provider="openai" model="glm-5.1" open={true} onClose={vi.fn()} />);
    expect(screen.getByText("Security")).toBeInTheDocument();
    expect(screen.getByText(/API keys are stored server-side/)).toBeInTheDocument();
  });

  it("shows 'unknown' when provider is empty", async () => {
    mockFetchServers();
    render(<SettingsPanel provider="" model="" open={true} onClose={vi.fn()} />);
    // There are two "unknown" values - one for provider, one for model
    const unknowns = screen.getAllByText("unknown");
    expect(unknowns.length).toBe(2);
  });
});
