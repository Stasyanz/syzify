// @vitest-environment happy-dom
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, cleanup, fireEvent, waitFor, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const { importFiles, open, addToast } = vi.hoisted(() => ({
  importFiles: vi.fn(),
  open: vi.fn(),
  addToast: vi.fn(),
}));
vi.mock("../../lib/tauri", () => ({ api: { importFiles } }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open }));
vi.mock("../../stores/toastStore", () => ({
  useToastStore: (sel: (s: { addToast: typeof addToast }) => unknown) => sel({ addToast }),
}));

import { ImportDialog } from "./ImportDialog";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

function mount(variant: "icon" | "button" = "button") {
  const qc = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <ImportDialog variant={variant} />
    </QueryClientProvider>,
  );
}

describe("ImportDialog", () => {
  it("imports what the file dialog picked and sums it up", async () => {
    open.mockResolvedValueOnce(["/a/ride.fit"]);
    importFiles.mockResolvedValueOnce({
      imported: 1,
      skipped: 0,
      failed: [],
      monitoring_files: 0,
      monitoring_days: 0,
      monitoring_range: null,
      monitoring_night: false,
    });
    mount("icon");
    fireEvent.click(screen.getByLabelText("Import workout files"));
    await waitFor(() => expect(importFiles).toHaveBeenCalledWith(["/a/ride.fit"]));
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("success", expect.stringContaining("1")));
  });

  it("does nothing when the dialog is cancelled", async () => {
    open.mockResolvedValueOnce(null);
    mount();
    fireEvent.click(screen.getByRole("button"));
    await waitFor(() => expect(open).toHaveBeenCalledTimes(1));
    expect(importFiles).not.toHaveBeenCalled();
  });

  /// A Tauri command refuses with a bare string, not an Error: the toast
  /// must carry its words, not "undefined" (#174).
  it("says a refused import in the backend's words", async () => {
    open.mockResolvedValueOnce(["/a/ride.fit"]);
    importFiles.mockRejectedValueOnce("The vault is locked");
    mount();
    fireEvent.click(screen.getByRole("button"));
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("error", "Import failed: The vault is locked"));
  });
});
