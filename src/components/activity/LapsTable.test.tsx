// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import type { Lap, TrackPointColumns } from "../../lib/types";
import { LapsTable } from "./LapsTable";
import { useLanguageStore } from "../../lib/i18n";
import { useUnitsStore } from "../../lib/units";

afterEach(() => {
  cleanup();
  useLanguageStore.getState().setPref("system");
  useUnitsStore.setState({ mode: "metric" });
});

const lap = (n: number, over: Partial<Lap> = {}): Lap =>
  ({
    lap_number: n,
    total_timer_time_s: 600,
    total_elapsed_time_s: 600,
    total_distance_m: 5000,
    avg_speed_mps: 8.5,
    avg_hr: 150,
    max_hr: 170,
    total_ascent_m: 40,
    avg_cadence: 88.4,
    avg_power_w: 212.6,
    ...over,
  }) as Lap;

/** A steady track `km` long, a point every 500 m. */
function track(km: number): TrackPointColumns {
  const n = km * 2 + 1;
  const col = (f: (i: number) => number | null) => Array.from({ length: n }, (_, i) => f(i));
  return {
    distance_m: col((i) => i * 500),
    t: col((i) => i * 60),
    altitude_m: col(() => null),
    hr: col(() => null),
  } as unknown as TrackPointColumns;
}

const cells = (container: HTMLElement) =>
  Array.from(container.querySelectorAll("tbody tr"), (tr) => Array.from(tr.children, (td) => td.textContent));

describe("LapsTable in the interface language (#232)", () => {
  it("writes cadence and power with their units, a dash where a lap has none", () => {
    const { container } = render(
      <LapsTable laps={[lap(1), lap(2, { avg_cadence: null, avg_power_w: null })]} trackpoints={track(0)} sport="ride" />,
    );
    const rows = cells(container);
    expect(rows[0].slice(-2)).toEqual(["88 rpm", "213 W"]);
    expect(rows[1].slice(-2)).toEqual(["--", "--"]);
  });

  it("counts a run's cadence in steps", () => {
    const { container } = render(<LapsTable laps={[lap(1), lap(2)]} trackpoints={track(0)} sport="run" />);
    expect(cells(container)[0].slice(-2)).toEqual(["88 spm", "213 W"]);
  });

  it("counts a swim's cadence in strokes", () => {
    const en = render(<LapsTable laps={[lap(1), lap(2)]} trackpoints={track(0)} sport="swim" />);
    expect(cells(en.container)[0].slice(-2)).toEqual(["88 spm", "213 W"]);
    cleanup();
    useLanguageStore.getState().setPref("ru");
    const ru = render(<LapsTable laps={[lap(1), lap(2)]} trackpoints={track(0)} sport="swim" />);
    expect(cells(ru.container)[0].slice(-2)).toEqual(["88 гр/мин", "213 Вт"]);
  });

  it("writes them in Russian", () => {
    useLanguageStore.getState().setPref("ru");
    const { container } = render(<LapsTable laps={[lap(1), lap(2)]} trackpoints={track(0)} sport="ride" />);
    expect(cells(container)[0]).toEqual(["1", "10:00", "5,00 км", "30,6 км/ч", "150 уд/мин", "170 уд/мин", "40 м", "88 об/мин", "213 Вт"]);
  });

  it("names the auto-split length in the language and the units", () => {
    useLanguageStore.getState().setPref("ru");
    const ride = render(<LapsTable laps={[]} trackpoints={track(12)} sport="ride" />);
    expect(ride.container.querySelector("h3")!.textContent).toBe("Laps· auto · 5 км");
    cleanup();
    const swim = render(<LapsTable laps={[]} trackpoints={track(1)} sport="swim" />);
    expect(swim.container.querySelector("h3")!.textContent).toBe("Laps· auto · 100 м");
    cleanup();
    useUnitsStore.setState({ mode: "imperial" });
    const run = render(<LapsTable laps={[]} trackpoints={track(4)} sport="run" />);
    expect(run.container.querySelector("h3")!.textContent).toBe("Laps· auto · 1 ми");
    cleanup();
    const mtb = render(<LapsTable laps={[]} trackpoints={track(20)} sport="mountain_bike" />);
    expect(mtb.container.querySelector("h3")!.textContent).toBe("Laps· auto · 5 ми");
  });
});
