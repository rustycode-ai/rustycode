import { describe, it, expect, afterEach, vi, beforeAll } from "vitest";
import { render, screen, cleanup, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { CommandPalette } from "../CommandPalette";
import type { FrontendMessage } from "../../protocol/types";

afterEach(cleanup);

const mockSkills = {
  skills: [
    { id: "tdd", name: "TDD Guide", description: "Write tests first", categories: ["Development"] },
    { id: "refactor", name: "Refactor", description: "Clean up code", categories: ["Maintenance"] },
  ],
};

beforeAll(() => {
  vi.stubGlobal("fetch", vi.fn());
  Element.prototype.scrollIntoView = vi.fn();
});

function mockFetchSkills() {
  (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValue({
    ok: true,
    json: () => Promise.resolve(mockSkills),
  });
}

function makeMessages(): FrontendMessage[] {
  return [
    { id: "m1", content: "Hello", kind: "User", parts: [] },
    { id: "m2", content: "Hi there", kind: "Assistant", parts: [] },
  ];
}

describe("CommandPalette", () => {
  it("renders nothing when closed", () => {
    mockFetchSkills();
    render(<CommandPalette open={false} onClose={vi.fn()} sessionToken="tok" messages={[]} />);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("renders dialog when open", async () => {
    mockFetchSkills();
    render(<CommandPalette open={true} onClose={vi.fn()} sessionToken="tok" messages={[]} />);
    await waitFor(() => {
      expect(screen.getByRole("dialog")).toBeInTheDocument();
    });
  });

  it("shows search input when open", async () => {
    mockFetchSkills();
    render(<CommandPalette open={true} onClose={vi.fn()} sessionToken="tok" messages={[]} />);
    await waitFor(() => {
      expect(screen.getByPlaceholderText("Search commands and skills...")).toBeInTheDocument();
    });
  });

  it("loads and displays skills", async () => {
    mockFetchSkills();
    render(<CommandPalette open={true} onClose={vi.fn()} sessionToken="tok" messages={[]} />);
    await waitFor(() => {
      expect(screen.getByText("TDD Guide")).toBeInTheDocument();
      expect(screen.getByText("Refactor")).toBeInTheDocument();
    });
  });

  it("shows action items", async () => {
    mockFetchSkills();
    render(<CommandPalette open={true} onClose={vi.fn()} sessionToken="tok" messages={[]} />);
    await waitFor(() => {
      expect(screen.getByText("Toggle Sidebar")).toBeInTheDocument();
      expect(screen.getByText("Switch Model")).toBeInTheDocument();
      expect(screen.getByText("Export Conversation")).toBeInTheDocument();
    });
  });

  it("filters items by search", async () => {
    mockFetchSkills();
    render(<CommandPalette open={true} onClose={vi.fn()} sessionToken="tok" messages={[]} />);
    await waitFor(() => {
      expect(screen.getByText("TDD Guide")).toBeInTheDocument();
    });
    await userEvent.type(screen.getByPlaceholderText("Search commands and skills..."), "sidebar");
    expect(screen.getByText("Toggle Sidebar")).toBeInTheDocument();
    expect(screen.queryByText("TDD Guide")).not.toBeInTheDocument();
  });

  it("shows no results for unmatched search", async () => {
    mockFetchSkills();
    render(<CommandPalette open={true} onClose={vi.fn()} sessionToken="tok" messages={[]} />);
    await waitFor(() => {
      expect(screen.getByText("TDD Guide")).toBeInTheDocument();
    });
    await userEvent.type(screen.getByPlaceholderText("Search commands and skills..."), "xyz123");
    expect(screen.getByText("No results found.")).toBeInTheDocument();
  });

  it("calls onClose on backdrop click", async () => {
    mockFetchSkills();
    const onClose = vi.fn();
    render(<CommandPalette open={true} onClose={onClose} sessionToken="tok" messages={[]} />);
    await waitFor(() => {
      expect(screen.getByRole("dialog")).toBeInTheDocument();
    });
    await userEvent.click(screen.getByRole("dialog"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("shows skill badge for skill items", async () => {
    mockFetchSkills();
    render(<CommandPalette open={true} onClose={vi.fn()} sessionToken="tok" messages={[]} />);
    await waitFor(() => {
      const badges = screen.queryAllByText("skill");
      expect(badges.length).toBeGreaterThanOrEqual(1);
    });
  });

  it("has aria-modal=true", async () => {
    mockFetchSkills();
    render(<CommandPalette open={true} onClose={vi.fn()} sessionToken="tok" messages={[]} />);
    await waitFor(() => {
      expect(screen.getByRole("dialog")).toHaveAttribute("aria-modal", "true");
    });
  });
});
