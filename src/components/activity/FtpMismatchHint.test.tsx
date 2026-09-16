// @vitest-environment happy-dom
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, cleanup, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";

vi.mock("../../lib/tauri", () => ({
  api: { getSetting: vi.fn(async () => null), setSetting: vi.fn(async () => undefined) },
}));

import { api } from "../../lib/tauri";
import { FtpMismatchHint, ftpMismatch, recentFtp, ftpHintDismissedKey } from "./FtpMismatchHint";
import type { PreviousPower } from "../../lib/types";

afterEach(() => {
  cleanup();
  vi.mocked(api.getSetting).mockImplementation(async () => null);
  vi.mocked(api.setSetting).mockClear();
});

/** The hint under a fresh query client; `findByRole` waits for the
 * dismissed-setting lookup to settle. */
function mount(ui: ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

const ride = (id: string, ftp: number, device: string | null = "fenix 7"): PreviousPower => ({
  activity_id: id,
  start_time: `2026-09-${id}T08:00:00+00:00`,
  threshold_power_w: ftp,
  source_device: device,
});
const fenix = [ride("13", 238), ride("12", 238), ride("10", 238)];

describe("recentFtp", () => {
  it("is the majority value of the recent rides, the newest winning a tie, with that ride's device", () => {
    expect(recentFtp(fenix)).toEqual({ ftp: 238, device: "fenix 7" });
    // One stale Edge ride among fenix rides does not move the majority.
    expect(recentFtp([ride("16", 200, "Edge 840"), ride("13", 238), ride("12", 238)])).toEqual({ ftp: 238, device: "fenix 7" });
    // A deliberate change: newest wins the tie, and after two rides it is the majority.
    expect(recentFtp([ride("20", 250), ride("13", 238), ride("12", 238)])?.ftp).toBe(238);
    expect(recentFtp([ride("22", 250), ride("20", 250), ride("13", 238)])?.ftp).toBe(250);
    expect(recentFtp([ride("20", 250, "Edge 840"), ride("13", 238)])).toEqual({ ftp: 250, device: "Edge 840" });
    expect(recentFtp([])).toBeNull();
  });
});

describe("ftpMismatch", () => {
  it("speaks only when both sides exist and differ by a watt or more", () => {
    expect(ftpMismatch({ threshold_power_w: 200, source_device: "Edge 840" }, fenix)).toEqual({
      thisFtp: 200,
      recentFtp: 238,
      thisDevice: "Edge 840",
      recentDevice: "fenix 7",
    });
    expect(ftpMismatch({ threshold_power_w: null, source_device: null }, fenix)).toBeNull();
    expect(ftpMismatch({ threshold_power_w: 200, source_device: null }, [])).toBeNull();
    expect(ftpMismatch({ threshold_power_w: 200, source_device: null }, null)).toBeNull();
    expect(ftpMismatch({ threshold_power_w: 238, source_device: null }, fenix)).toBeNull();
    expect(ftpMismatch({ threshold_power_w: 238.4, source_device: null }, fenix)).toBeNull();
    // The ride after the stale one is not told to correct itself.
    expect(ftpMismatch({ threshold_power_w: 238, source_device: "fenix 7" }, [ride("16", 200, "Edge 840"), ride("13", 238), ride("12", 238)])).toBeNull();
  });
});

describe("FtpMismatchHint", () => {
  it("names both FTPs and devices, and the button leads to the correction", async () => {
    const onCorrect = vi.fn();
    const { findByRole, getByText } = mount(
      <FtpMismatchHint activity={{ id: "x", threshold_power_w: 200, source_device: "Edge 840" }} recent={fenix} onCorrect={onCorrect} />,
    );
    const note = await findByRole("note");
    expect(note.textContent).toBe("Recorded with FTP 200 W on Edge 840 — your recent rides used 238 W (fenix 7).Correct FTP");
    fireEvent.click(getByText("Correct FTP"));
    expect(onCorrect).toHaveBeenCalledTimes(1);
  });

  it("renders nothing when the FTPs match, and no device clauses when devices are unknown", async () => {
    const { container } = mount(
      <FtpMismatchHint activity={{ id: "x", threshold_power_w: 238, source_device: null }} recent={fenix} onCorrect={() => {}} />,
    );
    await new Promise((r) => setTimeout(r, 20));
    expect(container.innerHTML).toBe("");
    const bare = mount(
      <FtpMismatchHint
        activity={{ id: "y", threshold_power_w: 200, source_device: null }}
        recent={[ride("13", 238, null)]}
        onCorrect={() => {}}
      />,
    );
    expect((await bare.findByRole("note")).textContent).toBe("Recorded with FTP 200 W — your recent rides used 238 W.Correct FTP");
  });

  it("the cross hides the hint at once and remembers it in the vault per activity", async () => {
    const { findByRole, getByLabelText, queryByRole } = mount(
      <FtpMismatchHint activity={{ id: "act-7", threshold_power_w: 200, source_device: null }} recent={fenix} onCorrect={() => {}} />,
    );
    await findByRole("note");
    fireEvent.click(getByLabelText("Dismiss"));
    expect(queryByRole("note")).toBeNull();
    expect(api.setSetting).toHaveBeenCalledWith(ftpHintDismissedKey("act-7"), "1");
    // A hint dismissed earlier never shows, not even for a frame.
    vi.mocked(api.getSetting).mockImplementation(async (key: string) => (key === ftpHintDismissedKey("act-7") ? "1" : null));
    const again = mount(
      <FtpMismatchHint activity={{ id: "act-7", threshold_power_w: 200, source_device: null }} recent={fenix} onCorrect={() => {}} />,
    );
    await waitFor(() => expect(api.getSetting).toHaveBeenCalledWith(ftpHintDismissedKey("act-7")));
    await new Promise((r) => setTimeout(r, 20));
    expect(again.queryByRole("note")).toBeNull();
  });
});
