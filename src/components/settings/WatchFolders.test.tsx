// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
import { useToastStore } from "../../stores/toastStore";
import { open } from "@tauri-apps/plugin-dialog";

const garmin = { id: 1, path: "/Volumes/GARMIN/GARMIN/Activity" };
let qc: QueryClient;
function renderIt() {
  qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <WatchFolders />
    </QueryClientProvider>,
  );
}

describe("WatchFolders", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.getWatchFolders).mockResolvedValue([garmin]);
    vi.mocked(api.getSetting).mockResolvedValue("ask");
    vi.mocked(api.restartWatcher).mockResolvedValue(undefined);
  });
  afterEach(cleanup);

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

  it("flips auto-import and re-reads the setting", async () => {
    vi.mocked(api.setSetting).mockResolvedValue(undefined);
    vi.mocked(api.getSetting).mockResolvedValueOnce("ask").mockResolvedValue("auto");
    renderIt();
    await waitFor(() => expect(screen.getByLabelText("Auto-import new files")).toBeTruthy());
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
    await waitFor(() => expect(screen.getByText(garmin.path)).toBeTruthy());
    fireEvent.click(screen.getByRole("button", { name: /Preview/ }));
    await waitFor(() => expect(screen.getByTestId("scan-preview")).toBeTruthy());
    expect(screen.getByText("2 files found,")).toBeTruthy();
    expect(screen.getByText("1 new")).toBeTruthy();
    expect(screen.getAllByText("IMPORTED")).toHaveLength(1);
    expect(screen.getAllByText("NEW")).toHaveLength(1);

    fireEvent.click(screen.getByRole("button", { name: "Import All New" }));
    await waitFor(() => expect(api.scanWatchFolders).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(useToastStore.getState().addToast).toHaveBeenCalledWith(expect.any(String), expect.stringMatching(/Imported 1/)),
    );
    // The preview is stale once an import ran.
    expect(screen.queryByTestId("scan-preview")).toBeNull();
    expect(qc.getQueryState(["activities"])?.isInvalidated ?? true).toBe(true);
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
});
