import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AudioBars } from "./AudioBars";

function centerBar(container: HTMLElement): HTMLElement {
  const bars = container.querySelectorAll<HTMLElement>('[data-testid="audio-bars"] > span');
  return bars[Math.floor(bars.length / 2)];
}

describe("AudioBars", () => {
  it("waves while recording and grows the wave with the audio level", () => {
    const quiet = centerBar(render(<AudioBars state="listening" audioLevel={0} />).container);
    const loud = centerBar(render(<AudioBars state="listening" audioLevel={1} />).container);

    // A traveling wave (shared keyframe + per-bar delay), not a static volume scale.
    expect(loud.className).toContain("animate-pill-wave");
    expect(loud.style.animationDelay).not.toBe("");

    const quietPeak = Number(quiet.style.getPropertyValue("--wave-max"));
    const loudPeak = Number(loud.style.getPropertyValue("--wave-max"));
    expect(quietPeak).toBeLessThan(0.25); // small "dot" at rest
    expect(loudPeak).toBeGreaterThan(0.9); // tall wave when loud
    expect(loudPeak).toBeGreaterThan(quietPeak);
  });

  it("exposes the backend phase via data-state", () => {
    for (const state of ["idle", "listening", "transcribing", "formatting"] as const) {
      const { container, unmount } = render(<AudioBars state={state} />);
      expect(container.querySelector('[data-testid="audio-bars"]')).toHaveAttribute("data-state", state);
      unmount();
    }
  });

  it("differentiates the three active phases", () => {
    const transcribing = centerBar(render(<AudioBars state="transcribing" />).container);
    const formatting = centerBar(render(<AudioBars state="formatting" />).container);

    // both sweep left -> right (staggered delay), but AI formatting is taller
    expect(transcribing.className).toContain("animate-pill-wave");
    expect(transcribing.style.animationDelay).not.toBe("");
    expect(formatting.className).toContain("animate-pill-wave");
    expect(formatting.style.animationDelay).not.toBe("");

    const transcribePeak = Number(transcribing.style.getPropertyValue("--wave-max"));
    const formatPeak = Number(formatting.style.getPropertyValue("--wave-max"));
    expect(formatPeak).toBeGreaterThan(transcribePeak);
  });
});
