// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen, waitFor, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Link, MemoryRouter, Route, Routes } from "react-router";
import { api } from "../lib/tauri";
import type { LinkStatus, PendingRide, TrainingLive } from "../lib/types";
import { useLanguageStore } from "../lib/i18n";
import { useToastStore } from "../stores/toastStore";
import { commandError } from "../test/commandError";

const hook = vi.hoisted(() => ({ live: undefined as TrainingLive | null | undefined }));
vi.mock("../hooks/useTrainingLive", () => ({ useTrainingLive: () => hook.live }));

vi.mock("../lib/tauri", () => ({
  api: {
    trainingScan: vi.fn(),
    trainingConnect: vi.fn(),
    trainingBegin: vi.fn(),
    trainingLap: vi.fn(),
    trainingPause: vi.fn(),
    trainingResume: vi.fn(),
    trainingSetTarget: vi.fn(),
    trainingFinish: vi.fn(),
    trainingDiscard: vi.fn(),
    trainingLeave: vi.fn(),
    trainingFtp: vi.fn(),
    trainingPending: vi.fn(),
    trainingFinishPending: vi.fn(),
    trainingDiscardPending: vi.fn(),
  },
}));

const confirmMock = vi.fn<(opts: unknown) => Promise<boolean>>();
vi.mock("../stores/confirmStore", () => ({ confirmDialog: (opts: unknown) => confirmMock(opts) }));

import { Training } from "./Training";

const mocked = vi.mocked(api);

function link(over: Partial<LinkStatus> = {}): LinkStatus {
  return { role: "trainer", state: "connected", name: null, problem: null, detail: null, ...over };
}

function live(over: Partial<TrainingLive> = {}): TrainingLive {
  return {
    phase: "riding",
    ride_s: 3725,
    lap_s: 125,
    lap: 2,
    watts: 200,
    power_from: "pedals",
    trainer_watts: 212,
    cadence_rpm: 90,
    heart_rate: 141,
    speed_kmh: 36,
    distance_m: 25_400,
    avg_watts: 187,
    profile: "Elite Avanti",
    links: [link({ name: "AVANTI COG" }), link({ role: "pedals", name: "ASSIOMA" }), link({ role: "strap", state: "lost" })],
    warnings: [],
    slept_s: 0,
    erg: null,
    ...over,
  };
}

function pending(over: Partial<PendingRide> = {}): PendingRide {
  return { start_unix: 1_794_420_000, ride_s: 1800, locked: false, sealed_elsewhere: false, unreadable: null, ...over };
}

afterEach(() => {
  cleanup();
  useToastStore.setState({ toasts: [] });
});
beforeEach(() => {
  vi.clearAllMocks();
  vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)");
  act(() => useLanguageStore.getState().setPref("en"));
  hook.live = null;
  mocked.trainingPending.mockResolvedValue([]);
  mocked.trainingFtp.mockResolvedValue(244);
  mocked.trainingScan.mockResolvedValue([]);
  for (const ok of [
    mocked.trainingConnect,
    mocked.trainingBegin,
    mocked.trainingLap,
    mocked.trainingPause,
    mocked.trainingResume,
    mocked.trainingSetTarget,
    mocked.trainingDiscard,
    mocked.trainingLeave,
    mocked.trainingDiscardPending,
  ]) {
    ok.mockResolvedValue(undefined);
  }
  mocked.trainingFinish.mockResolvedValue({ kind: "imported", activity_id: "new-ride" });
  mocked.trainingFinishPending.mockResolvedValue({ kind: "imported", activity_id: "old-ride" });
  confirmMock.mockResolvedValue(true);
});

/** The page; `again()` draws it once more with the same client (after
 * `hook.live` changed, as an event would). */
function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const tree = () => (
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/training"]}>
        <Link to="/elsewhere">go elsewhere</Link>
        <Routes>
          <Route path="/training" element={<Training />} />
          <Route path="/activity/:id" element={<p>activity page</p>} />
          <Route path="/elsewhere" element={<p>elsewhere</p>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>
  );
  const view = render(tree());
  return { ...view, again: () => view.rerender(tree()) };
}

const toasts = () => useToastStore.getState().toasts.map((t) => t.message);

describe("Training before a session", () => {
  it("shows nothing until it knows whether a session runs", () => {
    hook.live = undefined;
    renderPage();
    expect(screen.queryByText("Devices")).toBeNull();
  });

  it("offers the rides a crash left: finish one into the library, discard another", async () => {
    mocked.trainingPending.mockResolvedValue([
      pending(),
      pending({ start_unix: 1_794_430_000, ride_s: null, locked: true }),
      pending({ start_unix: 1_794_440_000, ride_s: null, unreadable: "the journal cannot be read: empty" }),
      pending({ start_unix: 1_794_450_000, ride_s: null, sealed_elsewhere: true }),
    ]);
    renderPage();
    expect(await screen.findByText("Unfinished rides")).toBeTruthy();
    expect(screen.getByText(/30:00/)).toBeTruthy();
    expect(screen.getByText("Encrypted: unlock the vault to finish it.")).toBeTruthy();
    expect(screen.getByText("Cannot be read: the journal cannot be read: empty")).toBeTruthy();
    expect(screen.getByText(/another vault's key/)).toBeTruthy();
    // Only the readable one can be finished; each can be discarded.
    expect(screen.getAllByText("Finish")).toHaveLength(1);
    fireEvent.click(screen.getAllByText("Discard")[2]);
    await waitFor(() => expect(mocked.trainingDiscardPending).toHaveBeenCalledWith(1_794_440_000));
    fireEvent.click(screen.getByText("Finish"));
    await waitFor(() => expect(mocked.trainingFinishPending).toHaveBeenCalledWith(1_794_420_000));
    expect(await screen.findByText("activity page")).toBeTruthy();
  });

  it("says why a left-over ride did not finish, and keeps it when the discard is called off", async () => {
    mocked.trainingPending.mockResolvedValue([pending()]);
    mocked.trainingFinishPending.mockRejectedValue(commandError("vault.locked", "The vault is locked — unlock it first"));
    confirmMock.mockResolvedValue(false);
    renderPage();
    fireEvent.click(await screen.findByText("Finish"));
    await waitFor(() => expect(toasts()).toContain("The vault is locked — unlock it first"));
    fireEvent.click(screen.getByText("Discard"));
    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(mocked.trainingDiscardPending).not.toHaveBeenCalled();
  });

  it("searches, preselects the first trainer and connects the picks", async () => {
    mocked.trainingScan.mockResolvedValue([
      { role: "trainer", address: "T1", name: "AVANTI COG" },
      { role: "trainer", address: "T2", name: null },
      { role: "pedals", address: "P1", name: "ASSIOMA55430L" },
      { role: "strap", address: "H1", name: "HRM-Pro" },
    ]);
    renderPage();
    fireEvent.click(screen.getByText("Search for devices"));
    expect(await screen.findByText("AVANTI COG")).toBeTruthy();
    expect(screen.getByText("Unnamed device")).toBeTruthy();
    expect(screen.getByText("Search again")).toBeTruthy();
    fireEvent.click(screen.getByText("ASSIOMA55430L"));
    fireEvent.click(screen.getByText("Connect"));
    await waitFor(() => expect(mocked.trainingConnect).toHaveBeenCalledWith("T1", "P1", null));
  });

  it("says it is searching while it searches", async () => {
    mocked.trainingScan.mockReturnValue(new Promise(() => {}));
    renderPage();
    fireEvent.click(screen.getByText("Search for devices"));
    expect(await screen.findByText("Searching…")).toBeTruthy();
  });

  it("lets pedals and a strap be left out again", async () => {
    mocked.trainingScan.mockResolvedValue([
      { role: "trainer", address: "T1", name: "AVANTI COG" },
      { role: "pedals", address: "P1", name: "ASSIOMA55430L" },
    ]);
    renderPage();
    fireEvent.click(screen.getByText("Search for devices"));
    fireEvent.click(await screen.findByText("ASSIOMA55430L"));
    fireEvent.click(screen.getAllByText("None")[0]);
    fireEvent.click(screen.getByText("Connect"));
    await waitFor(() => expect(mocked.trainingConnect).toHaveBeenCalledWith("T1", null, null));
  });

  it("only finishes left-over rides elsewhere than macOS", async () => {
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Mozilla/5.0 (X11; Linux x86_64)");
    mocked.trainingPending.mockResolvedValue([pending()]);
    renderPage();
    expect(await screen.findByText("Unfinished rides")).toBeTruthy();
    expect(screen.getByText(/Riding a trainer needs macOS/)).toBeTruthy();
    expect(screen.queryByText("Search for devices")).toBeNull();
  });

  it("says why a discard failed", async () => {
    mocked.trainingPending.mockResolvedValue([pending()]);
    mocked.trainingDiscardPending.mockRejectedValue(commandError("training.noSuchRide", "That ride is gone"));
    renderPage();
    fireEvent.click(await screen.findByText("Discard"));
    await waitFor(() => expect(toasts()).toContain("That ride is gone"));
  });

  it("says when nothing or no trainer was found, and why a search failed", async () => {
    renderPage();
    fireEvent.click(screen.getByText("Search for devices"));
    expect(await screen.findByText(/Nothing found/)).toBeTruthy();

    mocked.trainingScan.mockResolvedValue([{ role: "strap", address: "H1", name: "HRM-Pro" }]);
    fireEvent.click(screen.getByText("Search again"));
    expect(await screen.findByText(/No trainer found/)).toBeTruthy();
    expect((screen.getByText("Connect") as HTMLButtonElement).disabled).toBe(true);

    mocked.trainingScan.mockRejectedValue(commandError("training.bluetoothOff", "Bluetooth is off"));
    fireEvent.click(screen.getByText("Search again"));
    expect(await screen.findByText("Bluetooth is off")).toBeTruthy();
  });
});

describe("Training while pairing", () => {
  it("starts once the trainer is heard, or disconnects", async () => {
    hook.live = live({ phase: "pairing", ride_s: 0, links: [link({ state: "searching" })] });
    renderPage();
    expect((screen.getByText("Start") as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText(/Nothing is recorded yet/)).toBeTruthy();
    expect(screen.getByText("searching…")).toBeTruthy();

    cleanup();
    hook.live = live({ phase: "pairing", ride_s: 0, links: [link()] });
    renderPage();
    fireEvent.click(screen.getByText("Start"));
    await waitFor(() => expect(mocked.trainingBegin).toHaveBeenCalled());
    fireEvent.click(screen.getByText("Disconnect"));
    await waitFor(() => expect(mocked.trainingLeave).toHaveBeenCalled());
  });

  it("names why a device is not available", () => {
    hook.live = live({
      phase: "pairing",
      links: [
        link({ state: "failed", problem: "permission_denied" }),
        link({ role: "pedals", state: "failed", problem: "radio_off" }),
        link({ role: "strap", state: "failed", problem: "adapter_gone" }),
      ],
    });
    renderPage();
    expect(screen.getByText(/Bluetooth not allowed/)).toBeTruthy();
    expect(screen.getByText("Bluetooth is off")).toBeTruthy();
    expect(screen.getByText("no Bluetooth adapter")).toBeTruthy();
    cleanup();
    hook.live = live({ phase: "pairing", links: [link({ state: "failed", problem: "other", detail: "boom" })] });
    renderPage();
    expect(screen.getByText("not available: boom")).toBeTruthy();
  });
});

describe("Training during the ride", () => {
  it("shows the ride and its zone, the links quietly", async () => {
    hook.live = live();
    renderPage();
    expect(screen.getByRole("group", { name: "Power" }).textContent).toContain("200 W");
    expect(await screen.findByText("Z3 · 82% of FTP")).toBeTruthy();
    expect(screen.getByRole("group", { name: "Cadence" }).textContent).toContain("90 rpm");
    expect(screen.getByRole("group", { name: "Heart rate" }).textContent).toContain("141 bpm");
    expect(screen.getByRole("group", { name: "Speed" }).textContent).toContain("36.0 km/h");
    expect(screen.getByRole("group", { name: "Time" }).textContent).toContain("1:02:05");
    expect(screen.getByRole("group", { name: "Distance" }).textContent).toContain("25.40 km");
    expect(screen.getByRole("group", { name: "Lap 2" }).textContent).toContain("2:05");
    expect(screen.getByRole("group", { name: "Average power" }).textContent).toContain("187 W");
    expect(screen.getByText("reconnecting…")).toBeTruthy();
    expect(screen.getByText("Elite Avanti")).toBeTruthy();
  });

  it("says when there is no FTP, what the trainer measures, a sleep and a failing disk", async () => {
    mocked.trainingFtp.mockResolvedValue(null);
    hook.live = live({ slept_s: 65, warnings: ["No space left on device"] });
    renderPage();
    expect(await screen.findByText(/Zones show once/)).toBeTruthy();
    expect(screen.getByText(/The Mac slept for 1:05/)).toBeTruthy();
    expect(screen.getByText(/No space left on device/)).toBeTruthy();
    cleanup();
    mocked.trainingFtp.mockResolvedValue(244);
    hook.live = live({ watts: null, power_from: "pedals" });
    renderPage();
    expect(await screen.findByText("Trainer: 212 W")).toBeTruthy();
  });

  it("takes a lap, pauses and finishes into the new activity", async () => {
    hook.live = live();
    renderPage();
    fireEvent.click(screen.getByText("Lap"));
    await waitFor(() => expect(mocked.trainingLap).toHaveBeenCalled());
    fireEvent.click(screen.getByText("Pause"));
    await waitFor(() => expect(mocked.trainingPause).toHaveBeenCalled());
    fireEvent.click(screen.getByText("Finish"));
    expect(await screen.findByText("activity page")).toBeTruthy();
  });

  it("does not pull the user back when the ride is saved after they left", async () => {
    let done: (f: { kind: "imported"; activity_id: string }) => void = () => {};
    mocked.trainingFinish.mockReturnValue(new Promise((resolve) => (done = resolve)));
    hook.live = live();
    renderPage();
    fireEvent.click(screen.getByText("Finish"));
    fireEvent.click(screen.getByText("go elsewhere"));
    expect(screen.getByText("elsewhere")).toBeTruthy();
    await act(async () => done({ kind: "imported", activity_id: "new-ride" }));
    expect(screen.getByText("elsewhere")).toBeTruthy();
    expect(screen.queryByText("activity page")).toBeNull();
  });

  it("resumes a paused ride; no lap while paused", async () => {
    hook.live = live({ phase: "paused" });
    renderPage();
    expect(screen.getAllByText("Paused").length).toBeGreaterThan(0);
    expect((screen.getByText("Lap") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByText("Resume"));
    await waitFor(() => expect(mocked.trainingResume).toHaveBeenCalled());
  });

  it("discards after asking, and says why an order failed", async () => {
    hook.live = live();
    mocked.trainingLap.mockRejectedValue(commandError("training.none", "No trainer is connected"));
    renderPage();
    fireEvent.click(screen.getByText("Discard"));
    await waitFor(() => expect(mocked.trainingDiscard).toHaveBeenCalled());
    fireEvent.click(screen.getByText("Lap"));
    expect(await screen.findByText("No trainer is connected")).toBeTruthy();
  });

  it("says a duplicate or an empty ride instead of opening it", async () => {
    hook.live = live();
    mocked.trainingFinish.mockResolvedValue({ kind: "duplicate", activity_id: null });
    renderPage();
    fireEvent.click(screen.getByText("Finish"));
    await waitFor(() => expect(toasts()).toContain("That ride is already in your library."));
    cleanup();
    mocked.trainingFinish.mockResolvedValue({ kind: "empty", activity_id: null });
    renderPage();
    fireEvent.click(screen.getByText("Finish"));
    await waitFor(() => expect(toasts()).toContain("Nothing was recorded, so no ride was saved."));
  });

  it("says a failed finish even though the session ended first", async () => {
    let fail: (e: unknown) => void = () => {};
    mocked.trainingFinish.mockReturnValue(new Promise((_, reject) => (fail = reject)));
    mocked.trainingDiscard.mockRejectedValue(commandError("training.none", "No trainer is connected"));
    hook.live = live();
    const page = renderPage();
    fireEvent.click(screen.getByText("Finish"));
    // The backend ends the session (live goes null) before it answers.
    hook.live = null;
    page.again();
    expect(await screen.findByText("Search for devices")).toBeTruthy();
    await act(async () => fail(commandError("vault.locked", "The vault is locked — unlock it first")));
    await waitFor(() => expect(toasts()).toContain("The vault is locked — unlock it first"));
    expect(mocked.trainingPending.mock.calls.length).toBeGreaterThan(1);

    cleanup();
    hook.live = live();
    renderPage();
    fireEvent.click(screen.getByText("Discard"));
    await waitFor(() => expect(toasts()).toContain("No trainer is connected"));
  });

  it("shows dashes for what is not heard, and keeps a ride whose discard is called off", async () => {
    mocked.trainingFtp.mockResolvedValue(244);
    confirmMock.mockResolvedValue(false);
    hook.live = live({
      watts: null,
      power_from: null,
      trainer_watts: null,
      cadence_rpm: null,
      heart_rate: null,
      speed_kmh: null,
      links: [link({ state: "failed", problem: "other", detail: null })],
    });
    renderPage();
    for (const label of ["Power", "Cadence", "Heart rate", "Speed"]) {
      expect(screen.getByRole("group", { name: label }).textContent).toContain("--");
    }
    expect(screen.getByText("not available:")).toBeTruthy();
    expect(screen.queryByText(/Trainer:/)).toBeNull();
    fireEvent.click(screen.getByText("Discard"));
    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(mocked.trainingDiscard).not.toHaveBeenCalled();
  });

  it("holds every order while the ride is being saved", () => {
    hook.live = live({ phase: "finishing" });
    renderPage();
    expect(screen.getByText(/Letting the trainer go/)).toBeTruthy();
    for (const label of ["Lap", "Pause", "Finish", "Discard"]) {
      expect((screen.getByText(label) as HTMLButtonElement).disabled).toBe(true);
    }
    expect(screen.queryByRole("region", { name: "ERG mode" })).toBeNull();
  });
});

describe("ERG on the ride (#274)", () => {
  it("is switched on at an easy effort from the FTP", async () => {
    hook.live = live({ phase: "pairing" });
    renderPage();
    await screen.findByText(/of FTP/); // the FTP is known
    fireEvent.click(screen.getByText("Hold a power"));
    await waitFor(() => expect(mocked.trainingSetTarget).toHaveBeenCalledWith(160));
  });

  it("steps the target, rides free, and says what the trainer does", async () => {
    hook.live = live({ erg: { target_w: 150, state: "holding" } });
    renderPage();
    expect(screen.getByLabelText("Target power").textContent).toContain("150 W");
    expect(screen.getByText("The trainer holds the target.")).toBeTruthy();
    // Two quick clicks before the screen shows the first back: +20.
    fireEvent.click(screen.getByLabelText("10 W more"));
    fireEvent.click(screen.getByLabelText("10 W more"));
    await waitFor(() => expect(mocked.trainingSetTarget).toHaveBeenLastCalledWith(170));
    fireEvent.click(screen.getByLabelText("10 W less"));
    await waitFor(() => expect(mocked.trainingSetTarget).toHaveBeenLastCalledWith(160));
    fireEvent.click(screen.getByText("Ride free"));
    await waitFor(() => expect(mocked.trainingSetTarget).toHaveBeenLastCalledWith(null));

    const says = {
      ready: /Held from the start/,
      waiting: /Waiting for the trainer/,
      off_target: /Off the target/,
      low_cadence: /Cadence too low/,
      paused: /the brake is off\.$/,
    } as const;
    for (const [state, text] of Object.entries(says)) {
      cleanup();
      hook.live = live({ erg: { target_w: 150, state: state as keyof typeof says } });
      renderPage();
      expect(screen.getByText(text)).toBeTruthy();
    }
  });

  it("says why a target was refused, and follows the screen once it shows the target back", async () => {
    hook.live = live({ erg: { target_w: 150, state: "holding" } });
    mocked.trainingSetTarget.mockRejectedValue(commandError("training.none", "No trainer is connected"));
    const page = renderPage();
    fireEvent.click(screen.getByLabelText("10 W more"));
    expect(await screen.findByText("No trainer is connected")).toBeTruthy();
    // Refused: the next step starts from what the screen shows (150).
    mocked.trainingSetTarget.mockResolvedValue(undefined);
    fireEvent.click(screen.getByLabelText("10 W more"));
    await waitFor(() => expect(mocked.trainingSetTarget).toHaveBeenLastCalledWith(160));
    // Shown back: the next step starts there.
    hook.live = live({ erg: { target_w: 160, state: "holding" } });
    page.again();
    fireEvent.click(screen.getByLabelText("10 W more"));
    await waitFor(() => expect(mocked.trainingSetTarget).toHaveBeenLastCalledWith(170));
  });
});
