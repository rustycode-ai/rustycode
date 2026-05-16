import { describe, it, expect } from "vitest";
import { classifyError, classifyWsClose } from "../ws-client";

describe("classifyError", () => {
  it("classifies unauthorized as auth error", () => {
    const result = classifyError("unauthorized", "bad token");
    expect(result.class).toBe("auth");
    expect(result.retryable).toBe(false);
  });

  it("classifies session_expired as auth error", () => {
    const result = classifyError("session_expired", "expired");
    expect(result.class).toBe("auth");
    expect(result.retryable).toBe(false);
  });

  it("classifies rate_limited as retryable", () => {
    const result = classifyError("rate_limited", "slow down");
    expect(result.class).toBe("rate_limit");
    expect(result.retryable).toBe(true);
  });

  it("classifies session_not_found as session error", () => {
    const result = classifyError("session_not_found", "no session");
    expect(result.class).toBe("session");
    expect(result.retryable).toBe(false);
  });

  it("classifies unknown codes as server error", () => {
    const result = classifyError("internal_error", "something broke");
    expect(result.class).toBe("server");
    expect(result.retryable).toBe(true);
  });
});

describe("classifyWsClose", () => {
  it("classifies 4001 as auth error", () => {
    const result = classifyWsClose(4001, "unauthorized");
    expect(result.class).toBe("auth");
    expect(result.retryable).toBe(false);
  });

  it("classifies 4003 as auth error", () => {
    const result = classifyWsClose(4003, "forbidden");
    expect(result.class).toBe("auth");
  });

  it("classifies 4008 as rate_limit", () => {
    const result = classifyWsClose(4008, "too many requests");
    expect(result.class).toBe("rate_limit");
    expect(result.retryable).toBe(true);
  });

  it("classifies 429 as rate_limit", () => {
    const result = classifyWsClose(429, "rate limited");
    expect(result.class).toBe("rate_limit");
  });

  it("classifies 4500+ as server error", () => {
    const result = classifyWsClose(4501, "internal");
    expect(result.class).toBe("server");
    expect(result.retryable).toBe(true);
  });

  it("classifies normal close as connection error", () => {
    const result = classifyWsClose(1000, "normal closure");
    expect(result.class).toBe("connection");
    expect(result.retryable).toBe(true);
  });

  it("uses default message when reason is empty", () => {
    const result = classifyWsClose(1006, "");
    expect(result.message).toContain("1006");
  });
});
