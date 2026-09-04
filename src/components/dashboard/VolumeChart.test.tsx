// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { render, cleanup, fireEvent, act } from "@testing-library/react";
import type { VolumeBucket } from "../../lib/types";
import { VolumeChart, formatVolumeValue, weekSports } from "./VolumeChart";
import { getSportColor } from "../../lib/sportColors";

afterEach(cleanup);

describe("formatVolumeValue", () => {
  it("rounds values >= 10 to a whole number", () => {
    expect(formatVolumeValue(42.4, "km")).toBe("42 km");
    expect(formatVolumeValue(10, "km")).toBe("10 km");
  });

  it("keeps one decimal below 10", () => {
    expect(formatVolumeValue(9.43, "km")).toBe("9.4 km");
    expect(formatVolumeValue(1.5, "km")).toBe("1.5 km");
  });

  it("shows time under an hour as whole minutes", () => {
    expect(formatVolumeValue(24, "min")).toBe("24 min");
    expect(formatVolumeValue(59.4, "min")).toBe("59 min");
  });

  it("shows time of an hour or more as h+m", () => {
    expect(formatVolumeValue(85, "min")).toBe("1h 25m");
    expect(formatVolumeValue(120, "min")).toBe("2h");
    expect(formatVolumeValue(60, "min")).toBe("1h");
  });
});

function todayYmd(): string {
  const d = new Date();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${m}-${day}`;
}

describe("weekSports", () => {
  it("lists sports by EITHER metric so the legend survives the Dist/Time toggle", () => {
    const weekVolume: VolumeBucket[] = [
      {
        label: "",
        start_date: todayYmd(),
        distance_m: 9400,
        duration_s: 5400,
        activities: 2,
        by_sport: {
          ride: { distance_m: 9400, duration_s: 3600, activities: 1 },
          // Distance-less strength: absent from the Dist bars, but it MUST
          // stay in the legend or the card height jumps on toggle.
          strength: { distance_m: 0, duration_s: 1800, activities: 1 },
          // Fully-zero row (degenerate import): not worth a legend entry.
          other: { distance_m: 0, duration_s: 0, activities: 0 },
        },
      },
    ];
    expect(weekSports(weekVolume)).toEqual(["ride", "strength"]);
    expect(weekSports([])).toEqual([]);
  });
});

describe("VolumeChart hover value", () => {
  it("shows the hovered segment's value in that segment's color", () => {
    const weekVolume: VolumeBucket[] = [
      {
        label: "",
        start_date: todayYmd(),
        distance_m: 9400,
        duration_s: 3600,
        activities: 1,
        by_sport: { ride: { distance_m: 9400, duration_s: 3600, activities: 1 } },
      },
    ];

    const { container } = render(<VolumeChart weekVolume={weekVolume} />);

    // The label slot is always present (reserved to avoid layout shift) but
    // shows no value until a segment is hovered.
    const labelText = () =>
      [...container.querySelectorAll(".vval")]
        .map((b) => b.textContent?.trim())
        .filter(Boolean);
    expect(labelText()).toEqual([]);

    const seg = container.querySelector(".vbar span") as HTMLElement;
    expect(seg).not.toBeNull();
    fireEvent.mouseEnter(seg);

    expect(labelText()).toEqual(["9.4 km"]); // 9400 m → 9.4 km (distance is the default metric)
    const shown = [...container.querySelectorAll<HTMLElement>(".vval")].find(
      (b) => b.textContent?.trim(),
    )!;
    expect(shown.style.color).toBe(getSportColor("ride"));

    // Leaving the chart clears the value again.
    fireEvent.mouseLeave(container.querySelector(".bars") as HTMLElement);
    expect(labelText()).toEqual([]);
  });
});

describe("VolumeChart day rollover", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 8, 3, 23, 59, 30)); // a Thursday
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });
  const dayLabels = (c: HTMLElement) =>
    [...c.querySelectorAll(".bars > div > span:last-child")].map((s) => s.textContent);
  const short = (d: Date) => d.toLocaleDateString(undefined, { weekday: "short" });

  it("slides the 7-day window at midnight", () => {
    const { container } = render(<VolumeChart weekVolume={[]} />);
    const labels = dayLabels(container);
    expect(labels).toHaveLength(7);
    expect(labels[6]).toBe(short(new Date(2026, 8, 3)));

    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    const after = dayLabels(container);
    expect(after[6]).toBe(short(new Date(2026, 8, 4)));
    expect(after[5]).toBe(labels[6]);
  });
});
