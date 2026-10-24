// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, getByText as getByTextIn, render } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import type { SportRecords } from "../../lib/types";
import { PersonalRecords } from "./PersonalRecords";
import { useLanguageStore } from "../../lib/i18n";

afterEach(() => {
  cleanup();
  useLanguageStore.getState().setPref("system");
});

const rec = (value: number) => ({ activity_id: "a1", title: "Morning", date: "2026-09-05", value });

const strength: SportRecords = {
  sport_type: "strength",
  activity_count: 3,
  distance_pbs: [],
  records: {
    longest_distance: null,
    longest_duration: rec(5000),
    highest_elevation: null,
    fastest_speed: null,
    heaviest_set: rec(22.5),
  },
};

const run: SportRecords = {
  sport_type: "run",
  activity_count: 9,
  records: { longest_distance: null, longest_duration: null, highest_elevation: null, fastest_speed: null, heaviest_set: null },
  distance_pbs: [{ label: "10K", activity_id: "a2", title: null, date: "2026-09-01", duration_s: 3000, distance_m: 10000 }],
};

const values = (container: HTMLElement) => Array.from(container.querySelectorAll(".rv"), (el) => el.textContent);

function renderRecords(records: SportRecords[]) {
  return render(
    <MemoryRouter>
      <PersonalRecords recordsBySport={records} />
    </MemoryRouter>,
  );
}

describe("PersonalRecords in the interface language (#232)", () => {
  it("writes a weight and a duration the language's way", () => {
    expect(values(renderRecords([strength]).container)).toEqual(["22.5 kg · Morning", "1h 23m · Morning"]);
    cleanup();
    useLanguageStore.getState().setPref("ru");
    expect(values(renderRecords([strength]).container)).toEqual(["22,5 кг · Morning", "1 ч 23 мин · Morning"]);
  });

  it("names the card, the sports, the records and the distance classes in Russian (#233)", () => {
    const pb = (label: string) => ({ ...run.distance_pbs[0], label });
    const races: SportRecords = {
      ...run,
      distance_pbs: [pb("Marathon"), pb("Half marathon"), pb("10 km"), pb("Sprint"), pb("Hill climb")],
    };
    useLanguageStore.getState().setPref("ru");
    const { container, getByText } = renderRecords([races, strength]);
    getByText("Личные рекорды");
    expect(Array.from(container.querySelectorAll(".chip"), (el) => el.textContent)).toEqual(["Бег", "Силовая"]);
    // A class by its name, a race distance with the language's unit, an
    // unknown label as the backend sent it.
    expect(Array.from(container.querySelectorAll(".rl"), (el) => el.textContent)).toEqual([
      "Марафон", "Полумарафон", "10 км", "Спринт", "Hill climb",
    ]);
    fireEvent.click(getByText("Силовая"));
    expect(Array.from(container.querySelectorAll(".rl"), (el) => el.textContent)).toEqual([
      "Самый тяжёлый подход", "Самая долгая тренировка",
    ]);
    cleanup();
    getByTextIn(renderRecords([]).container, "Рекордов пока нет");
  });

  it("writes a distance best's time and pace in Italian", () => {
    useLanguageStore.getState().setPref("it");
    expect(values(renderRecords([run]).container)).toEqual(["50 min · 5:00 /km"]);
  });
});
