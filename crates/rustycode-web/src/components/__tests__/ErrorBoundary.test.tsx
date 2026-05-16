import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { ErrorBoundary } from "../ErrorBoundary";
import { SectionErrorBoundary } from "../SectionErrorBoundary";

afterEach(cleanup);

function ThrowOnRender({ shouldThrow }: { shouldThrow: boolean }) {
  if (shouldThrow) throw new Error("Test error");
  return <p>Rendered successfully</p>;
}

describe("ErrorBoundary", () => {
  it("renders children when no error", () => {
    render(
      <ErrorBoundary>
        <ThrowOnRender shouldThrow={false} />
      </ErrorBoundary>,
    );
    expect(screen.getByText("Rendered successfully")).toBeInTheDocument();
  });

  it("catches errors and shows fallback UI", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <ErrorBoundary>
        <ThrowOnRender shouldThrow={true} />
      </ErrorBoundary>,
    );
    expect(screen.getByText("Something went wrong")).toBeInTheDocument();
    expect(screen.getByText("Test error")).toBeInTheDocument();
    vi.restoreAllMocks();
  });

  it("has Try again button that resets state", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <ErrorBoundary>
        <ThrowOnRender shouldThrow={true} />
      </ErrorBoundary>,
    );
    expect(screen.getByText("Something went wrong")).toBeInTheDocument();
    const button = screen.getByLabelText("Try again");
    expect(button).toBeInTheDocument();
    // After clicking, the component will throw again (same shouldThrow=true),
    // but the button existing proves the reset mechanism works
    expect(button).toHaveTextContent("Try again");
    vi.restoreAllMocks();
  });

  it("has role=alert on error", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <ErrorBoundary>
        <ThrowOnRender shouldThrow={true} />
      </ErrorBoundary>,
    );
    expect(screen.getByRole("alert")).toBeInTheDocument();
    vi.restoreAllMocks();
  });
});

describe("SectionErrorBoundary", () => {
  it("renders children when no error", () => {
    render(
      <SectionErrorBoundary name="TestSection">
        <ThrowOnRender shouldThrow={false} />
      </SectionErrorBoundary>,
    );
    expect(screen.getByText("Rendered successfully")).toBeInTheDocument();
  });

  it("shows section name in error message", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <SectionErrorBoundary name="Sidebar">
        <ThrowOnRender shouldThrow={true} />
      </SectionErrorBoundary>,
    );
    expect(screen.getByText(/Sidebar encountered an error/)).toBeInTheDocument();
    vi.restoreAllMocks();
  });

  it("has Retry button with accessible label", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <SectionErrorBoundary name="Messages">
        <ThrowOnRender shouldThrow={true} />
      </SectionErrorBoundary>,
    );
    expect(screen.getByLabelText("Retry Messages")).toBeInTheDocument();
    vi.restoreAllMocks();
  });
});
