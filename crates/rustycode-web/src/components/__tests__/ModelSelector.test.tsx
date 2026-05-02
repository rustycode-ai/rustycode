import { describe, it, expect, afterEach, vi, beforeAll } from "vitest";
import { render, screen, cleanup, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ModelSelector } from "../ModelSelector";

afterEach(cleanup);

const mockProviders = {
  current: { provider: "openai", model: "glm-5.1" },
  providers: [
    { name: "openai", display_name: "OpenAI", models: ["glm-5.1", "gpt-4o"], default_model: "glm-5.1", available: true },
    { name: "anthropic", display_name: "Anthropic", models: ["claude-opus-4-7", "claude-sonnet-4-6"], default_model: "claude-sonnet-4-6", available: false },
  ],
};

beforeAll(() => {
  vi.stubGlobal("fetch", vi.fn());
});

function mockFetchSuccess() {
  (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValue({
    ok: true,
    json: () => Promise.resolve(mockProviders),
  });
}

function mockFetchError() {
  (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValue({
    ok: false,
    json: () => Promise.resolve({}),
  });
}

describe("ModelSelector", () => {
  it("renders provider/model button in closed state", () => {
    mockFetchSuccess();
    render(<ModelSelector model="glm-5.1" provider="openai" />);
    expect(screen.getByLabelText("Switch model")).toBeInTheDocument();
  });

  it("displays provider and model name", () => {
    mockFetchSuccess();
    render(<ModelSelector model="glm-5.1" provider="openai" />);
    expect(screen.getByText("openai")).toBeInTheDocument();
    expect(screen.getByText("glm-5.1")).toBeInTheDocument();
  });

  it("opens modal on button click", async () => {
    mockFetchSuccess();
    render(<ModelSelector model="glm-5.1" provider="openai" />);
    await userEvent.click(screen.getByLabelText("Switch model"));
    await waitFor(() => {
      expect(screen.getByRole("dialog")).toBeInTheDocument();
    });
  });

  it("shows provider groups after loading", async () => {
    mockFetchSuccess();
    render(<ModelSelector model="glm-5.1" provider="openai" />);
    await userEvent.click(screen.getByLabelText("Switch model"));
    await waitFor(() => {
      expect(screen.getByText("OpenAI")).toBeInTheDocument();
      expect(screen.getByText("Anthropic")).toBeInTheDocument();
    });
  });

  it("shows models under providers", async () => {
    mockFetchSuccess();
    render(<ModelSelector model="glm-5.1" provider="openai" />);
    await userEvent.click(screen.getByLabelText("Switch model"));
    await waitFor(() => {
      expect(screen.getByText("glm-5.1")).toBeInTheDocument();
      expect(screen.getByText("gpt-4o")).toBeInTheDocument();
      expect(screen.getByText("claude-opus-4-7")).toBeInTheDocument();
    });
  });

  it("shows search input when open", async () => {
    mockFetchSuccess();
    render(<ModelSelector model="glm-5.1" provider="openai" />);
    await userEvent.click(screen.getByLabelText("Switch model"));
    await waitFor(() => {
      expect(screen.getByPlaceholderText("Search models...")).toBeInTheDocument();
    });
  });

  it("marks unavailable provider models as disabled", async () => {
    mockFetchSuccess();
    render(<ModelSelector model="glm-5.1" provider="openai" />);
    await userEvent.click(screen.getByLabelText("Switch model"));
    await waitFor(() => {
      expect(screen.getByText("claude-opus-4-7")).toBeInTheDocument();
    });
    // Anthropic models should be disabled
    const anthropicBtn = screen.getByText("claude-opus-4-7").closest("button");
    expect(anthropicBtn).toBeDisabled();
  });

  it("shows error state when fetch fails", async () => {
    mockFetchError();
    render(<ModelSelector model="glm-5.1" provider="openai" />);
    await userEvent.click(screen.getByLabelText("Switch model"));
    await waitFor(() => {
      expect(screen.getByText("Failed to load providers")).toBeInTheDocument();
    });
  });

  it("shows retry button on error", async () => {
    mockFetchError();
    render(<ModelSelector model="glm-5.1" provider="openai" />);
    await userEvent.click(screen.getByLabelText("Switch model"));
    await waitFor(() => {
      expect(screen.getByText("Retry")).toBeInTheDocument();
    });
  });

  it("truncates long model names in closed state", () => {
    mockFetchSuccess();
    render(<ModelSelector model="a-very-long-model-name-that-exceeds-limit" provider="openai" />);
    // Model names >28 chars are truncated
    expect(document.querySelector(".model-name")?.textContent).toContain("...");
  });

  it("closes on backdrop click", async () => {
    mockFetchSuccess();
    render(<ModelSelector model="glm-5.1" provider="openai" />);
    await userEvent.click(screen.getByLabelText("Switch model"));
    await waitFor(() => {
      expect(screen.getByRole("dialog")).toBeInTheDocument();
    });
    await userEvent.click(screen.getByRole("dialog"));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });
});
