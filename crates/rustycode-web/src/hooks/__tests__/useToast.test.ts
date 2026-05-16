import { describe, it, expect, vi, afterEach } from "vitest";
import { renderHook, act, cleanup } from "@testing-library/react";
import { useToast } from "../useToast";

afterEach(cleanup);

describe("useToast", () => {
  it("starts with empty toasts", () => {
    const { result } = renderHook(() => useToast());
    expect(result.current.toasts).toEqual([]);
  });

  it("adds a toast", () => {
    const { result } = renderHook(() => useToast());
    act(() => {
      result.current.addToast("Hello");
    });
    expect(result.current.toasts).toHaveLength(1);
    expect(result.current.toasts[0].message).toBe("Hello");
  });

  it("uses info variant by default", () => {
    const { result } = renderHook(() => useToast());
    act(() => {
      result.current.addToast("Info msg");
    });
    expect(result.current.toasts[0].variant).toBe("info");
  });

  it("uses specified variant", () => {
    const { result } = renderHook(() => useToast());
    act(() => {
      result.current.addToast("Error!", "error");
    });
    expect(result.current.toasts[0].variant).toBe("error");
  });

  it("auto-dismisses after timeout", () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useToast());
    act(() => {
      result.current.addToast("Will dismiss");
    });
    expect(result.current.toasts).toHaveLength(1);
    act(() => {
      vi.advanceTimersByTime(4000);
    });
    expect(result.current.toasts).toHaveLength(0);
    vi.useRealTimers();
  });

  it("dismisses a toast manually", () => {
    const { result } = renderHook(() => useToast());
    act(() => {
      result.current.addToast("First");
      result.current.addToast("Second");
    });
    expect(result.current.toasts).toHaveLength(2);
    act(() => {
      result.current.dismissToast(result.current.toasts[0].id);
    });
    expect(result.current.toasts).toHaveLength(1);
    expect(result.current.toasts[0].message).toBe("Second");
  });

  it("keeps max 5 toasts (slice -4)", () => {
    const { result } = renderHook(() => useToast());
    act(() => {
      for (let i = 0; i < 6; i++) {
        result.current.addToast(`Toast ${i}`);
      }
    });
    expect(result.current.toasts).toHaveLength(5);
  });

  it("cleans up timers on unmount", () => {
    vi.useFakeTimers();
    const { result, unmount } = renderHook(() => useToast());
    act(() => {
      result.current.addToast("Will be cleaned");
    });
    unmount();
    // After unmount, timers are cleared — advancing should not throw
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    vi.useRealTimers();
  });
});
