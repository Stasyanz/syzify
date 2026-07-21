// @vitest-environment happy-dom
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup, fireEvent } from "@testing-library/react";
import type { SportShare } from "../../lib/types";
import { SportDistribution } from "./SportDistribution";

afterEach(cleanup);

const share = (sport_type: string, activities: number, share_pct: number): SportShare => ({
  sport_type,
  activities,
  share_pct,
});

// Backend already capped to 5, ranked, and computed shares summing to 100.
const distribution = [
  share("ride", 5, 29),
  share("strength", 4, 23),
  share("paddle", 3, 18),
  share("run", 3, 18),
  share("open_water", 2, 12),
];

describe("SportDistribution", () => {
  it("renders one arc per sport from the backend distribution", () => {
    const { container } = render(<SportDistribution distribution={distribution} />);
    expect(container.querySelectorAll("circle").length).toBe(distribution.length);
  });

  it("shows the total activity count and the Last 7 days label", () => {
    const { getByText } = render(<SportDistribution distribution={distribution} />);
    expect(getByText("17")).toBeTruthy(); // 5+4+3+3+2
    expect(getByText("activities")).toBeTruthy();
    expect(getByText("Last 7 days")).toBeTruthy();
  });

  it("lists the sport names without percentages", () => {
    const { getByText, queryByText } = render(<SportDistribution distribution={distribution} />);
    expect(getByText("Ride")).toBeTruthy();
    expect(getByText("Strength")).toBeTruthy();
    // Shares are no longer printed in the legend.
    expect(queryByText("29%")).toBeNull();
    expect(queryByText("18%")).toBeNull();
  });

  it("reveals the hovered sport and its share only in the center", () => {
    const { container, getByText, getAllByText, queryByText } = render(
      <SportDistribution distribution={distribution} />
    );
    fireEvent.mouseEnter(container.querySelectorAll("circle")[0]);
    expect(getByText("29%")).toBeTruthy(); // center only (legend has no %)
    expect(getAllByText("Ride").length).toBe(2); // legend label + center label
    expect(queryByText("activities")).toBeNull();
  });

  it("does not react to hovering the bottom legend", () => {
    const { getByText, queryByText } = render(<SportDistribution distribution={distribution} />);
    // Hover the legend row (its label span) — the center must stay on the total.
    fireEvent.mouseEnter(getByText("Strength"));
    expect(getByText("activities")).toBeTruthy();
    expect(queryByText("23%")).toBeNull(); // no center share shown
  });

  it("shows the empty state without week activity", () => {
    const { getByText } = render(<SportDistribution distribution={[]} />);
    expect(getByText("No activities")).toBeTruthy();
  });
});
