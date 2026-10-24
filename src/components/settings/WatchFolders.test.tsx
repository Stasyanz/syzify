// @vitest-environment happy-dom
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ImportResult } from "../../lib/types";

vi.mock("../../lib/tauri", () => ({
  api: {
    getWatchFolders: vi.fn(),
    getSetting: vi.fn(),
    setSetting: vi.fn(),
    addWatchFolder: vi.fn(),
    removeWatchFolder: vi.fn(),
    previewWatchFolders: vi.fn(),
    scanWatchFolders: vi.fn(),
    restartWatcher: vi.fn(),
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../../stores/toastStore", () => {
  const state = { addToast: vi.fn(() => "t1"), updateToast: vi.fn(), removeToast: vi.fn() };
  const useToastStore = Object.assign((sel: (s: typeof state) => unknown) => sel(state), {
    getState: () => state,
  });
  return { useToastStore };
});

import { WatchFolders } from "./WatchFolders";
import { api } from "../../lib/tauri";
import { commandError } from "../../test/commandError";
import { useToastStore } from "../../stores/toastStore";
import { open } from "@tauri-apps/plugin-dialog";
import { useLanguageStore } from "../../lib/i18n";
import { useBusyStore } from "../../stores/busyStore";
import { useWatchPreviewStore } from "../../stores/watchPreviewStore";

const garmin = { id: 1, path: "/Volumes/GARMIN/GARMIN/Activity" };
let qc: QueryClient;
function renderIt({ strict = false } = {}) {
  qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const tree = (
    <QueryClientProvider client={qc}>
      <WatchFolders />
    </QueryClientProvider>
  );
  return render(strict ? <StrictMode>{tree}</StrictMode> : tree);
}

describe("WatchFolders", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.getWatchFolders).mockResolvedValue([garmin]);
    vi.mocked(api.getSetting).mockResolvedValue("ask");
    vi.mocked(api.restartWatcher).mockResolvedValue(undefined);
  });
  afterEach(() => {
    cleanup();
    useLanguageStore.getState().setPref("system");
    useBusyStore.setState({ running: {}, lastError: {} });
    useWatchPreviewStore.setState({ preview: null });
  });

  it("lists the watched folders and the ask-first switch", async () => {
    renderIt();
    await waitFor(() => expect(screen.getByText(garmin.path)).toBeTruthy());
    expect(screen.getByText(/You are asked before/)).toBeTruthy();
    expect(screen.getByLabelText("Auto-import new files").getAttribute("aria-checked")).toBe("false");
  });

  it("removes a folder and restarts the watcher", async () => {
    vi.mocked(api.removeWatchFolder).mockResolvedValue(undefined);
    renderIt();
    await waitFor(() => expect(screen.getByText(garmin.path)).toBeTruthy());
    fireEvent.click(screen.getByLabelText(`Remove ${garmin.path}`));
    await waitFor(() => expect(api.removeWatchFolder).toHaveBeenCalledWith(1));
    await waitFor(() => expect(api.restartWatcher).toHaveBeenCalledTimes(1));
  });

  it("adds a folder picked in the dialog and ignores a cancelled dialog", async () => {
    vi.mocked(open).mockResolvedValueOnce(null).mockResolvedValueOnce("/Users/me/rides");
    vi.mocked(api.addWatchFolder).mockResolvedValue({ id: 3, path: "/Users/me/rides" });
    renderIt();
    await waitFor(() => expect(screen.getByText(garmin.path)).toBeTruthy());
    fireEvent.click(screen.getByRole("button", { name: /Add Folder/ }));
    await waitFor(() => expect(open).toHaveBeenCalledTimes(1));
    expect(api.addWatchFolder).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: /Add Folder/ }));
    await waitFor(() => expect(api.addWatchFolder).toHaveBeenCalledWith("/Users/me/rides"));
    await waitFor(() => expect(api.restartWatcher).toHaveBeenCalledTimes(1));
  });

  it("warns when the watcher cannot restart after a folder change, and when a setting write fails", async () => {
    vi.mocked(api.removeWatchFolder).mockResolvedValue(undefined);
    vi.mocked(api.restartWatcher).mockRejectedValue(new Error("inotify limit"));
    vi.mocked(api.setSetting).mockRejectedValue(new Error("db closed"));
    renderIt();
    await waitFor(() => expect(screen.getByText(garmin.path)).toBeTruthy());
    fireEvent.click(screen.getByLabelText(`Remove ${garmin.path}`));
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith("warning", expect.stringMatching(/could not restart.*inotify limit/)),
    );
    fireEvent.click(screen.getByLabelText("Auto-import new files"));
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith("error", expect.stringMatching(/auto-import.*db closed/)),
    );
  });

  it("flips auto-import and re-reads the setting, ignoring a click before the value is known", async () => {
    vi.mocked(api.setSetting).mockResolvedValue(undefined);
    let settle!: (v: string) => void;
    vi.mocked(api.getSetting).mockReturnValueOnce(new Promise((r) => (settle = r))).mockResolvedValue("auto");
    renderIt();
    // Still loading: the row says so and a click writes nothing.
    await waitFor(() => expect(screen.getByLabelText("Auto-import new files")).toBeTruthy());
    expect(screen.getByText("…")).toBeTruthy();
    fireEvent.click(screen.getByLabelText("Auto-import new files"));
    expect(api.setSetting).not.toHaveBeenCalled();
    settle("ask");
    await waitFor(() => expect(screen.getByText(/You are asked before/)).toBeTruthy());
    fireEvent.click(screen.getByLabelText("Auto-import new files"));
    await waitFor(() => expect(api.setSetting).toHaveBeenCalledWith("watch_auto_import", "auto"));
    // The invalidation refetches; the row follows the stored value.
    await waitFor(() => expect(screen.getByText(/imported right away/)).toBeTruthy());
    expect(screen.getByLabelText("Auto-import new files").getAttribute("aria-checked")).toBe("true");
  });

  it("previews what is new and imports it all through the scan", async () => {
    vi.mocked(api.previewWatchFolders).mockResolvedValue({
      total_files: 2,
      new_files: 1,
      folders: [
        {
          folder: garmin.path,
          files: [
            { path: "/a/1.fit", filename: "1.fit", is_new: false },
            { path: "/a/2.fit", filename: "2.fit", is_new: true },
          ],
        },
      ],
    });
    const imported = { imported: 1, skipped: 1, failed: [], monitoring_files: 0, monitoring_days: 0 } as unknown as ImportResult;
    vi.mocked(api.scanWatchFolders).mockResolvedValue({ new_files: ["/a/1.fit", "/a/2.fit"], import_result: imported });
    renderIt();
    // A cached library, so the invalidation after the import is observable.
    qc.setQueryData(["activities"], []);
    await waitFor(() => expect(screen.getByText(garmin.path)).toBeTruthy());
    fireEvent.click(screen.getByRole("button", { name: /Preview/ }));
    await waitFor(() => expect(screen.getByTestId("scan-preview")).toBeTruthy());
    expect(screen.getByText("2 files found,")).toBeTruthy();
    expect(screen.getByText("1 new")).toBeTruthy();
    expect(screen.getAllByText("IMPORTED")).toHaveLength(1);
    expect(screen.getAllByText("NEW")).toHaveLength(1);
    // The list is a bounded, scrolling box; the panel can be dismissed.
    expect(screen.getByTestId("scan-preview").querySelector(".overflow-y-auto")).toBeTruthy();
    fireEvent.click(screen.getByLabelText("Close preview"));
    expect(screen.queryByTestId("scan-preview")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Preview/ }));
    await waitFor(() => expect(screen.getByTestId("scan-preview")).toBeTruthy());

    fireEvent.click(screen.getByRole("button", { name: "Import All New" }));
    await waitFor(() => expect(api.scanWatchFolders).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith(expect.any(String), expect.stringMatching(/Imported 1/)),
    );
    // The preview is stale once an import ran.
    expect(screen.queryByTestId("scan-preview")).toBeNull();
    expect(qc.getQueryState(["activities"])?.isInvalidated).toBe(true);
  });

  it("says so when a scan finds nothing, and keeps Preview / Import Now off without folders", async () => {
    vi.mocked(api.scanWatchFolders).mockResolvedValue({ new_files: [], import_result: null });
    renderIt();
    await waitFor(() => expect(screen.getByText(garmin.path)).toBeTruthy());
    fireEvent.click(screen.getByRole("button", { name: /Import Now/ }));
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith("info", expect.stringMatching(/No new files/)),
    );
    cleanup();
    vi.mocked(api.getWatchFolders).mockResolvedValue([]);
    renderIt();
    await waitFor(() => expect(api.getWatchFolders).toHaveBeenCalledTimes(2));
    expect((screen.getByRole("button", { name: /Preview/ }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: /Import Now/ }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByTestId("watch-folders")).toBeNull();
  });

  it("reports every failed call as a toast", async () => {
    vi.mocked(open).mockResolvedValue("/Users/me/rides");
    vi.mocked(api.addWatchFolder).mockRejectedValue(new Error("not a directory"));
    vi.mocked(api.removeWatchFolder).mockRejectedValue(new Error("gone"));
    vi.mocked(api.previewWatchFolders).mockRejectedValue(new Error("walk failed"));
    vi.mocked(api.scanWatchFolders).mockRejectedValue(new Error("vault locked"));
    renderIt();
    await waitFor(() => expect(screen.getByText(garmin.path)).toBeTruthy());
    const toast = useToastStore.getState().addToast;
    fireEvent.click(screen.getByRole("button", { name: /Add Folder/ }));
    await waitFor(() => expect(toast).toHaveBeenCalledWith("error", expect.stringMatching(/Could not add folder.*not a directory/)));
    fireEvent.click(screen.getByLabelText(`Remove ${garmin.path}`));
    await waitFor(() => expect(toast).toHaveBeenCalledWith("error", expect.stringMatching(/Could not remove folder.*gone/)));
    fireEvent.click(screen.getByRole("button", { name: /Preview/ }));
    await waitFor(() => expect(toast).toHaveBeenCalledWith("error", expect.stringMatching(/Preview failed.*walk failed/)));
    fireEvent.click(screen.getByRole("button", { name: /Import Now/ }));
    await waitFor(() => expect(toast).toHaveBeenCalledWith("error", "Import failed: vault locked"));
    // A command's refusal reads the same, and an Error never shows "Error:" (#182).
    vi.mocked(api.scanWatchFolders).mockRejectedValue(commandError(null, "vault locked by a backup"));
    fireEvent.click(screen.getByRole("button", { name: /Import Now/ }));
    await waitFor(() => expect(toast).toHaveBeenCalledWith("error", "Import failed: vault locked by a backup"));
    expect(toast).not.toHaveBeenCalledWith("error", expect.stringMatching(/Error:/));
  });

  it("keeps Importing… and both buttons locked across a remount while a scan runs (#235)", async () => {
    let finish!: (v: { new_files: string[]; import_result: null }) => void;
    vi.mocked(api.scanWatchFolders).mockReturnValue(new Promise((r) => (finish = r)));
    const first = renderIt();
    await waitFor(() => expect(screen.getByText(garmin.path)).toBeTruthy());
    fireEvent.click(screen.getByRole("button", { name: /Import Now/ }));
    await screen.findByRole("button", { name: /Importing…/ });
    first.unmount();
    renderIt();
    await waitFor(() => expect(screen.getByText(garmin.path)).toBeTruthy());
    expect((screen.getByRole("button", { name: /Importing…/ }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: /Preview/ }) as HTMLButtonElement).disabled).toBe(true);
    await act(async () => finish({ new_files: [], import_result: null }));
    await screen.findByRole("button", { name: /Import Now/ });
    expect(useBusyStore.getState().running).toEqual({});
  });

  // Under StrictMode, as main.tsx renders the app: its dev double-mount runs
  // a cleanup that must not wipe the preview the return shows.
  it("shows a preview that finished while the section was away, under StrictMode too (#235)", async () => {
    const result = {
      total_files: 3,
      new_files: 2,
      folders: [{ folder: garmin.path, files: [{ path: "/a/1.fit", filename: "1.fit", is_new: true }] }],
    };
    let finish!: (v: typeof result) => void;
    vi.mocked(api.previewWatchFolders).mockReturnValue(new Promise((r) => (finish = r)));
    const first = renderIt({ strict: true });
    await waitFor(() => expect(screen.getByText(garmin.path)).toBeTruthy());
    fireEvent.click(screen.getByRole("button", { name: /Preview/ }));
    await screen.findByRole("button", { name: /Scanning…/ });
    // Another tab while the walk runs; it ends meanwhile.
    first.unmount();
    await act(async () => finish(result));
    const back = renderIt({ strict: true });
    await waitFor(() => expect(screen.getByTestId("scan-preview")).toBeTruthy());
    expect(screen.getByText("2 new").parentElement?.textContent).toBe("3 files found, 2 new");
    // Still there after another visit; it goes when the user closes it.
    back.unmount();
    renderIt({ strict: true });
    await waitFor(() => expect(screen.getByTestId("scan-preview")).toBeTruthy());
    fireEvent.click(screen.getByLabelText("Close preview"));
    expect(screen.queryByTestId("scan-preview")).toBeNull();
    expect(useWatchPreviewStore.getState().preview).toBeNull();
  });

  it("hands a scan started from the preview to the Import Now button's Importing…", async () => {
    vi.mocked(api.previewWatchFolders).mockResolvedValue({
      total_files: 1,
      new_files: 1,
      folders: [{ folder: garmin.path, files: [{ path: "/a/1.fit", filename: "1.fit", is_new: true }] }],
    });
    vi.mocked(api.scanWatchFolders).mockReturnValue(new Promise(() => {}));
    renderIt();
    await waitFor(() => expect(screen.getByText(garmin.path)).toBeTruthy());
    fireEvent.click(screen.getByRole("button", { name: /Preview/ }));
    await waitFor(() => expect(screen.getByTestId("scan-preview")).toBeTruthy());
    fireEvent.click(screen.getByRole("button", { name: "Import All New" }));
    const busy = await screen.findByRole("button", { name: "Importing…" });
    expect((busy as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByTestId("scan-preview")).toBeNull();
    expect((screen.getByRole("button", { name: /Preview/ }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("speaks Russian, counting the files found with the forms of the number", async () => {
    useLanguageStore.getState().setPref("ru");
    vi.mocked(api.previewWatchFolders).mockResolvedValue({
      total_files: 5,
      new_files: 2,
      folders: [{ folder: garmin.path, files: [{ path: "/a/1.fit", filename: "1.fit", is_new: true }] }],
    });
    vi.mocked(api.scanWatchFolders).mockResolvedValue({ new_files: [], import_result: null });
    renderIt();
    await waitFor(() => expect(screen.getByText(garmin.path)).toBeTruthy());
    screen.getByText("Папки наблюдения");
    screen.getByText(/^Перед импортом файлов/);
    expect(screen.getByLabelText("Импортировать новые файлы автоматически")).toBeTruthy();
    screen.getByLabelText(`Убрать ${garmin.path}`);
    fireEvent.click(screen.getByRole("button", { name: "Просмотр" }));
    await waitFor(() => expect(screen.getByTestId("scan-preview")).toBeTruthy());
    expect(screen.getByText("2 новых").parentElement?.textContent).toBe("Найдено 5 файлов, 2 новых");
    screen.getByText("НОВЫЙ");
    fireEvent.click(screen.getByRole("button", { name: "Импортировать" }));
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith("info", "В папках наблюдения нет новых файлов."),
    );
  });
});
