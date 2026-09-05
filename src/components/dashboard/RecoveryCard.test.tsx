// @vitest-environment happy-dom
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, cleanup, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { RecoveryCard as Card } from "../../lib/types";

vi.mock("../../lib/tauri", () => ({
  api: { getRecovery: vi.fn() },
}));

import { RecoveryCard } from "./RecoveryCard";
import { api } from "../../lib/tauri";

// A FIXED past day: the card must build everything from the backend's
// computed_for, and with today's date in the fixture a component reading
// the frontend clock would pass by accident (until tomorrow).
const full: Card = {
  computed_for: "2026-06-10",
  date: "2026-06-10",
  age_days: 0,
  index: 98,
  band: "intervals_ok",
  advice: "Intervals are fine today",
  hr: { night_median: 53, baseline: 53, delta: 0, score: 100 },
  stress: { night_avg: 12.4, score: 94 },
  load: { tss_yesterday: 30.4, ctl: 59.6, score: 100 },
  warning: null,
  days_recorded_90d: 9,
  nights_recorded_90d: 8,
  nights_needed: 0,
  history: [
    { date: "2026-06-09", index: 90, band: "intervals_ok" },
    { date: "2026-06-10", index: 55, band: "rest" },
  ],
};

function renderCard() {
  // Mirror the app's QueryClient (App.tsx sets staleTime: 30_000).
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 30_000 } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <RecoveryCard />
    </QueryClientProvider>,
  );
}

afterEach(cleanup);

describe("RecoveryCard", () => {
  it("shows the index, band, components and a sparse sparkline", async () => {
    vi.mocked(api.getRecovery).mockResolvedValue(full);
    const { getByText, getByTestId, container } = renderCard();
    await waitFor(() => getByTestId("recovery-index"));
    expect(getByTestId("recovery-index").textContent).toBe("98");
    getByText("Intervals OK");
    getByText("Intervals are fine today");
    getByText("Last night");
    getByText("base 53 · 0");
    getByText("score 94");
    getByText("CTL 60");
    getByText("8 of the last 90 nights recorded");
    // Two nights → two points joined by one segment (plus the two guides).
    const circles = container.querySelectorAll("circle");
    expect(circles).toHaveLength(2);
    expect(container.querySelectorAll("svg line")).toHaveLength(3);
    // Point colors come from the backend's band, not a frontend re-derivation.
    expect(circles[1].getAttribute("fill")).toBe("var(--danger)");
    expect(container.textContent).not.toContain("not recorded");
  });

  it("labels a hovered point with its night and index", async () => {
    vi.mocked(api.getRecovery).mockResolvedValue(full);
    const { container, queryByTestId, getByTestId } = renderCard();
    await waitFor(() => getByTestId("recovery-index"));
    expect(queryByTestId("spark-tip")).toBeNull();
    const circles = container.querySelectorAll("circle");
    fireEvent.mouseEnter(circles[1]);
    expect(getByTestId("spark-tip").textContent).toBe("Jun 10 · 55");
    fireEvent.mouseLeave(circles[1]);
    expect(queryByTestId("spark-tip")).toBeNull();
  });

  it("keeps an old index on screen and says last night is missing", async () => {
    vi.mocked(api.getRecovery).mockResolvedValue({
      ...full,
      date: "2026-04-29",
      age_days: 42,
      index: 82,
      history: [],
    });
    const { getByText, container, getByTestId } = renderCard();
    await waitFor(() => getByText("Apr 29 · 42 days ago"));
    getByText("Last night not recorded");
    expect(container.querySelector("svg")).toBeNull();
    getByTestId("spark-empty");
  });

  it("shows dashes when a night has no stress and no training history", async () => {
    vi.mocked(api.getRecovery).mockResolvedValue({
      ...full,
      index: 100,
      stress: null,
      load: null,
    });
    const { getByText, getAllByText } = renderCard();
    await waitFor(() => getByText("no stress data"));
    getByText("no training history");
    expect(getAllByText("—")).toHaveLength(2);
    expect(document.body.textContent).not.toContain("TSS");
  });

  it("warns when the night HR ran well above the baseline", async () => {
    vi.mocked(api.getRecovery).mockResolvedValue({
      ...full,
      index: 55,
      band: "rest",
      hr: { night_median: 62, baseline: 53, delta: 9, score: 28 },
      warning: "hr_above_baseline",
    });
    const { getByText } = renderCard();
    await waitFor(() => getByText("Rest"));
    getByText("Night HR well above baseline (+9 bpm)");
    getByText("base 53 · +9");
  });

  it("renders the empty states", async () => {
    vi.mocked(api.getRecovery).mockResolvedValue({
      ...full,
      date: null,
      age_days: null,
      index: null,
      band: null,
      advice: null,
      hr: null,
      stress: null,
      load: null,
      nights_recorded_90d: 1,
      nights_needed: 2,
      history: [],
    });
    const { getByText, unmount } = renderCard();
    await waitFor(() => getByText("2 more nights to build the baseline"));
    getByText("1 of the last 90 nights recorded · the baseline needs 3 nights in 90 days");
    unmount();

    const blank = {
      ...full,
      date: null,
      age_days: null,
      index: null,
      band: null,
      days_recorded_90d: 0,
      nights_recorded_90d: 0,
      nights_needed: 3,
      history: [],
    };
    vi.mocked(api.getRecovery).mockResolvedValue(blank);
    const second = renderCard();
    await waitFor(() => second.getByText("No sleep data yet"));
    second.getByText(/Monitor folder/);
    second.unmount();

    // Worn by day only: monitoring exists, no night — do not ask for an import.
    vi.mocked(api.getRecovery).mockResolvedValue({ ...blank, days_recorded_90d: 5 });
    const third = renderCard();
    await waitFor(() => third.getByText("No full night recorded yet"));
    third.getByText(/5 days of monitoring in the last 90/);
    expect(third.container.textContent).not.toContain("Monitor folder");
  });

  it("says so when the backend fails, without throwing", async () => {
    vi.mocked(api.getRecovery).mockRejectedValue(new Error("boom"));
    const { getByText } = renderCard();
    await waitFor(() => getByText("Recovery unavailable"));
  });
});
