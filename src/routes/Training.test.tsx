// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen, waitFor, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Link, MemoryRouter, Route, Routes } from "react-router";
import { api } from "../lib/tauri";
import type { FoundDevice, LinkStatus, PendingRide, StickProblem, TrainingLive, WorkoutFile, WorkoutLive } from "../lib/types";
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
    trainingWorkouts: vi.fn(),
    trainingAntKey: vi.fn(),
    trainingSetAntKey: vi.fn(),
    trainingSetWorkoutsDir: vi.fn(),
    trainingStartWorkout: vi.fn(),
  },
}));

const dialog = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);

const confirmMock = vi.fn<(opts: unknown) => Promise<boolean>>();
vi.mock("../stores/confirmStore", () => ({ confirmDialog: (opts: unknown) => confirmMock(opts) }));

import { Training } from "./Training";

const mocked = vi.mocked(api);

const KEY_PAGE = "https://developer.garmin.com/ant-program/ant-ant-plus/network-keys/";
const AGREEMENT = "https://developer.garmin.com/ant-program/licensing/adopter-agreement/";

/** A search finds `devices`; the stick says `stick`. */
function scanned(devices: FoundDevice[], stick: StickProblem | null = null) {
  mocked.trainingScan.mockResolvedValue({ devices, stick });
}

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
    release_unconfirmed: false,
    power_match: null,
    workout: null,
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
  scanned([]);
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
    mocked.trainingSetWorkoutsDir,
    mocked.trainingStartWorkout,
    mocked.trainingSetAntKey,
  ]) {
    ok.mockResolvedValue(undefined);
  }
  mocked.trainingWorkouts.mockResolvedValue({ dir: null, missing: false, workouts: [] });
  mocked.trainingAntKey.mockResolvedValue({ built_in: true, known: true, page: KEY_PAGE, agreement: AGREEMENT });
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
    scanned([
      { role: "trainer", radio: "ble", address: "T1", name: "AVANTI COG", other_side: null, lone_side: false },
      { role: "trainer", radio: "ble", address: "T2", name: null, other_side: null, lone_side: false },
      { role: "trainer", radio: "ant", address: "ant:17:14830", name: "Elite 14830", other_side: null, lone_side: false },
      { role: "pedals", radio: "ble", address: "P1", name: "ASSIOMA55430L", other_side: null, lone_side: false },
      { role: "strap", radio: "ble", address: "H1", name: "HRM-Pro", other_side: null, lone_side: false },
    ]);
    renderPage();
    fireEvent.click(screen.getByText("Search for devices"));
    expect(await screen.findByText("AVANTI COG")).toBeTruthy();
    expect(screen.getByText("Unnamed device")).toBeTruthy();
    expect(screen.getByText("Search again")).toBeTruthy();
    // The same trainer both ways: which way it goes is said.
    expect(screen.getAllByText("Bluetooth")).toHaveLength(4);
    expect(screen.getAllByText("ANT+ stick")).toHaveLength(1);
    fireEvent.click(screen.getByText("ASSIOMA55430L"));
    fireEvent.click(screen.getByText("Connect"));
    await waitFor(() => expect(mocked.trainingConnect).toHaveBeenCalledWith("T1", ["P1"], null));
    fireEvent.click(screen.getByText("Elite 14830"));
    fireEvent.click(screen.getByText("Connect"));
    await waitFor(() => expect(mocked.trainingConnect).toHaveBeenLastCalledWith("ant:17:14830", ["P1"], null));
  });

  it("asks for the ANT+ network key when the build has none, checks it and can forget it (#277)", async () => {
    mocked.trainingAntKey.mockResolvedValue({ built_in: false, known: false, page: KEY_PAGE, agreement: AGREEMENT });
    renderPage();
    const box = await screen.findByRole("region", { name: "ANT+ USB stick" });
    expect(box.textContent).toContain("Garmin publishes it");
    expect(screen.getByText("The ANT+ network key on Garmin's site").getAttribute("href")).toBe(KEY_PAGE);
    expect(screen.getByText("The ANT+ Adopter Agreement").getAttribute("href")).toBe(AGREEMENT);
    const save = screen.getByText("Save") as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    const field = screen.getByLabelText("ANT+ network key");
    mocked.trainingSetAntKey.mockRejectedValueOnce(commandError("training.badAntKey", "That is not the ANT+ network key"));
    fireEvent.change(field, { target: { value: "01 23 45 67 89 ab cd ef" } });
    fireEvent.click(save);
    expect(await screen.findByText("That is not the ANT+ network key")).toBeTruthy();
    expect(mocked.trainingSetAntKey).toHaveBeenCalledWith("01 23 45 67 89 ab cd ef");
    // The right one: kept, and the box says so.
    mocked.trainingAntKey.mockResolvedValue({ built_in: false, known: true, page: KEY_PAGE, agreement: AGREEMENT });
    fireEvent.submit(field.closest("form")!);
    expect(await screen.findByText(/The ANT\+ network key is saved/)).toBeTruthy();
    fireEvent.click(screen.getByText("Forget it"));
    await waitFor(() => expect(mocked.trainingSetAntKey).toHaveBeenLastCalledWith(null));
    // Built in: nothing to ask.
    cleanup();
    mocked.trainingAntKey.mockResolvedValue({ built_in: true, known: true, page: KEY_PAGE, agreement: AGREEMENT });
    renderPage();
    await screen.findByText("Search for devices");
    expect(screen.queryByRole("region", { name: "ANT+ USB stick" })).toBeNull();
  });

  it("says it is searching while it searches", async () => {
    mocked.trainingScan.mockReturnValue(new Promise(() => {}));
    renderPage();
    fireEvent.click(screen.getByText("Search for devices"));
    expect(await screen.findByText("Searching…")).toBeTruthy();
  });

  it("offers a pedal pair as one and links both its sides (#284)", async () => {
    scanned([
      { role: "trainer", radio: "ble", address: "T1", name: "AVANTI COG", other_side: null, lone_side: false },
      { role: "pedals", radio: "ble", address: "PL", name: "ASSIOMA55430L + ASSIOMA61026R", other_side: "PR", lone_side: false },
      { role: "pedals", radio: "ble", address: "QR", name: "ASSIOMA77001R", other_side: null, lone_side: true },
    ]);
    renderPage();
    fireEvent.click(screen.getByText("Search for devices"));
    expect(await screen.findByText("ASSIOMA55430L + ASSIOMA61026R")).toBeTruthy();
    expect(screen.getByText("both pedals")).toBeTruthy();
    expect(screen.getByText(/one pedal of a pair/)).toBeTruthy();
    fireEvent.click(screen.getByText("ASSIOMA55430L + ASSIOMA61026R"));
    fireEvent.click(screen.getByText("Connect"));
    await waitFor(() => expect(mocked.trainingConnect).toHaveBeenCalledWith("T1", ["PL", "PR"], null));
  });

  it("lets pedals and a strap be left out again", async () => {
    scanned([
      { role: "trainer", radio: "ble", address: "T1", name: "AVANTI COG", other_side: null, lone_side: false },
      { role: "pedals", radio: "ble", address: "P1", name: "ASSIOMA55430L", other_side: null, lone_side: false },
    ]);
    renderPage();
    fireEvent.click(screen.getByText("Search for devices"));
    fireEvent.click(await screen.findByText("ASSIOMA55430L"));
    fireEvent.click(screen.getAllByText("None")[0]);
    fireEvent.click(screen.getByText("Connect"));
    await waitFor(() => expect(mocked.trainingConnect).toHaveBeenCalledWith("T1", [], null));
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

    scanned([{ role: "strap", radio: "ble", address: "H1", name: "HRM-Pro", other_side: null, lone_side: false }]);
    fireEvent.click(screen.getByText("Search again"));
    expect(await screen.findByText(/No trainer found/)).toBeTruthy();
    expect((screen.getByText("Connect") as HTMLButtonElement).disabled).toBe(true);

    // The stick could not search along: said, whatever was found.
    scanned([], { kind: "in_use" });
    fireEvent.click(screen.getByText("Search again"));
    expect(await screen.findByText(/another app is using it \(Zwift, Garmin Express\)/)).toBeTruthy();
    scanned([], { kind: "other", detail: "the ANT stick refused message 0x46 (code 0x28)" });
    fireEvent.click(screen.getByText("Search again"));
    expect(await screen.findByText("The ANT+ stick did not search: the ANT stick refused message 0x46 (code 0x28)")).toBeTruthy();

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
    hook.live = live({
      phase: "pairing",
      links: [link({ state: "failed", problem: "stick_gone" }), link({ role: "pedals", state: "failed", problem: "stick_busy" })],
    });
    renderPage();
    expect(screen.getByText("no ANT+ stick, or it was unplugged")).toBeTruthy();
    expect(screen.getByText("another app is using the ANT+ stick")).toBeTruthy();
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
    expect(screen.queryByText(/refused to let go/)).toBeNull();
    cleanup();
    hook.live = live({ phase: "finishing", release_unconfirmed: true });
    renderPage();
    expect(screen.getByText(/refused to let go: another app may control it/)).toBeTruthy();
  });

  it("shows the brake being let go after riding free, ERG off", () => {
    hook.live = live({ erg: { target_w: 0, state: "releasing" } });
    renderPage();
    expect(screen.getByText("Letting the brake go…")).toBeTruthy();
    expect(screen.getByText("Hold a power")).toBeTruthy();
    expect(screen.queryByLabelText("Target power")).toBeNull();
  });

  it("says whether ERG is held by the pedals' watts or the trainer's (#275)", () => {
    const erg = { target_w: 200, state: "holding" as const };
    hook.live = live({ erg, power_match: { state: "matching", trainer_target_w: 182 } });
    renderPage();
    expect(screen.getByText(/Held by the pedals' watts: the trainer is asked for 182/)).toBeTruthy();
    cleanup();
    hook.live = live({ erg, power_match: { state: "no_pedals", trainer_target_w: 200 } });
    renderPage();
    expect(screen.getByText("No power from the pedals: held by the trainer's own watts.")).toBeTruthy();
    cleanup();
    // Letting go after riding free: nothing is matched.
    hook.live = live({ erg: { target_w: 0, state: "releasing" }, power_match: { state: "matching", trainer_target_w: 0 } });
    renderPage();
    expect(screen.queryByText(/Held by the pedals' watts/)).toBeNull();
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
      ready: /takes the target once you pedal/,
      no_cadence: /sends no cadence/,
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

function workoutFile(over: Partial<WorkoutFile> = {}): WorkoutFile {
  return {
    path: "ss.zwo",
    name: "Sweet spot",
    duration_s: 3600,
    profile: [
      { secs: 600, from: 0.4, to: 0.75, free: false, cadence: null },
      { secs: 2400, from: 0.9, to: 0.9, free: false, cadence: 90 },
      { secs: 600, from: 0, to: 0, free: true, cadence: null },
    ],
    notes: [],
    problem: null,
    ...over,
  };
}

function workoutLive(over: Partial<WorkoutLive> = {}): WorkoutLive {
  return {
    name: "Sweet spot",
    elapsed_s: 900,
    total_s: 3600,
    step: 1,
    steps: 5,
    step_left_s: 60,
    free: false,
    cadence: 90,
    next: { secs: 300, from_w: 150, to_w: 150 },
    cue: null,
    done: false,
    ...over,
  };
}

describe("Workouts (#276)", () => {
  it("picks a folder, lists what is in it, and starts one", async () => {
    hook.live = live({ phase: "pairing" });
    renderPage();
    expect(await screen.findByText(/Zwift keeps its own in/)).toBeTruthy();
    // Called off: nothing changes.
    dialog.open.mockResolvedValue(null);
    fireEvent.click(screen.getByText("Choose a workouts folder"));
    await waitFor(() => expect(dialog.open).toHaveBeenCalledWith({ directory: true, multiple: false, title: "Folder with .zwo workouts" }));
    expect(mocked.trainingSetWorkoutsDir).not.toHaveBeenCalled();

    mocked.trainingWorkouts.mockResolvedValue({
      dir: "/Users/me/Workouts",
      missing: false,
      workouts: [
        workoutFile({
          notes: [
            { kind: "unknown", element: "Mystery" },
            { kind: "ridden_free", element: "MaxEffort" },
            { kind: "bad_value", element: "Ramp" },
            { kind: "distance_cue" },
            { kind: "cue_outside" },
          ],
        }),
        workoutFile({ path: "run.zwo", name: "Tempo run", problem: { kind: "run" }, profile: [] }),
      ],
    });
    dialog.open.mockResolvedValue("/Users/me/Workouts");
    fireEvent.click(screen.getByText("Choose a workouts folder"));
    await waitFor(() => expect(mocked.trainingSetWorkoutsDir).toHaveBeenCalledWith("/Users/me/Workouts"));
    const list = await screen.findByRole("region", { name: "Workouts" });
    expect(list.textContent).toContain("/Users/me/Workouts");
    expect(screen.getByText("Change folder")).toBeTruthy();
    expect(
      screen.getByText(
        "1:00:00 · Mystery skipped · MaxEffort ridden free · a Ramp with a wrong value skipped · messages placed by distance skipped · messages past the end of their interval skipped",
      ),
    ).toBeTruthy();
    expect(screen.getByText("A running workout: only rides are supported")).toBeTruthy();
    expect(list.querySelectorAll("polygon")).toHaveLength(3);
    expect(screen.getAllByText("Ride it")).toHaveLength(1);
    fireEvent.click(screen.getByText("Ride it"));
    await waitFor(() => expect(mocked.trainingStartWorkout).toHaveBeenCalledWith("ss.zwo"));
    // Refused: said under the list.
    mocked.trainingStartWorkout.mockRejectedValue(commandError("training.noSuchWorkout", "That workout is not in the folder"));
    fireEvent.click(screen.getByText("Ride it"));
    expect(await screen.findByText("That workout is not in the folder")).toBeTruthy();
  });

  it("says why a file cannot be ridden, an empty folder, and no FTP", async () => {
    const problems = [
      [{ kind: "unreadable", detail: "<workout> is never closed" }, "Cannot be read: <workout> is never closed"],
      [{ kind: "not_a_workout" }, "Not a workout file"],
      [{ kind: "empty" }, "Nothing in it can be ridden"],
      [{ kind: "too_big" }, "Too large for a workout file"],
      [{ kind: "distance" }, "Its intervals are in metres: only workouts by time are supported"],
      [{ kind: "too_long" }, "Longer than a day, or more intervals than a workout has"],
    ] as const;
    mocked.trainingWorkouts.mockResolvedValue({
      dir: "/w",
      missing: false,
      workouts: problems.map(([problem], i) => workoutFile({ path: `${i}.zwo`, problem, profile: [] })),
    });
    hook.live = live();
    renderPage();
    for (const [, text] of problems) {
      expect(await screen.findByText(text)).toBeTruthy();
    }
    expect(screen.queryByText("Ride it")).toBeNull();

    cleanup();
    mocked.trainingWorkouts.mockResolvedValue({ dir: "/w", missing: false, workouts: [] });
    renderPage();
    expect(await screen.findByText("No .zwo workouts in this folder.")).toBeTruthy();

    cleanup();
    mocked.trainingWorkouts.mockResolvedValue({ dir: "/w", missing: true, workouts: [] });
    renderPage();
    expect(await screen.findByText("The folder is gone: choose it again, or another.")).toBeTruthy();
    expect(screen.queryByText("No .zwo workouts in this folder.")).toBeNull();

    cleanup();
    mocked.trainingFtp.mockResolvedValue(null);
    mocked.trainingWorkouts.mockResolvedValue({ dir: "/w", missing: false, workouts: [workoutFile()] });
    renderPage();
    expect(await screen.findByText(/none was found in your rides yet/)).toBeTruthy();
    expect((screen.getByText("Ride it") as HTMLButtonElement).disabled).toBe(true);
  });

  it("shows where the workout is, what comes next, and stops it", async () => {
    mocked.trainingWorkouts.mockResolvedValue({ dir: "/w", missing: false, workouts: [workoutFile()] });
    hook.live = live({ erg: { target_w: 220, state: "holding" }, workout: workoutLive({ cue: "Settle in" }) });
    const page = renderPage();
    const erg = screen.getByRole("region", { name: "ERG mode" });
    expect(screen.getByLabelText("Target power").textContent).toContain("220 W");
    expect(erg.textContent).toContain("Sweet spot");
    expect(erg.textContent).toContain("15:00 of 1:00:00");
    expect(erg.textContent).toContain("Interval 2 of 5 · 1:00 left · cadence 90 rpm");
    expect(erg.textContent).toContain("Next: 150 W for 5:00");
    expect(screen.getByText("Settle in")).toBeTruthy();
    expect(screen.queryByLabelText("10 W more")).toBeNull();
    expect(screen.queryByRole("region", { name: "Workouts" })).toBeNull();
    fireEvent.click(screen.getByText("Stop the workout"));
    await waitFor(() => expect(mocked.trainingSetTarget).toHaveBeenCalledWith(null));

    const next = [
      [{ secs: 120, from_w: 150, to_w: 250 }, "Next: 150 W to 250 W over 2:00"],
      [{ secs: 300, from_w: 0, to_w: 0 }, "Next: ride free for 5:00"],
      [null, "The last interval."],
    ] as const;
    for (const [step, text] of next) {
      hook.live = live({ erg: { target_w: 220, state: "holding" }, workout: workoutLive({ next: step, cadence: null }) });
      page.again();
      expect(screen.getByText(text)).toBeTruthy();
    }
    // A free stretch: no target, the workout goes on.
    hook.live = live({ erg: null, workout: workoutLive({ free: true }) });
    page.again();
    expect(screen.getByLabelText("Target power").textContent).toBe("Free ride");
    expect(screen.queryByText("Hold a power")).toBeNull();
    // Over: riding free, the list again.
    hook.live = live({ erg: null, workout: workoutLive({ done: true, step: 5 }) });
    page.again();
    expect(screen.getByText("Sweet spot is over: riding free.")).toBeTruthy();
    expect(screen.getByText("Hold a power")).toBeTruthy();
    expect(await screen.findByRole("region", { name: "Workouts" })).toBeTruthy();
  });
});
