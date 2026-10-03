// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Activity, ActivityDetail, MultisportLeg, TrackPointColumns } from "../lib/types";

vi.mock("../lib/tauri", () => ({
  api: {
    getActivityDetail: vi.fn(),
    getAdjacentActivities: vi.fn(),
    listGear: vi.fn(),
    getActivityRecordBadges: vi.fn(),
    exportActivityGpx: vi.fn(),
    unmergeTriathlon: vi.fn(),
    updateActivity: vi.fn(),
  },
  isTauri: () => false,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));
vi.mock("../lib/activityInvalidation", () => ({ invalidateActivityData: vi.fn() }));
// The page's panels each have tests of their own; here only the page's own
// handlers matter, so the heavy children render nothing.
vi.mock("../components/activity/RouteMap", () => ({ RouteMap: () => null }));
vi.mock("../components/activity/ChartPanel", () => ({ ChartPanel: () => null }));
vi.mock("../components/activity/SegmentEffortsPanel", () => ({ SegmentEffortsPanel: () => null }));
vi.mock("../components/activity/PowerCurvePanel", () => ({ PowerCurvePanel: () => null }));
vi.mock("../components/activity/DynamicsZonesRow", () => ({ DynamicsZonesRow: () => null, zonesFiller: () => null }));
vi.mock("../components/activity/MultisportLegs", () => ({ MultisportLegs: () => null }));
vi.mock("../components/activity/LapsTable", () => ({ LapsTable: () => null }));
vi.mock("../components/activity/EditActivityModal", () => ({ EditActivityModal: () => null }));
vi.mock("../components/activity/FtpMismatchHint", () => ({ FtpMismatchHint: () => null }));
vi.mock("../components/activity/DeviceChip", () => ({ DeviceChip: () => null }));
vi.mock("../components/activity/GearChip", () => ({ GearChip: () => null, gearChipVisible: () => false }));
vi.mock("../components/activity/PhotoGallery", () => ({ PhotoGallery: () => null }));
vi.mock("../components/activity/ShareModal", () => ({ ShareModal: () => null }));
vi.mock("../components/activity/SummaryPanel", () => ({ SummaryPanel: () => null }));
vi.mock("../components/plugins/PluginContributions", () => ({ PluginContributions: () => null }));
// The title editor is reduced to one button that saves a new title.
vi.mock("../components/activity/InlineTitle", () => ({
  InlineTitle: ({ onSave }: { onSave: (t: string) => void }) => (
    <button onClick={() => onSave("Renamed")}>rename</button>
  ),
}));

import { api } from "../lib/tauri";
import { useToastStore } from "../stores/toastStore";
import { ActivityDetailPage } from "./ActivityDetail";

afterEach(cleanup);

const columns = (): TrackPointColumns =>
  ({
    t: [], lat: [], lon: [], altitude_m: [], speed_mps: [], hr: [], cadence: [], power_w: [],
    temperature_c: [], vertical_oscillation_mm: [], stance_time_ms: [], stance_time_percent: [],
    step_length_mm: [], grade_percent: [], distance_m: [], left_right_balance: [],
    left_torque_effectiveness: [], right_torque_effectiveness: [], left_pedal_smoothness: [],
    right_pedal_smoothness: [],
  }) as unknown as TrackPointColumns;

const leg = (over: Partial<MultisportLeg>): MultisportLeg =>
  ({
    id: 1, activity_id: "tri", leg_number: 1, sport_type: "swim", is_transition: false,
    start_time: null, total_distance_m: null, total_timer_time_s: null, total_elapsed_time_s: null,
    avg_speed_mps: null, avg_hr: null, max_hr: null, total_ascent_m: null, total_calories: null,
    source_activity_id: null, ...over,
  }) as MultisportLeg;

function detail(over: Partial<ActivityDetail> = {}): ActivityDetail {
  const activity = {
    id: "tri", start_time: "2026-07-15T18:00:00+03:00", timezone_offset: 180, sport_type: "triathlon",
    title: "Evening Triathlon", notes: null, distance_m: 30000, duration_s: 5400, elev_gain_m: 100,
    elev_loss_m: 100, avg_speed_mps: 5.5, max_speed_mps: 12, avg_hr: 150, max_hr: 180, avg_cadence: null,
    calories: null, location_name: null, start_lat: null, start_lon: null, source_device: null,
    parent_id: null, gear_id: null, threshold_power_w: null, profile_name: null,
  } as unknown as Activity;
  return {
    activity, trackpoints: columns(), gear_id: null, is_multisport: true, laps: [], legs: [],
    lengths: [], sets: [], time_in_zones: [], hrv_samples: [], recent_power: [], ...over,
  };
}

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/activity/tri"]}>
        <Routes>
          <Route path="/activity/:id" element={<ActivityDetailPage />} />
          <Route path="/library" element={<div>Library</div>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const lastToast = () => useToastStore.getState().toasts.slice(-1)[0];

beforeEach(() => {
  useToastStore.setState({ toasts: [] });
  vi.mocked(api.getAdjacentActivities).mockResolvedValue({ prev_id: null, next_id: null });
  vi.mocked(api.listGear).mockResolvedValue([]);
  vi.mocked(api.getActivityRecordBadges).mockResolvedValue([]);
});

describe("ActivityDetailPage refusals (#182)", () => {
  /** The backend refuses with a bare string; a JS-side failure is an
   * Error. The toast carries the words alone either way. */
  for (const [kind, make] of [
    ["a string", (m: string) => m],
    ["an Error", (m: string) => new Error(m)],
  ] as const) {
    it(`toasts why an unmerge was refused with ${kind}, and stays on the page`, async () => {
      vi.mocked(api.getActivityDetail).mockResolvedValue(
        detail({ legs: [leg({ source_activity_id: "swim-1" }), leg({ leg_number: 2, sport_type: "run", source_activity_id: "run-1" })] }),
      );
      vi.mocked(api.unmergeTriathlon).mockRejectedValue(make("a leg was deleted meanwhile"));
      renderPage();
      fireEvent.click(await screen.findByLabelText("Unmerge"));
      await waitFor(() => expect(lastToast()).toMatchObject({ type: "error", message: "a leg was deleted meanwhile" }));
      expect(screen.queryByText("Library")).toBeNull();
    });

    it(`toasts why a title could not be saved with ${kind}`, async () => {
      vi.mocked(api.getActivityDetail).mockResolvedValue(detail({ is_multisport: false, activity: { ...detail().activity, sport_type: "ride" } }));
      vi.mocked(api.updateActivity).mockRejectedValue(make("title too long"));
      renderPage();
      fireEvent.click(await screen.findByText("rename"));
      await waitFor(() => expect(lastToast()).toMatchObject({ type: "error", message: "title too long" }));
      expect(api.updateActivity).toHaveBeenCalledWith("tri", { title: "Renamed" });
    });
  }
});
