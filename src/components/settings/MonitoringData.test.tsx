// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { api } from "../../lib/tauri";
import { confirmDialog } from "../../stores/confirmStore";
import { useToastStore } from "../../stores/toastStore";
import {
  MonitoringData,
  deleteMessage,
  deleteToast,
  describeSummary,
  rangeProblem,
} from "./MonitoringData";

vi.mock("../../lib/tauri", () => ({
  api: { getMonitoringSummary: vi.fn(), deleteMonitoringRange: vi.fn() },
}));
vi.mock("../../stores/confirmStore", () => ({ confirmDialog: vi.fn() }));

const mocked = vi.mocked(api);
const summary = { days: 79, first_date: "2026-06-19", last_date: "2026-09-05", files: 185 };

function renderBlock() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MonitoringData />
    </QueryClientProvider>,
  );
}

afterEach(cleanup);
beforeEach(() => {
  vi.clearAllMocks();
  useToastStore.setState({ toasts: [] });
});

describe("wording", () => {
  it("describes what is stored", () => {
    expect(describeSummary(summary)).toBe(
      "79 days of Garmin monitoring (Jun 19, 2026 – Sep 5, 2026) · 185 files",
    );
    expect(describeSummary({ days: 1, first_date: "2026-09-05", last_date: "2026-09-05", files: 1 })).toBe(
      "1 day of Garmin monitoring (Sep 5, 2026) · 1 file",
    );
    expect(describeSummary({ days: 0, first_date: null, last_date: null, files: 0 })).toBe(
      "No Garmin monitoring data stored",
    );
  });

  it("refuses a half-filled or inverted range", () => {
    expect(rangeProblem("", "2026-09-05")).toBe("Pick both dates");
    expect(rangeProblem("2026-09-06", "2026-09-05")).toBe("The end date is before the start date");
    expect(rangeProblem("2026-09-05", "2026-09-05")).toBeNull();
  });

  it("never folds a refused removal into success", () => {
    expect(deleteToast({ days: 3, files: 2, failed: 0, error: null })).toEqual([
      "success",
      "Deleted 3 days of monitoring and 2 files",
    ]);
    expect(deleteToast({ days: 0, files: 0, failed: 0, error: null })).toEqual([
      "info",
      "No monitoring data in that range",
    ]);
    // Files swept from an earlier interrupted delete count even with no days.
    expect(deleteToast({ days: 0, files: 1, failed: 0, error: null })[1]).toBe(
      "Deleted 0 days of monitoring and 1 file",
    );
    const [level, text] = deleteToast({ days: 1, files: 0, failed: 1, error: "raw/x.fit: busy" });
    expect(level).toBe("warning");
    expect(text).toBe(
      "Deleted 1 day of monitoring and 0 files · 1 file could not be removed (raw/x.fit: busy) — try again later",
    );
    expect(deleteToast({ days: 1, files: 0, failed: 2, error: null })[1]).toContain("unknown error");
  });

  it("names the span in the confirmation", () => {
    expect(deleteMessage("2026-09-05", "2026-09-05")).toContain("on Sep 5, 2026?");
    expect(deleteMessage("2026-08-01", "2026-09-05")).toContain("from Aug 1, 2026 to Sep 5, 2026?");
    expect(deleteMessage("2026-08-01", "2026-09-05")).toContain("imported again");
  });
});

describe("MonitoringData", () => {
  it("seeds the range with the stored span and deletes after a danger confirm", async () => {
    mocked.getMonitoringSummary.mockResolvedValue(summary);
    mocked.deleteMonitoringRange.mockResolvedValue({ days: 3, files: 2, failed: 0, error: null });
    vi.mocked(confirmDialog).mockResolvedValue(true);
    renderBlock();
    await screen.findByText(/79 days of Garmin monitoring/);
    const from = screen.getByLabelText("Delete from") as HTMLInputElement;
    const to = screen.getByLabelText("Delete to") as HTMLInputElement;
    expect([from.value, to.value]).toEqual(["2026-06-19", "2026-09-05"]);

    fireEvent.change(from, { target: { value: "2026-09-03" } });
    fireEvent.click(screen.getByText("Delete…"));
    await waitFor(() => expect(mocked.deleteMonitoringRange).toHaveBeenCalledWith("2026-09-03", "2026-09-05"));
    expect(vi.mocked(confirmDialog)).toHaveBeenCalledWith(
      expect.objectContaining({ danger: true, confirmLabel: "Delete" }),
    );
    expect(vi.mocked(confirmDialog).mock.calls[0][0].message).toContain("from Sep 3, 2026 to Sep 5, 2026");
    await waitFor(() =>
      expect(useToastStore.getState().toasts.map((t) => t.message)).toEqual([
        "Deleted 3 days of monitoring and 2 files",
      ]),
    );
  });

  it("does nothing when the confirmation is declined", async () => {
    mocked.getMonitoringSummary.mockResolvedValue(summary);
    vi.mocked(confirmDialog).mockResolvedValue(false);
    renderBlock();
    await screen.findByText(/79 days/);
    fireEvent.click(screen.getByText("Delete…"));
    await waitFor(() => expect(confirmDialog).toHaveBeenCalled());
    expect(mocked.deleteMonitoringRange).not.toHaveBeenCalled();
  });

  it("blocks an inverted range and says why", async () => {
    mocked.getMonitoringSummary.mockResolvedValue(summary);
    renderBlock();
    await screen.findByText(/79 days/);
    fireEvent.change(screen.getByLabelText("Delete to"), { target: { value: "2026-06-01" } });
    screen.getByText("The end date is before the start date");
    expect((screen.getByText("Delete…").closest("button") as HTMLButtonElement).disabled).toBe(true);
    expect(confirmDialog).not.toHaveBeenCalled();
  });

  it("reports an empty range and a backend failure", async () => {
    mocked.getMonitoringSummary.mockResolvedValue(summary);
    vi.mocked(confirmDialog).mockResolvedValue(true);
    mocked.deleteMonitoringRange.mockResolvedValueOnce({ days: 0, files: 0, failed: 0, error: null });
    renderBlock();
    await screen.findByText(/79 days/);
    fireEvent.click(screen.getByText("Delete…"));
    await waitFor(() =>
      expect(useToastStore.getState().toasts.map((t) => t.message)).toEqual([
        "No monitoring data in that range",
      ]),
    );
    // The stored span did not change (same summary): the range stays put.
    expect((screen.getByLabelText("Delete from") as HTMLInputElement).value).toBe("2026-06-19");
    mocked.deleteMonitoringRange.mockRejectedValueOnce(new Error("locked"));
    fireEvent.click(screen.getByText("Delete…"));
    await waitFor(() =>
      expect(useToastStore.getState().toasts.map((t) => t.type)).toEqual(["info", "error"]),
    );
    expect(useToastStore.getState().toasts[1].message).toContain("locked");
  });

  it("re-seeds the range only when the stored span itself changes", async () => {
    mocked.getMonitoringSummary.mockResolvedValueOnce(summary);
    vi.mocked(confirmDialog).mockResolvedValue(true);
    mocked.deleteMonitoringRange.mockResolvedValue({ days: 5, files: 5, failed: 0, error: null });
    renderBlock();
    await screen.findByText(/79 days/);
    const from = screen.getByLabelText("Delete from") as HTMLInputElement;
    const to = screen.getByLabelText("Delete to") as HTMLInputElement;
    // The user narrows the range to the tail, deletes it, and the refetch
    // reports a shorter span: both inputs follow the new span.
    fireEvent.change(from, { target: { value: "2026-09-01" } });
    mocked.getMonitoringSummary.mockResolvedValueOnce({
      ...summary,
      days: 74,
      last_date: "2026-08-31",
      files: 180,
    });
    fireEvent.click(screen.getByText("Delete…"));
    await screen.findByText(/74 days/);
    await waitFor(() => expect([from.value, to.value]).toEqual(["2026-06-19", "2026-08-31"]));
  });

  it("shows Deleting… and refuses a second click while the request runs", async () => {
    mocked.getMonitoringSummary.mockResolvedValue(summary);
    vi.mocked(confirmDialog).mockResolvedValue(true);
    let finish: (v: { days: number; files: number; failed: number; error: null }) => void = () => {};
    mocked.deleteMonitoringRange.mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    renderBlock();
    await screen.findByText(/79 days/);
    fireEvent.click(screen.getByText("Delete…"));
    const busy = await screen.findByText("Deleting…");
    expect((busy.closest("button") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(busy);
    expect(mocked.deleteMonitoringRange).toHaveBeenCalledTimes(1);
    finish({ days: 1, files: 1, failed: 0, error: null });
    await screen.findByText("Delete…");
  });

  it("hides the range and disables the button when nothing is stored", async () => {
    mocked.getMonitoringSummary.mockResolvedValue({
      days: 0,
      first_date: null,
      last_date: null,
      files: 0,
    });
    renderBlock();
    await screen.findByText("No Garmin monitoring data stored");
    expect(screen.queryByLabelText("Delete from")).toBeNull();
    expect((screen.getByText("Delete…").closest("button") as HTMLButtonElement).disabled).toBe(true);
  });
});
