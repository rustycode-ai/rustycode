import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SectionErrorBoundary } from "../SectionErrorBoundary";

afterEach(cleanup);

function ThrowingChild(): never {
  throw new Error("Test render error");
}

function GoodChild(): JSX.Element {
  return <p>Child content</p>;
}

describe("SectionErrorBoundary", () => {
  // Suppress console.error from React error boundary in test output
  const spy = vi.spyOn(console, "error").mockImplementation(() => {});

  afterEach(() => {
    spy.mockClear();
  });

  it("renders children when no error", () => {
    render(
      <SectionErrorBoundary name="TestSection">
        <GoodChild />
      </SectionErrorBoundary>
    );
    expect(screen.getByText("Child content")).toBeInTheDocument();
  });

  it("catches errors and shows error message", () => {
    render(
      <SectionErrorBoundary name="TestSection">
        <ThrowingChild />
      </SectionErrorBoundary>
    );
    expect(screen.getByText(/TestSection encountered an error/)).toBeInTheDocument();
    expect(screen.getByText(/Test render error/)).toBeInTheDocument();
  });

  it("shows Retry button with aria-label", () => {
    render(
      <SectionErrorBoundary name="MySection">
        <ThrowingChild />
      </SectionErrorBoundary>
    );
    expect(screen.getByLabelText("Retry MySection")).toBeInTheDocument();
  });

  it("has role=alert on error", () => {
    render(
      <SectionErrorBoundary name="TestSection">
        <ThrowingChild />
      </SectionErrorBoundary>
    );
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  it("recovers when Retry is clicked", async () => {
    let shouldThrow = true;
    function ConditionalChild() {
      if (shouldThrow) throw new Error("Boom");
      return <p>Recovered</p>;
    }

    render(
      <SectionErrorBoundary name="TestSection">
        <ConditionalChild />
      </SectionErrorBoundary>
    );
    expect(screen.getByRole("alert")).toBeInTheDocument();

    shouldThrow = false;
    await userEvent.click(screen.getByLabelText("Retry TestSection"));
    expect(screen.getByText("Recovered")).toBeInTheDocument();
  });

  it("includes section name in error message", () => {
    render(
      <SectionErrorBoundary name="PlanView">
        <ThrowingChild />
      </SectionErrorBoundary>
    );
    expect(screen.getByText(/PlanView encountered an error/)).toBeInTheDocument();
  });
});
