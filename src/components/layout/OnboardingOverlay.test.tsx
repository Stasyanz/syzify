// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { api } from "../../lib/tauri";
import {
  OnboardingOverlay,
  ONBOARDING_KEY,
  arrowPath,
  shouldShowOnboarding,
} from "./OnboardingOverlay";

vi.mock("../../lib/tauri", () => ({
  api: {
    getSetting: vi.fn(),
    setSetting: vi.fn().mockResolvedValue(undefined),
    getActivities: vi.fn(),
  },
  isTauri: () => false,
}));

const mocked = vi.mocked(api);

// The probe only counts rows — a bare id is all the mock needs.
const row = { id: "a-1" } as unknown as Awaited<
  ReturnType<typeof api.getActivities>
>[number];

afterEach(cleanup);
beforeEach(() => {
  vi.clearAllMocks();
  mocked.setSetting.mockResolvedValue(undefined);
});

function renderOverlay() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <OnboardingOverlay />
    </QueryClientProvider>,
  );
}

describe("shouldShowOnboarding", () => {
  it("shows only for a resolved, unset flag over an empty library", () => {
    expect(shouldShowOnboarding(null, 0)).toBe(true);
    expect(shouldShowOnboarding("1", 0)).toBe(false); // already passed
    expect(shouldShowOnboarding(null, 3)).toBe(false); // lived-in install
    expect(shouldShowOnboarding(undefined, 0)).toBe(false); // still loading
    expect(shouldShowOnboarding(null, undefined)).toBe(false); // still loading
  });

  it('treats an explicit "0" as the tester override, any library size', () => {
    expect(shouldShowOnboarding("0", 5)).toBe(true);
    expect(shouldShowOnboarding("0", undefined)).toBe(true);
  });
});

describe("arrowPath", () => {
  it("starts and ends at the given points with one control point", () => {
    expect(arrowPath(0, 0, 100, 0)).toBe("M 0 0 Q 50 25 100 0");
  });
});

describe("OnboardingOverlay", () => {
  it("appears on a fresh install and Got it persists the flag", async () => {
    mocked.getSetting.mockResolvedValue(null);
    mocked.getActivities.mockResolvedValue([]);
    renderOverlay();

    expect(await screen.findByText("Import your first workout")).toBeTruthy();
    fireEvent.click(screen.getByText("Got it"));

    expect(screen.queryByText("Import your first workout")).toBeNull();
    expect(mocked.setSetting).toHaveBeenCalledWith(ONBOARDING_KEY, "1");
  });

  it("the X close also persists the flag, exactly once", async () => {
    mocked.getSetting.mockResolvedValue(null);
    mocked.getActivities.mockResolvedValue([]);
    renderOverlay();

    fireEvent.click(await screen.findByLabelText("Close"));
    expect(mocked.setSetting).toHaveBeenCalledTimes(1);
  });

  it("never appears once the flag is set", async () => {
    mocked.getSetting.mockResolvedValue("1");
    mocked.getActivities.mockResolvedValue([]);
    renderOverlay();

    // Let the queries settle, then assert absence.
    await waitFor(() => expect(mocked.getSetting).toHaveBeenCalled());
    expect(screen.queryByText("Import your first workout")).toBeNull();
  });

  it("never appears for a lived-in library and doesn't write the flag", async () => {
    mocked.getSetting.mockResolvedValue(null);
    mocked.getActivities.mockResolvedValue([row]);
    renderOverlay();

    await waitFor(() => expect(mocked.getActivities).toHaveBeenCalled());
    expect(screen.queryByText("Import your first workout")).toBeNull();
    // Existing installs keep their unset flag — the empty-library condition
    // already shields them, and an unsolicited write would hide a real
    // fresh-install regression from tests.
    expect(mocked.setSetting).not.toHaveBeenCalled();
  });

  it('forced "0" shows over a lived-in library without self-completing', async () => {
    mocked.getSetting.mockResolvedValue("0");
    mocked.getActivities.mockResolvedValue([row]);
    renderOverlay();

    expect(await screen.findByText("Import your first workout")).toBeTruthy();
    // The activities already present must NOT count as "first import landed".
    expect(mocked.setSetting).not.toHaveBeenCalled();
  });

  it("completes silently when the first import lands while it is up", async () => {
    mocked.getSetting.mockResolvedValue(null);
    mocked.getActivities.mockResolvedValue([]);
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={qc}>
        <OnboardingOverlay />
      </QueryClientProvider>,
    );
    expect(await screen.findByText("Import your first workout")).toBeTruthy();

    // The import invalidates the shared "activities" prefix; the probe
    // refetches and now sees one row.
    mocked.getActivities.mockResolvedValue([row]);
    await qc.invalidateQueries({ queryKey: ["activities"] });

    await waitFor(() =>
      expect(screen.queryByText("Import your first workout")).toBeNull(),
    );
    expect(mocked.setSetting).toHaveBeenCalledWith(ONBOARDING_KEY, "1");
  });
});
