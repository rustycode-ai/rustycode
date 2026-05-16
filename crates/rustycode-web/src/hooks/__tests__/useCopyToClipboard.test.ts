import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useCopyToClipboard } from "../useCopyToClipboard";

describe("useCopyToClipboard", () => {
  beforeEach(() => {
    vi.stubGlobal("navigator", {
      clipboard: {
        writeText: vi.fn().mockResolvedValue(undefined),
      },
    });
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("sets copied to true then resets after delay", async () => {
    const { result } = renderHook(() => useCopyToClipboard(2000));
    expect(result.current.copied).toBe(false);

    await act(async () => {
      result.current.copy("hello");
    });

    expect(result.current.copied).toBe(true);

    act(() => {
      vi.advanceTimersByTime(2000);
    });

    expect(result.current.copied).toBe(false);
  });

  it("sets copied to false on clipboard rejection", async () => {
    vi.stubGlobal("navigator", {
      clipboard: {
        writeText: vi.fn().mockRejectedValue(new Error("denied")),
      },
    });

    const { result } = renderHook(() => useCopyToClipboard());

    await act(async () => {
      result.current.copy("hello");
    });

    expect(result.current.copied).toBe(false);
  });

  it("sets copied to false when clipboard is unavailable", async () => {
    vi.stubGlobal("navigator", {});

    const { result } = renderHook(() => useCopyToClipboard());

    await act(async () => {
      result.current.copy("hello");
    });

    expect(result.current.copied).toBe(false);
  });

  it("clears timeout on unmount", async () => {
    const { result, unmount } = renderHook(() => useCopyToClipboard(2000));

    await act(async () => {
      result.current.copy("hello");
    });

    expect(result.current.copied).toBe(true);
    unmount();

    act(() => {
      vi.advanceTimersByTime(2000);
    });

    // No error should be thrown for setting state on unmounted component
  });
});
