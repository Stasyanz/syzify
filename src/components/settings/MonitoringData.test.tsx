// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { api } from "../../lib/tauri";
import { confirmDialog } from "../../stores/confirmStore";
import { useToastStore } from "../../stores/toastStore";
import { t, translate, useLanguageStore, type Translator } from "../../lib/i18n";
import { useBusyStore } from "../../stores/busyStore";
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

afterEach(() => {
  cleanup();
  useLanguageStore.getState().setPref("system");
  useBusyStore.setState({ running: {}, lastError: {} });
});
beforeEach(() => {
  vi.clearAllMocks();
  useToastStore.setState({ toasts: [] });
});

describe("wording", () => {
  it("describes what is stored", () => {
    expect(describeSummary(summary, t)).toBe(
      "79 days of Garmin monitoring (Jun 19, 2026 – Sep 5, 2026) · 185 files",
    );
    expect(describeSummary({ days: 1, first_date: "2026-09-05", last_date: "2026-09-05", files: 1 }, t)).toBe(
      "1 day of Garmin monitoring (Sep 5, 2026) · 1 file",
    );
    expect(describeSummary({ days: 0, first_date: null, last_date: null, files: 0 }, t)).toBe(
      "No Garmin monitoring data stored",
    );
  });

  it("dates the stored range in the interface language (#232)", () => {
    useLanguageStore.getState().setPref("it");
    expect(describeSummary(summary, t)).toContain("(19 giu 2026 – 5 set 2026)");
  });

  it("refuses a half-filled or inverted range", () => {
    expect(rangeProblem("", "2026-09-05", t)).toBe("Pick both dates");
    expect(rangeProblem("2026-09-06", "2026-09-05", t)).toBe("The end date is before the start date");
    expect(rangeProblem("2026-09-05", "2026-09-05", t)).toBeNull();
  });

  it("never folds a refused removal into success", () => {
    expect(deleteToast({ days: 3, files: 2, failed: 0, error: null }, t)).toEqual([
      "success",
      "Deleted 3 days of monitoring and 2 files",
    ]);
    expect(deleteToast({ days: 0, files: 0, failed: 0, error: null }, t)).toEqual([
      "info",
      "No monitoring data in that range",
    ]);
    // Files swept from an earlier interrupted delete count even with no days.
    expect(deleteToast({ days: 0, files: 1, failed: 0, error: null }, t)[1]).toBe(
      "Deleted 0 days of monitoring and 1 file",
    );
    const [level, text] = deleteToast({ days: 1, files: 0, failed: 1, error: "raw/x.fit: busy" }, t);
    expect(level).toBe("warning");
    expect(text).toBe(
      "Deleted 1 day of monitoring and 0 files · 1 file could not be removed (raw/x.fit: busy) — try again later",
    );
    expect(deleteToast({ days: 1, files: 0, failed: 2, error: null }, t)[1]).toContain(
      "2 files could not be removed (unknown error)",
    );
  });

  it("names the span in the confirmation", () => {
    expect(deleteMessage("2026-09-05", "2026-09-05", t)).toContain("on Sep 5, 2026?");
    expect(deleteMessage("2026-08-01", "2026-09-05", t)).toContain("from Aug 1, 2026 to Sep 5, 2026?");
    expect(deleteMessage("2026-08-01", "2026-09-05", t)).toContain("imported again");
  });
});

describe("wording in Russian", () => {
  const ru: Translator = (key, ...args) => translate("ru", key, ...args);

  it("counts days and files with the forms of the number", () => {
    useLanguageStore.getState().setPref("ru");
    expect(describeSummary(summary, ru)).toMatch(/^Мониторинг Garmin: 79 дней \(19 июн.+\) · 185 файлов$/);
    expect(describeSummary({ days: 2, first_date: "2026-09-04", last_date: "2026-09-05", files: 1 }, ru)).toMatch(
      /^Мониторинг Garmin: 2 дня \(.*\) · 1 файл$/,
    );
    expect(deleteToast({ days: 21, files: 3, failed: 0, error: null }, ru)[1]).toBe(
      "Удалено: мониторинг за 21 день и 3 файла",
    );
    expect(deleteToast({ days: 5, files: 0, failed: 5, error: null }, ru)[1]).toBe(
      "Удалено: мониторинг за 5 дней и 0 файлов · 5 файлов удалить не удалось (неизвестная ошибка) — попробуйте позже",
    );
  });

  it("refuses a bad range and asks before a delete in Russian", () => {
    expect(rangeProblem("", "2026-09-05", ru)).toBe("Выберите обе даты");
    expect(rangeProblem("2026-09-06", "2026-09-05", ru)).toBe("Конечная дата раньше начальной");
    expect(deleteMessage("2026-09-05", "2026-09-05", ru)).toMatch(/^Удалить данные мониторинга за .+\?\n\n/);
    expect(deleteMessage("2026-08-01", "2026-09-05", ru)).toMatch(/^Удалить данные мониторинга с .+ по .+\?/);
    expect(describeSummary({ days: 0, first_date: null, last_date: null, files: 0 }, ru)).toBe(
      "Данных мониторинга Garmin нет",
    );
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
    // The seeding effect runs after the summary renders: wait for it.
    await waitFor(() => expect([from.value, to.value]).toEqual(["2026-06-19", "2026-09-05"]));

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
    expect(useToastStore.getState().toasts[1].message).toBe("Delete failed: locked");
    // The backend's own refusal is a bare string: the same words (#182).
    mocked.deleteMonitoringRange.mockRejectedValueOnce("vault busy");
    fireEvent.click(screen.getByText("Delete…"));
    await waitFor(() => expect(useToastStore.getState().toasts.slice(-1)[0]?.message).toBe("Delete failed: vault busy"));
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

  it("keeps Deleting… across a remount while the request runs (#235)", async () => {
    mocked.getMonitoringSummary.mockResolvedValue(summary);
    vi.mocked(confirmDialog).mockResolvedValue(true);
    let finish: (v: { days: number; files: number; failed: number; error: null }) => void = () => {};
    mocked.deleteMonitoringRange.mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    const first = renderBlock();
    await screen.findByText(/79 days/);
    fireEvent.click(screen.getByText("Delete…"));
    await screen.findByText("Deleting…");
    // A language change remounts the app: the new block still says so.
    first.unmount();
    renderBlock();
    const busy = await screen.findByText("Deleting…");
    expect((busy.closest("button") as HTMLButtonElement).disabled).toBe(true);
    expect(useBusyStore.getState().running).toEqual({ monitoringDelete: null });
    finish({ days: 1, files: 1, failed: 0, error: null });
    await waitFor(() => expect(useBusyStore.getState().running).toEqual({}));
    await screen.findByText("Delete…");
  });

  it("speaks Russian", async () => {
    useLanguageStore.getState().setPref("ru");
    mocked.getMonitoringSummary.mockResolvedValue(summary);
    vi.mocked(confirmDialog).mockResolvedValue(true);
    mocked.deleteMonitoringRange.mockResolvedValue({ days: 1, files: 1, failed: 0, error: null });
    renderBlock();
    await screen.findByText(/^Мониторинг Garmin: 79 дней/);
    screen.getByText("Данные мониторинга");
    screen.getByLabelText("Удалить с");
    screen.getByLabelText("Удалить по");
    fireEvent.click(screen.getByText("Удалить…"));
    await waitFor(() =>
      expect(useToastStore.getState().toasts.map((x) => x.message)).toEqual(["Удалено: мониторинг за 1 день и 1 файл"]),
    );
    expect(vi.mocked(confirmDialog)).toHaveBeenCalledWith(
      expect.objectContaining({ title: "Удаление данных мониторинга", confirmLabel: "Удалить" }),
    );
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
