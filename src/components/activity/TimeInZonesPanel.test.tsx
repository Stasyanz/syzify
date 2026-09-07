// @vitest-environment happy-dom
import { describe, it, expect, afterEach } from "vitest";
import { render, cleanup, fireEvent, screen } from "@testing-library/react";
import type { TimeInZone } from "../../lib/types";
import { HR_ZONE_COLORS, POWER_ZONE_COLORS } from "./chartZones";
import { TimeInZonesPanel } from "./TimeInZonesPanel";

afterEach(cleanup);

function row(zone_type: string, zone_index: number, time_s: number, hi: number | null): TimeInZone {
  return { id: null, activity_id: "a", zone_type, zone_index, time_s, zone_high_boundary: hi };
}

const HR: TimeInZone[] = [
  row("hr", 0, 19, 93),
  row("hr", 1, 2674, 112),
  row("hr", 2, 4602, 130),
  row("hr", 3, 1357, 149),
  row("hr", 4, 0, 167),
  row("hr", 5, 0, 186),
  row("hr", 6, 0, null),
];
const POWER: TimeInZone[] = [
  row("power", 0, 0, 0),
  row("power", 1, 2661, 129),
  row("power", 2, 2898, 176),
  row("power", 3, 1799, 212),
  row("power", 4, 825, 247),
  row("power", 5, 326, 282),
  row("power", 6, 135, 353),
  row("power", 7, 8, 3393),
];
const DURATION = 8652;

const lists = (c: HTMLElement) => [...c.querySelectorAll("ul")];
const switchButtons = () => screen.queryAllByRole("button");

/** Rows of the VISIBLE list — the other type's list stays mounted, hidden. */
function rowsOf(container: HTMLElement) {
  const list = container.querySelector('ul:not([aria-hidden="true"])')!;
  return [...list.querySelectorAll("li")].map((r) => ({
    zone: r.getAttribute("data-zone"),
    label: r.getAttribute("aria-label") ?? "",
    text: r.textContent ?? "",
    bar: (r.querySelector('[data-testid="zone-bar"]') as HTMLElement).style,
  }));
}

describe("TimeInZonesPanel", () => {
  it("lists the zones top-down with range, time and share, colored by index", () => {
    const { container } = render(<TimeInZonesPanel timeInZones={HR} durationS={DURATION} />);
    expect(screen.getByText("Time in Zones")).toBeTruthy();
    const rows = rowsOf(container);
    expect(rows.map((r) => r.zone)).toEqual(["5", "4", "3", "2", "1"]);
    expect(rows[3].text).toContain("Z2 Easy");
    expect(rows[3].text).toContain("112–130 bpm");
    expect(rows[3].text).toContain("1:16:42");
    expect(rows[3].text).toContain("53%");
    // The whole row is one spoken item; the visual cells are hidden from AT.
    expect(rows[3].label).toBe("Z2 Easy, 112–130 bpm, 1:16:42, 53%");
    expect(rows[3].bar.width).toBe(`${(4602 / DURATION) * 100}%`);
    expect(rows[3].bar.background).toBe(HR_ZONE_COLORS[1]);
    // Zero-time zones keep their row with an empty track.
    expect(rows[0].text).toContain("Z5 Maximum");
    expect(rows[0].bar.width).toBe("0%");
    // One type: no switch.
    expect(screen.queryByRole("group")).toBeNull();
    expect(switchButtons()).toHaveLength(0);
  });

  it("offers a switch when both types exist, power first, and switches on click", () => {
    const { container } = render(
      <TimeInZonesPanel timeInZones={[...HR, ...POWER]} durationS={DURATION} />,
    );
    expect(screen.getByRole("group").getAttribute("aria-label")).toBe("Zone type");
    const buttons = switchButtons();
    expect(buttons.map((b) => b.textContent)).toEqual(["Power", "Heart rate"]);
    expect(buttons[0].getAttribute("aria-pressed")).toBe("true");
    expect(buttons[1].getAttribute("aria-pressed")).toBe("false");
    let rows = rowsOf(container);
    expect(rows).toHaveLength(7);
    expect(rows[0].text).toContain("Z7 Neuromuscular");
    expect(rows[0].text).toContain("> 353 W");
    expect(rows[6].bar.background).toBe(POWER_ZONE_COLORS[0]);
    expect(screen.getByRole("list").getAttribute("aria-label")).toBe("Time in power zones");

    fireEvent.click(buttons[1]);
    expect(buttons[1].getAttribute("aria-pressed")).toBe("true");
    expect(buttons[0].getAttribute("aria-pressed")).toBe("false");
    rows = rowsOf(container);
    expect(rows).toHaveLength(5);
    expect(rows[4].text).toContain("Z1 Warm up");
    expect(screen.getByRole("list").getAttribute("aria-label")).toBe("Time in heart rate zones");
    // Clicking the pressed button changes nothing.
    fireEvent.click(buttons[1]);
    expect(rowsOf(container)).toHaveLength(5);
  });

  it("writes <1% for a sliver instead of a bar next to 0%", () => {
    const { container } = render(
      <TimeInZonesPanel timeInZones={POWER} durationS={DURATION} />,
    );
    const z7 = rowsOf(container)[0];
    expect(z7.text).toContain("0:08");
    expect(z7.text).toContain("<1%");
    expect(z7.label).toBe("Z7 Neuromuscular, > 353 W, 0:08, <1%");
    expect(rowsOf(container)[6].text).toContain("31%");
  });

  it("keeps the other list mounted but hidden so the card height does not jump", () => {
    const { container } = render(
      <TimeInZonesPanel timeInZones={[...HR, ...POWER]} durationS={DURATION} />,
    );
    expect(lists(container)).toHaveLength(2);
    // Both share one grid cell; the hidden one is invisible, not display:none.
    for (const t of lists(container)) {
      expect(t.className).toContain("col-start-1 row-start-1");
      expect(t.className).toContain("content-between");
    }
    expect(lists(container)[1].getAttribute("aria-hidden")).toBe("true");
    expect(lists(container)[1].className).toContain("invisible");
    expect(lists(container)[0].className).not.toContain("invisible");
    fireEvent.click(switchButtons()[1]);
    expect(lists(container)[0].getAttribute("aria-hidden")).toBe("true");
    expect(lists(container)[0].className).toContain("invisible");
    expect(lists(container)[1].className).not.toContain("invisible");
    // A single type mounts a single list.
    cleanup();
    const single = render(<TimeInZonesPanel timeInZones={HR} durationS={DURATION} />);
    expect(lists(single.container)).toHaveLength(1);
  });

  it("falls back to the type that exists when the picked one is gone", () => {
    const { container, rerender } = render(
      <TimeInZonesPanel timeInZones={[...HR, ...POWER]} durationS={DURATION} />,
    );
    fireEvent.click(switchButtons()[1]);
    expect(rowsOf(container)).toHaveLength(5);
    // The same card refreshed without HR: only power is left.
    rerender(<TimeInZonesPanel timeInZones={POWER} durationS={DURATION} />);
    expect(screen.queryByRole("group")).toBeNull();
    expect(rowsOf(container)).toHaveLength(7);
  });

  it("renders nothing without device zone data", () => {
    const { container } = render(
      <TimeInZonesPanel
        timeInZones={[row("hr", 0, 40, 93), row("hr", 6, 0, null)]}
        durationS={DURATION}
      />,
    );
    expect(container.innerHTML).toBe("");
  });

  it("clamps the bar to the track and passes the class through", () => {
    // A duration shorter than the zone time cannot happen after the
    // corrupt-row guard, but a bar must never overflow its track anyway.
    const { container } = render(
      <TimeInZonesPanel
        timeInZones={[row("hr", 1, 100, 112), row("hr", 6, 0, null)]}
        durationS={null}
        className="col-span-2"
      />,
    );
    expect(container.firstElementChild!.className).toContain("col-span-2");
    const z1 = rowsOf(container)[4];
    expect(z1.bar.width).toBe("0%");
    expect(z1.text).toContain("0%");
    expect(z1.label).toBe("Z1 Warm up, ≤ 112 bpm, 1:40, 0%");
  });
});
