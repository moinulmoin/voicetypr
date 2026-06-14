import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FeedbackToast } from "./FeedbackToast";
import { emitMockEvent } from "@/test/setup";

describe("FeedbackToast", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    act(() => {
      vi.runOnlyPendingTimers();
    });
    vi.useRealTimers();
  });

  it("shows transient info toast and auto-clears after duration", () => {
    render(<FeedbackToast />);

    act(() => {
      emitMockEvent("toast", {
        id: 1,
        action: "show",
        message: "Saved",
        duration_ms: 1500,
        variant: "info",
        persistent: false,
      });
    });

    expect(screen.getByRole("status")).toHaveTextContent("Saved");
    expect(screen.getByRole("status")).toHaveClass("bg-black");

    act(() => {
      vi.advanceTimersByTime(1500);
    });

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("shows persistent warning and does not auto-clear", () => {
    render(<FeedbackToast />);

    act(() => {
      emitMockEvent("toast", {
        id: 2,
        action: "show",
        message: "Long silence detected",
        duration_ms: 1500,
        variant: "warning",
        persistent: true,
      });
    });

    expect(screen.getByRole("status")).toHaveTextContent("Long silence detected");

    act(() => {
      vi.advanceTimersByTime(60_000);
    });

    expect(screen.getByRole("status")).toHaveTextContent("Long silence detected");
  });

  it("clear payload hides current toast", () => {
    render(<FeedbackToast />);

    act(() => {
      emitMockEvent("toast", {
        id: 3,
        action: "show",
        message: "Mic check",
        duration_ms: 0,
        variant: "warning",
        persistent: true,
      });
    });

    expect(screen.getByRole("status")).toHaveTextContent("Mic check");

    act(() => {
      emitMockEvent("toast", {
        id: 3,
        action: "clear",
        message: "",
        duration_ms: 0,
        variant: "info",
        persistent: false,
      });
    });

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("stale transient timer does not clear newer persistent warning", () => {
    render(<FeedbackToast />);

    act(() => {
      emitMockEvent("toast", {
        id: 10,
        action: "show",
        message: "Brief info",
        duration_ms: 2000,
        variant: "info",
        persistent: false,
      });
    });

    expect(screen.getByRole("status")).toHaveTextContent("Brief info");

    act(() => {
      emitMockEvent("toast", {
        id: 11,
        action: "show",
        message: "No audio detected",
        duration_ms: 2000,
        variant: "warning",
        persistent: true,
      });
    });

    expect(screen.getByRole("status")).toHaveTextContent("No audio detected");

    act(() => {
      vi.advanceTimersByTime(2000);
    });

    expect(screen.getByRole("status")).toHaveTextContent("No audio detected");
  });

  it("warning variant renders TriangleAlert and warning styles", () => {
    render(<FeedbackToast />);

    act(() => {
      emitMockEvent("toast", {
        id: 4,
        action: "show",
        message: "Check microphone",
        duration_ms: 0,
        variant: "warning",
        persistent: true,
      });
    });

    const status = screen.getByRole("status");
    expect(status).toHaveClass("bg-amber-950");
    expect(status).toHaveTextContent("Check microphone");

    const alertIcon = status.querySelector("svg.text-amber-300");
    expect(alertIcon).toBeInTheDocument();
  });

  it("defaults missing payload fields for legacy info toasts", () => {
    render(<FeedbackToast />);

    act(() => {
      emitMockEvent("toast", {
        id: 5,
        message: "Legacy toast",
        duration_ms: 1000,
      });
    });

    const status = screen.getByRole("status");
    expect(status).toHaveTextContent("Legacy toast");
    expect(status).toHaveClass("bg-black");

    act(() => {
      vi.advanceTimersByTime(1000);
    });

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});