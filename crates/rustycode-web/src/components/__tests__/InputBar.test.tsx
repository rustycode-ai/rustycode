import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { InputBar } from "../InputBar";

afterEach(cleanup);

function renderInputBar(overrides: Partial<Parameters<typeof InputBar>[0]> = {}) {
  const onSend = vi.fn();
  const onAbort = vi.fn();
  const result = render(
    <InputBar onSend={onSend} onAbort={onAbort} pending={false} {...overrides} />,
  );
  return { ...result, onSend, onAbort };
}

describe("InputBar", () => {
  it("renders textarea with placeholder", () => {
    renderInputBar();
    const textarea = screen.getByRole("textbox");
    expect(textarea).toBeInTheDocument();
    expect(textarea).toHaveAttribute("placeholder", expect.stringContaining("Message RustyCode"));
  });

  it("shows pending placeholder when pending", () => {
    renderInputBar({ pending: true });
    const textarea = screen.getByRole("textbox");
    expect(textarea).toHaveAttribute("placeholder", expect.stringContaining("type ahead"));
  });

  it("calls onSend on Enter with trimmed value", async () => {
    const { onSend } = renderInputBar();
    const textarea = screen.getByRole("textbox");

    await userEvent.type(textarea, "hello world{Enter}");
    expect(onSend).toHaveBeenCalledWith("hello world");
  });

  it("does not call onSend on Shift+Enter", async () => {
    const { onSend } = renderInputBar();
    const textarea = screen.getByRole("textbox");

    await userEvent.type(textarea, "hello");
    fireEvent.keyDown(textarea, { key: "Enter", shiftKey: true });
    expect(onSend).not.toHaveBeenCalled();
  });

  it("does not send empty or whitespace-only input", async () => {
    const { onSend } = renderInputBar();
    const textarea = screen.getByRole("textbox");

    await userEvent.type(textarea, "   {Enter}");
    expect(onSend).not.toHaveBeenCalled();
  });

  it("does not send when pending", async () => {
    const { onSend } = renderInputBar({ pending: true });
    const textarea = screen.getByRole("textbox");

    await userEvent.type(textarea, "hello{Enter}");
    expect(onSend).not.toHaveBeenCalled();
  });

  it("clears input after sending", async () => {
    renderInputBar();
    const textarea = screen.getByRole("textbox");

    await userEvent.type(textarea, "hello{Enter}");
    expect(textarea).toHaveValue("");
  });

  it("calls onAbort on Escape when pending", () => {
    const { onAbort } = renderInputBar({ pending: true });
    const textarea = screen.getByRole("textbox");

    fireEvent.keyDown(textarea, { key: "Escape" });
    expect(onAbort).toHaveBeenCalled();
  });

  it("does not call onAbort on Escape when not pending", () => {
    const { onAbort } = renderInputBar({ pending: false });
    const textarea = screen.getByRole("textbox");

    fireEvent.keyDown(textarea, { key: "Escape" });
    expect(onAbort).not.toHaveBeenCalled();
  });

  it("sends a message via onChange + Enter", () => {
    const { onSend } = renderInputBar();
    const textarea = screen.getByRole("textbox");

    fireEvent.change(textarea, { target: { value: "hello" } });
    fireEvent.keyDown(textarea, { key: "Enter" });

    expect(onSend).toHaveBeenCalledWith("hello");
  });

  it("shows character count when input has content", async () => {
    renderInputBar();
    const textarea = screen.getByRole("textbox");

    expect(screen.queryByText("5")).not.toBeInTheDocument();

    await userEvent.type(textarea, "hello");
    expect(screen.getByText("5")).toBeInTheDocument();
  });

  it("shows Stop button when pending", () => {
    renderInputBar({ pending: true });
    expect(screen.getByLabelText("Stop generation")).toBeInTheDocument();
  });

  it("shows Send button when not pending with content", async () => {
    renderInputBar();
    const textarea = screen.getByRole("textbox");

    expect(screen.getByLabelText("Send message")).toBeInTheDocument();
    expect(screen.getByLabelText("Send message")).toBeDisabled();

    await userEvent.type(textarea, "hello");
    expect(screen.getByLabelText("Send message")).not.toBeDisabled();
  });
});
